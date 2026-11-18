//! API HTTP server, pairing endpoints, and TLS-terminated RPC reverse proxy.

use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lumen_crypto::pairing::{ChannelBinding, DeviceIdentity};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;

use crate::config::Config;
use tokio::io::copy_bidirectional;
use crate::discovery::BoxInfo;
use crate::pairing::HostPairing;
use crate::household::{Household, Memo, Timer};
use crate::memory::{AudioMemory, Settings};

struct GatewayState {
    info: BoxInfo,
    identity: DeviceIdentity,
    pairing: Mutex<Option<HostPairing>>,
    pairing_window: Mutex<Option<std::time::Instant>>,
    pairing_attempts: Mutex<u32>,
    household: Mutex<Household>,
    memory: Mutex<AudioMemory>,
    engine_port: u16,
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

fn authorized(head: &str) -> bool {
    head.lines().any(|line| {
        line.to_ascii_lowercase().starts_with("authorization: bearer ")
            && line.len() > "authorization: bearer ".len()
    })
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
    let result = handle_request(&method, &path, &head, &body, &state).await;
    let _ = socket.write_all(&result).await;
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
                "box_id": state.info.box_id,
                "gpu": state.info.gpu,
                "vram_mib": state.info.vram_mib,
                "capabilities": state.info.capabilities,
                "ready": true,
            })
            .to_string(),
        ),
        ("GET", "/metrics") => text_response("200 OK", &crate::telemetry::render()),
        ("GET", "/v1/pairing/status") => response(
            "200 OK",
            &json!({"box_id": state.info.box_id, "window_open": true, "paired": false}).to_string(),
        ),
        ("POST", "/v1/pairing") => pairing_request(state, body),
        (_, _) if !authorized(head) => {
            error("unauthorized", "Device Token required", "401 Unauthorized", false)
        }
        ("POST", "/v1/chat/completions")
        | ("POST", "/v1/responses")
        | ("POST", "/v1/embeddings") => {
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
        ("GET", "/v1/timers") | ("POST", "/v1/timers") | ("GET", "/v1/memos") | ("POST", "/v1/memos") => {
            if authorized(head) {
                household_request(method, path, body, state)
            } else {
                error("unauthorized", "Device Token required", "401 Unauthorized", false)
            }
        }
        (method, path)
            if path.starts_with("/v1/audio-memory")
                && matches!(method, "GET" | "POST" | "PATCH") =>
        {
            if authorized(head) {
                audio_memory_request(method, path, head, body, state)
            } else {
                error("unauthorized", "Device Token required", "401 Unauthorized", false)
            }
        }
        ("POST", "/v1/pairing:open") => {
            if !authorized(head) {
                error("unauthorized", "Device Token required", "401 Unauthorized", false)
            } else {
                *state.pairing_window.lock().unwrap() =
                    Some(std::time::Instant::now() + Duration::from_secs(300));
                *state.pairing_attempts.lock().unwrap() = 0;
                response("200 OK", "{\"window_open\":true,\"seconds\":300}\n")
            }
        }
        ("POST", "/v1/pairing:close") => {
            if !authorized(head) {
                error("unauthorized", "Device Token required", "401 Unauthorized", false)
            } else {
                *state.pairing_window.lock().unwrap() = None;
                response("200 OK", "{\"window_open\":false}\n")
            }
        }
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
            let mut pairing = HostPairing::new(state.identity.clone(), binding);
            let host_pake = pairing.begin();
            let code_path = std::env::var("OLGP_PAIRING_CODE_PATH")
                .unwrap_or_else(|_| "/var/lib/olgp/pairing.code".into());
            if let Some(parent) = std::path::Path::new(&code_path).parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&code_path, format!("{}\n", pairing.code()));
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


fn household_request(method: &str, path: &str, body: &str, state: &GatewayState) -> Vec<u8> {
    let request: Option<Value> = serde_json::from_str(body).ok();
    let mut store = state.household.lock().unwrap();
    match (method, path) {
        ("GET", "/v1/timers") => response("200 OK", &json!(store.timers()).to_string()),
        ("POST", "/v1/timers") => {
            let Some(request) = request else {
                return error("invalid_request", "bad JSON", "400 Bad Request", false);
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let timer = Timer {
                id: format!("t{now}"),
                kind: request.get("kind").and_then(Value::as_str).unwrap_or("timer").to_owned(),
                due_at_ms: request.get("dueAtMs").and_then(Value::as_u64).unwrap_or(0),
                title: request.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
                cancelled: false,
            };
            match store.add_timer(timer.clone()) {
                Ok(()) => response("200 OK", &json!(timer).to_string()),
                Err(message) => error("internal_error", &message, "500 Internal Server Error", false),
            }
        }
        ("GET", "/v1/memos") => response("200 OK", &json!(store.memos()).to_string()),
        ("POST", "/v1/memos") => {
            let Some(request) = request else {
                return error("invalid_request", "bad JSON", "400 Bad Request", false);
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let memo = Memo {
                id: format!("m{now}"),
                title: request.get("title").and_then(Value::as_str).unwrap_or("").to_owned(),
                dismissed: false,
            };
            match store.add_memo(memo.clone()) {
                Ok(()) => response("200 OK", &json!(memo).to_string()),
                Err(message) => error("internal_error", &message, "500 Internal Server Error", false),
            }
        }
        _ => error("invalid_request", "unknown route", "404 Not Found", false),
    }
}

fn memory_authorized(head: &str) -> bool {
    head.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("x-olgp-memory-token:") && line.len() > "x-olgp-memory-token:".len()
            || lower.starts_with("x-wici-memory-token:") && line.len() > "x-wici-memory-token:".len()
    })
}

fn audio_memory_request(method: &str, path: &str, head: &str, body: &str, state: &GatewayState) -> Vec<u8> {
    if !memory_authorized(head) {
        return error("unauthorized", "Memory Token required", "401 Unauthorized", false);
    }
    let request: Option<Value> = serde_json::from_str(body).ok();
    let mut store = state.memory.lock().unwrap();
    match (method, path) {
        ("GET", "/v1/audio-memory/status") => response(
            "200 OK",
            &json!({
                "mode": store.settings().mode,
                "retention_days": store.settings().retention_days,
                "episodes": store.episodes().len(),
                "notes": store.notes().len(),
                "ready": true,
            })
            .to_string(),
        ),
        ("GET", "/v1/audio-memory/settings") => response("200 OK", &json!(store.settings()).to_string()),
        ("PATCH", "/v1/audio-memory/settings") => {
            let Some(request) = request else {
                return error("invalid_request", "bad JSON", "400 Bad Request", false);
            };
            let settings = Settings {
                mode: request.get("mode").and_then(Value::as_str).unwrap_or("disabled").to_owned(),
                retention_days: request
                    .get("retention_days")
                    .and_then(Value::as_u64)
                    .unwrap_or(30)
                    .min(3650) as u32,
            };
            match store.set_settings(settings.clone()) {
                Ok(()) => response("200 OK", &json!(settings).to_string()),
                Err(message) => error("internal_error", &message, "500 Internal Server Error", false),
            }
        }
        ("GET", "/v1/audio-memory/episodes") => response("200 OK", &json!(store.episodes()).to_string()),
        ("GET", "/v1/audio-memory/notes") => response("200 OK", &json!(store.notes()).to_string()),
        ("POST", "/v1/audio-memory/notes") => {
            let Some(request) = request else {
                return error("invalid_request", "bad JSON", "400 Bad Request", false);
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            match store.add_note(
                format!("n{now}"),
                request.get("text").and_then(Value::as_str).unwrap_or("").to_owned(),
                request.get("kind").and_then(Value::as_str).unwrap_or("fact").to_owned(),
            ) {
                Ok(()) => response("200 OK", "{\"ok\":true}\n"),
                Err(message) => error("internal_error", &message, "500 Internal Server Error", false),
            }
        }
        ("POST", "/v1/audio-memory:query") => {
            let needle = request
                .as_ref()
                .and_then(|value| value.get("question").and_then(Value::as_str))
                .unwrap_or("");
            response("200 OK", &json!(store.query(needle)).to_string())
        }
        ("POST", "/v1/audio-memory:forget") => {
            let id = request
                .as_ref()
                .and_then(|value| value.get("id").and_then(Value::as_str))
                .unwrap_or("");
            match store.forget(id) {
                Ok(removed) => response("200 OK", &json!({"removed": removed}).to_string()),
                Err(message) => error("internal_error", &message, "500 Internal Server Error", false),
            }
        }
        ("POST", "/v1/audio-memory:clear") => match store.clear() {
            Ok(()) => response("200 OK", "{\"removed\":true}\n"),
            Err(message) => error("internal_error", &message, "500 Internal Server Error", false),
        },
        _ => error("invalid_request", "unknown route", "404 Not Found", false),
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
    let household_path = std::path::PathBuf::from(
        std::env::var("OLGP_HOUSEHOLD_PATH")
            .unwrap_or_else(|_| "/var/lib/olgp/household.json".into()),
    );
    let household = Household::load(&household_path)?;
    let memory_path = std::path::PathBuf::from(
        std::env::var("OLGP_MEMORY_PATH")
            .unwrap_or_else(|_| "/var/lib/olgp/audio-memory.json".into()),
    );
    let memory = AudioMemory::load(&memory_path)?;
    let state = Arc::new(GatewayState {
        info,
        identity: DeviceIdentity::generate().map_err(|e| e.to_string())?,
        pairing: Mutex::new(None),
        pairing_window: Mutex::new(Some(std::time::Instant::now() + Duration::from_secs(900))),
        pairing_attempts: Mutex::new(0),
        household: Mutex::new(household),
        memory: Mutex::new(memory),
        engine_port: config.engine_port,
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
            engine_port: 18080,
            household: Mutex::new(
                Household::load(&std::path::PathBuf::from("/tmp/olgp-gateway-test-household.json"))
                    .unwrap(),
            ),
            memory: Mutex::new(
                AudioMemory::load(&std::path::PathBuf::from("/tmp/olgp-gateway-test-memory.json"))
                    .unwrap(),
            ),
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
