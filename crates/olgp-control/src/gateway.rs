//! API HTTP server, pairing endpoints, and TLS-terminated RPC reverse proxy.

use std::fs::File;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lumen_crypto::pairing::{ChannelBinding, DeviceIdentity, PairingCode};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

use crate::auth::TrustedClients;
use crate::config::Config;
use tokio::io::copy_bidirectional;
use crate::discovery::BoxInfo;
use crate::product::ProductState;
use crate::pairing::HostPairing;
use crate::registry::ModelRegistry;
use crate::sched::Scheduler;
use crate::telemetry;

struct GatewayState {
    info: BoxInfo,
    identity: DeviceIdentity,
    pairing: Mutex<Option<HostPairing>>,
    pairing_window: Mutex<Option<std::time::Instant>>,
    pairing_attempts: Mutex<u32>,
    pairing_code: PairingCode,
    engine_port: u16,
    product: Option<ProductState>,
    trusted: Mutex<TrustedClients>,
    scheduler: Mutex<Scheduler>,
    registry: Mutex<ModelRegistry>,
}
fn response(status: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn text_response(status: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

fn error(code: &str, message: &str, status: &str, retryable: bool) -> Vec<u8> {
    response(
        status,
        &json!({"error":{"code":code,"message":message,"retryable":retryable}}).to_string(),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str, length: usize) -> Result<Vec<u8>, String> {
    if text.len() != length * 2 {
        return Err("bad hex length".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "bad hex".into()))
        .collect()
}

async fn read_request(socket: &mut TcpStream) -> Option<(String, String, String)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        let read = socket.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if buffer.len() > 1024 * 1024 {
            return None;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let length = head
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if key.eq_ignore_ascii_case("content-length") {
                value.trim().parse::<usize>().ok()
            } else {
                None
            }
        })
        .unwrap_or(0);
    while buffer.len() < header_end + length {
        let read = socket.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8_lossy(&buffer[header_end..]).into_owned();
    let first = head.lines().next().unwrap_or("").to_owned();
    Some((head, body, first))
}

fn authorized(head: &str, trusted: &TrustedClients) -> bool {
    let Some(token) = head.lines().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        lower
            .strip_prefix("authorization: bearer ")
            .map(|_| line["authorization: bearer ".len()..].trim())
    }) else {
        return false;
    };
    trusted.is_valid(token)
}

async fn proxy_engine(port: u16, head: &str, body: &str) -> Option<Vec<u8>> {
    let mut upstream = TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .ok()?;
    let first = head.lines().next().unwrap_or("");
    let mut request = format!("{first}\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
    for line in head.lines().skip(1) {
        if line.is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("content-length:")
            || lower.starts_with("host:")
            || lower.starts_with("connection:")
        {
            continue;
        }
        request.push_str(line);
        request.push_str("\r\n");
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    upstream.write_all(request.as_bytes()).await.ok()?;
    let mut reply = Vec::new();
    upstream.read_to_end(&mut reply).await.ok()?;
    Some(reply)
}

async fn engine_reachable(port: u16) -> bool {
    TcpStream::connect(SocketAddr::from(([127, 0, 0, 1], port))).await.is_ok()
}

async fn api_connection(mut socket: TcpStream, state: Arc<GatewayState>) {
    let Some((head, body, first)) = read_request(&mut socket).await else {
        return;
    };
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let path = parts.next().unwrap_or("").split('?').next().unwrap_or("").to_owned();
    if path == "/v1/realtime"
        && head.lines().any(|line| line.to_ascii_lowercase().contains("upgrade: websocket"))
    {
        tokio::spawn(crate::voice::serve(socket));
        return;
    }
    if path.starts_with("/v1/speakers/")
        && path.ends_with("/audio:stream")
        && head.lines().any(|line| line.to_ascii_lowercase().contains("upgrade: websocket"))
    {
        tokio::spawn(crate::voice::serve_speaker_audio(socket));
        return;
    }
    let result = handle_request(&method, &path, &head, &body, &state).await;
    let _ = socket.write_all(&result).await;
}

fn sync_runtime_metrics(state: &GatewayState) {
    let scheduler = state.scheduler.lock().unwrap();
    telemetry::set("vram_bytes", scheduler.counters.vram_bytes);
    telemetry::set("ram_bytes", scheduler.counters.ram_bytes);
    telemetry::set("kv_ssd_writes", scheduler.counters.kv_ssd_writes);
    drop(scheduler);
    let registry = state.registry.lock().unwrap();
    telemetry::set("shared_weight_bytes", registry.resident_bytes());
    telemetry::set("kv_sessions", registry.tenant_count());
}

async fn handle_request(
    method: &str,
    path: &str,
    head: &str,
    body: &str,
    state: &GatewayState,
) -> Vec<u8> {
    match (method, path) {
        ("GET", "/v1/health/live") => response(
            "200 OK",
            "{\"ok\":true,\"service\":\"olgp-edge\",\"apiVersion\":1}\n",
        ),
        ("GET", "/v1/capabilities") => response(
            "200 OK",
            &json!({
                "service": "olgp-edge",
                "apiVersion": 1,
                "responses": {"text": true},
                "ready": {"llm": true, "qna": true, "voice": true},
                "box_id": state.info.box_id,
                "gpu": state.info.gpu,
                "vram_mib": state.info.vram_mib,
                "capabilities": state.info.capabilities,
                "product": {
                    "speaker": true,
                    "timers": true,
                    "memos": true,
                    "audioMemory": true,
                    "privateRag": true,
                    "recordings": true,
                    "images": true,
                },
                "realtime": {"voice": true, "chat": true},
                "scheduler": {
                    "vram_bytes": state.scheduler.lock().unwrap().counters.vram_bytes,
                    "sessions": state.scheduler.lock().unwrap().session_count(),
                },
                "sharing": {
                    "shared_weight_bytes": state.registry.lock().unwrap().resident_bytes(),
                    "kv_sessions": state.registry.lock().unwrap().tenant_count(),
                },
                "search": {"policy": "caller-key"},
                "images": {
                    "contractVersion": 3,
                    "operations": {
                        "generate": {"ready": false},
                        "edit": {"ready": false},
                        "face_swap": {"ready": false},
                        "reframe": {"ready": false},
                        "detect": {"ready": false},
                    },
                },
                "memory": {"policy": "opt-in"},
            })
            .to_string(),
        ),
        ("GET", "/metrics") => {
            sync_runtime_metrics(state);
            text_response("200 OK", &telemetry::render())
        }
        ("GET", "/v1/pairing/status") => {
            let window = state.pairing_window.lock().unwrap();
            let open = window.map_or(false, |deadline| std::time::Instant::now() < deadline);
            let attempts = *state.pairing_attempts.lock().unwrap();
            response(
                "200 OK",
                &json!({
                    "box_id": state.info.box_id,
                    "window_open": open,
                    "attempts": attempts,
                    "paired": false,
                })
                .to_string(),
            )
        }
        ("POST", "/v1/pairing") => pairing_request(state, body),
        (_, _) if !authorized(head, &state.trusted.lock().unwrap()) => {
            error("unauthorized", "Device Token required", "401 Unauthorized", false)
        }
        ("GET", "/engine/status") => {
            if engine_reachable(state.engine_port).await {
                response("200 OK", &json!({"service":"olgp-engine","ready":true}).to_string())
            } else {
                error("not_ready", "engine is not ready", "503 Service Unavailable", true)
            }
        }
        ("GET", "/engine/capabilities") => response(
            "200 OK",
            &json!({"service":"olgp-engine","apiVersion":1,"sessions":true}).to_string(),
        ),
        ("GET", "/engine/sessions") => response("200 OK", "{\"sessions\":[]}\n"),
        ("POST", "/v1/chat/completions")
        | ("POST", "/v1/responses")
        | ("POST", "/v1/embeddings")
        | ("POST", "/v1/audio/transcriptions")
        | ("POST", "/v1/audio/speech")
        | ("POST", "/api/chat")
        | ("POST", "/api/generate") => {
            match proxy_engine(state.engine_port, head, body).await {
                Some(reply) => reply,
                None => error(
                    "upstream_unavailable",
                    "engine is not ready",
                    "502 Bad Gateway",
                    true,
                ),
            }
        }
        ("GET", "/v1/models") => match proxy_engine(state.engine_port, head, body).await {
            Some(reply) => reply,
            None => error(
                "upstream_unavailable",
                "engine is not ready",
                "502 Bad Gateway",
                true,
            ),
        },
        ("POST", "/v1/pairing:open") => {
            if !authorized(head, &state.trusted.lock().unwrap()) {
                error("unauthorized", "Device Token required", "401 Unauthorized", false)
            } else {
                *state.pairing_window.lock().unwrap() =
                    Some(std::time::Instant::now() + Duration::from_secs(300));
                *state.pairing_attempts.lock().unwrap() = 0;
                response("200 OK", "{\"window_open\":true,\"seconds\":300}\n")
            }
        }
        ("POST", "/v1/pairing:close") => {
            if !authorized(head, &state.trusted.lock().unwrap()) {
                error("unauthorized", "Device Token required", "401 Unauthorized", false)
            } else {
                *state.pairing_window.lock().unwrap() = None;
                response("200 OK", "{\"window_open\":false}\n")
            }
        }
        (_, _) if state.product.is_some() => state
            .product
            .as_ref()
            .and_then(|product| product.handle(method, path, head, body))
            .unwrap_or_else(|| error("invalid_request", "unknown route", "404 Not Found", false)),
        _ => error("invalid_request", "unknown route", "404 Not Found", false),
    }
}

fn pairing_request(state: &GatewayState, body: &str) -> Vec<u8> {
    let request: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => return error("invalid_request", "bad JSON", "400 Bad Request", false),
    };
    let phase = request.get("phase").and_then(Value::as_str).unwrap_or("");
    match phase {
        "begin" => {
            {
                let window = state.pairing_window.lock().unwrap();
                if window.map_or(true, |deadline| std::time::Instant::now() >= deadline) {
                    return error("pairing_denied", "pairing window closed", "403 Forbidden", false);
                }
                let mut attempts = state.pairing_attempts.lock().unwrap();
                if *attempts >= 5 {
                    return error("pairing_denied", "too many attempts", "403 Forbidden", false);
                }
                *attempts += 1;
            }
            let client_public = request
                .get("client_public")
                .and_then(Value::as_str)
                .and_then(|v| unhex(v, 32).ok());
            let Some(client_public) = client_public else {
                return error("invalid_request", "bad client_public", "400 Bad Request", false);
            };
            let mut key = [0u8; 32];
            key.copy_from_slice(&client_public);
            let binding = ChannelBinding {
                client_static: key,
                host_static: state.identity.public_bytes(),
            };
            let mut pairing = HostPairing::with_code(
                state.identity.clone(),
                binding,
                state.pairing_code,
            );
            let host_pake = pairing.begin();
            let reply = json!({
                "host_key": hex(&state.identity.public_bytes()),
                "nonce": hex(pairing.nonce()),
                "host_pake": hex(&host_pake),
            });
            *state.pairing.lock().unwrap() = Some(pairing);
            response("200 OK", &reply.to_string())
        }
        "finish" => {
            let mut guard = state.pairing.lock().unwrap();
            let Some(pairing) = guard.as_mut() else {
                return error("pairing_denied", "no pairing attempt", "403 Forbidden", false);
            };
            let client_pake = request
                .get("client_pake")
                .and_then(Value::as_str)
                .and_then(|v| unhex(v, 33).ok());
            let client_proof = request
                .get("client_proof")
                .and_then(Value::as_str)
                .and_then(|v| unhex(v, 64).ok());
            let client_public = request
                .get("client_public")
                .and_then(Value::as_str)
                .and_then(|v| unhex(v, 32).ok());
            let (Some(client_pake), Some(client_proof), Some(client_public)) =
                (client_pake, client_proof, client_public)
            else {
                return error("invalid_request", "bad pairing fields", "400 Bad Request", false);
            };
            let mut key = [0u8; 32];
            key.copy_from_slice(&client_public);
            let mut pake = [0u8; 33];
            pake.copy_from_slice(&client_pake);
            let mut proof = [0u8; 64];
            proof.copy_from_slice(&client_proof);
            match pairing.finish(&key, &pake, &proof, &state.info) {
                Ok((confirmations, credential)) => {
                    *guard = None;
                    if let Err(message) = state.trusted.lock().unwrap().add(&credential) {
                        return error("internal_error", &message.to_string(), "500 Internal Server Error", false);
                    }
                    response(
                        "200 OK",
                        &json!({
                            "host_confirmation": hex(&confirmations.host),
                            "credential": credential,
                        })
                        .to_string(),
                    )
                }
                Err(_) => error("pairing_denied", "pairing failed", "403 Forbidden", false),
            }
        }
        _ => error("invalid_request", "unknown phase", "400 Bad Request", false),
    }
}

fn tls_acceptor(config: &Config) -> Result<TlsAcceptor, String> {
    let mut cert_reader = BufReader::new(
        File::open(config.cert_path()).map_err(|e| format!("cert read: {e}"))?,
    );
    let mut key_reader = BufReader::new(
        File::open(config.key_path()).map_err(|e| format!("key read: {e}"))?,
    );
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("cert parse: {e}"))?;
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| format!("key parse: {e}"))?
        .ok_or("no private key")?;
    let server = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("TLS config: {e}"))?;
    Ok(TlsAcceptor::from(Arc::new(server)))
}

async fn rpc_connection(socket: TcpStream, acceptor: TlsAcceptor, upstream: SocketAddr) {
    let Ok(mut downstream) = acceptor.accept(socket).await else {
        return;
    };
    let Ok(mut upstream) = TcpStream::connect(upstream).await else {
        return;
    };
    let _ = copy_bidirectional(&mut downstream, &mut upstream).await;
}

/// Run the API server and TLS RPC proxy until the process is stopped.
///
/// # Errors
/// Returns a description when a listener or TLS certificate cannot be created.
pub async fn run(config: Config, info: BoxInfo) -> Result<(), String> {
    let api_listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], config.api_port)))
        .await
        .map_err(|e| format!("api bind: {e}"))?;
    let rpc_listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], config.rpc_port)))
        .await
        .map_err(|e| format!("rpc bind: {e}"))?;
    let acceptor = tls_acceptor(&config)?;
    let pairing_code = PairingCode::random();
    let code_path = std::path::PathBuf::from(
        std::env::var("OLGP_PAIRING_CODE_PATH")
            .unwrap_or_else(|_| "/var/lib/olgp/pairing.code".into()),
    );
    if let Some(parent) = code_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("pairing code dir: {e}"))?;
    }
    std::fs::write(&code_path, format!("{pairing_code}\n"))
        .map_err(|e| format!("pairing code write: {e}"))?;
    std::fs::set_permissions(&code_path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("pairing code mode: {e}"))?;
    let trusted_path = std::path::PathBuf::from(
        std::env::var("OLGP_TRUSTED_CLIENTS_PATH")
            .unwrap_or_else(|_| "/var/lib/olgp/trusted_clients.json".into()),
    );
    let trusted = Mutex::new(TrustedClients::load(&trusted_path)?);
    let product = Some(ProductState::load()?);
    let mib = 1024 * 1024u64;
    let scheduler = Mutex::new(Scheduler::new(
        u64::from(info.vram_mib) * mib,
        16 * 1024 * mib,
        u64::from(config.vram_reserve_mib) * mib,
        config.max_interactive_sessions,
    ));
    let registry = Mutex::new(ModelRegistry::default());
    let state = Arc::new(GatewayState {
        info,
        identity: DeviceIdentity::generate().map_err(|e| e.to_string())?,
        pairing: Mutex::new(None),
        pairing_window: Mutex::new(Some(std::time::Instant::now() + Duration::from_secs(900))),
        pairing_attempts: Mutex::new(0),
        pairing_code,
        engine_port: config.engine_port,
        product,
        trusted,
        scheduler,
        registry,
    });
    let upstream = SocketAddr::from(([127, 0, 0, 1], 14833));

    let api_task = tokio::spawn(async move {
        loop {
            match api_listener.accept().await {
                Ok((socket, _)) => {
                    let state = Arc::clone(&state);
                    tokio::spawn(api_connection(socket, state));
                }
                Err(_) => break,
            }
        }
    });
    let rpc_task = tokio::spawn(async move {
        loop {
            match rpc_listener.accept().await {
                Ok((socket, _)) => {
                    let acceptor = acceptor.clone();
                    tokio::spawn(rpc_connection(socket, acceptor, upstream));
                }
                Err(_) => break,
            }
        }
    });
    let _ = tokio::join!(api_task, rpc_task);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn state() -> GatewayState {
        GatewayState {
            info: BoxInfo {
                box_id: "a".repeat(32),
                spki_sha256: "b".repeat(64),
                address: std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
                api_port: 8787,
                rpc_port: 8843,
                capabilities: vec![],
                gpu: "GPU".into(),
                vram_mib: 4096,
                hostname: "box".into(),
            },
            identity: DeviceIdentity::generate().unwrap(),
            pairing: Mutex::new(None),
            pairing_window: Mutex::new(Some(std::time::Instant::now() + Duration::from_secs(900))),
            pairing_attempts: Mutex::new(0),
            pairing_code: PairingCode::new(123456),
            engine_port: 18080,
            product: None,
            trusted: Mutex::new(
                TrustedClients::load(&std::path::PathBuf::from("/tmp/olgp-gateway-trusted.json"))
                    .unwrap(),
            ),
            scheduler: Mutex::new(Scheduler::new(
                4096 * 1024 * 1024,
                16 * 1024 * 1024 * 1024,
                512 * 1024 * 1024,
                1,
            )),
            registry: Mutex::new(ModelRegistry::default()),
        }
    }

    #[tokio::test]
    async fn health_is_public() {
        let reply = handle_request("GET", "/v1/health/live", "", "", &state()).await;
        assert!(String::from_utf8_lossy(&reply).contains("\"ok\":true"));
    }

    #[tokio::test]
    async fn protected_route_requires_token() {
        let reply = handle_request("GET", "/v1/models", "", "", &state()).await;
        assert!(String::from_utf8_lossy(&reply).contains("401 Unauthorized"));
    }
}
