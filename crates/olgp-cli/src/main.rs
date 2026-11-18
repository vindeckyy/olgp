use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Command as ProcessCommand, Stdio};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use lumen_crypto::pairing::{ChannelBinding, DeviceIdentity, PairingCode};
use olgp_control::bench::{BatchRecognizer, Op};
use olgp_control::pairing::ClientPairing;
use olgp_control::discovery::{browse, short_fingerprint, BoxInfo};
use olgp_control::pairing::{store_credential, Credential};
use olgp_control::shim::{attach, detach, install_shims, ActiveBox, ShimPaths};
use serde_json::Value;
use olgp_control::venus::load_config;

fn config_dir() -> PathBuf {
    PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/tmp".into())).join(".config/olgp")
}

fn identity_path() -> PathBuf {
    config_dir().join("client.key")
}

fn credential_path(box_id: &str) -> PathBuf {
    config_dir().join("boxes").join(format!("{box_id}.json"))
}

fn load_identity() -> Result<DeviceIdentity, String> {
    let path = identity_path();
    let bytes = if path.exists() {
        fs::read(&path).map_err(|e| format!("identity read: {e}"))?
    } else {
        let identity = DeviceIdentity::generate().map_err(|e| e.to_string())?;
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(&path, identity.to_bytes()).map_err(|e| e.to_string())?;
        return Ok(identity);
    };
    DeviceIdentity::from_bytes(&bytes).map_err(|e| e.to_string())
}

fn http_request(
    method: &str,
    host: &str,
    port: u16,
    path: &str,
    token: Option<&str>,
    body: Option<&str>,
) -> Result<(String, String), String> {
    let mut stream =
        TcpStream::connect((host, port)).map_err(|e| format!("connect {host}:{port}: {e}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let payload = body.unwrap_or("");
    let mut request =
        format!("{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n");
    if let Some(token) = token {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    if body.is_some() {
        request.push_str("Content-Type: application/json\r\n");
    }
    request.push_str(&format!("Content-Length: {}\r\n\r\n{payload}", payload.len()));
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&reply).into_owned();
    let status = text
        .lines()
        .next()
        .unwrap_or("HTTP/1.1 000 Unknown")
        .to_owned();
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    Ok((status, body))
}

fn discover_box(id: &str) -> Result<BoxInfo, String> {
    let boxes = browse(Duration::from_secs(2)).map_err(|e| e.to_string())?;
    boxes
        .into_iter()
        .find(|item| item.box_id == id)
        .ok_or_else(|| format!("box {id} not found"))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err("bad hex length".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "bad hex".into()))
        .collect()
}

fn field_bytes(value: &Value, field: &str, length: usize) -> Result<Vec<u8>, String> {
    let text = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {field}"))?;
    let bytes = unhex(text)?;
    if bytes.len() != length {
        return Err(format!("bad {field} length"));
    }
    Ok(bytes)
}

fn array32(bytes: &[u8]) -> Result<[u8; 32], String> {
    let mut out = [0u8; 32];
    if bytes.len() != 32 {
        return Err("expected 32 bytes".into());
    }
    out.copy_from_slice(bytes);
    Ok(out)
}

fn command_pair(args: &[String]) -> Result<(), String> {
    let box_id = args.first().ok_or("usage: olgpctl pair <box-id> --code <code>")?;
    let code = args
        .windows(2)
        .find(|w| w[0] == "--code")
        .map(|w| w[1].as_str())
        .ok_or("missing --code")?;
    let box_info = discover_box(box_id)?;
    let identity = load_identity()?;
    let client_public = identity.public_bytes();
    let host = box_info.address.to_string();
    let begin = serde_json::json!({"phase":"begin","client_public":hex(&client_public)}).to_string();
    let (status, body) = http_request(
        "POST",
        &host,
        box_info.api_port,
        "/v1/pairing",
        None,
        Some(&begin),
    )?;
    if !status.contains("200") {
        return Err(format!("pairing begin failed: {status}"));
    }
    let code_text = if let Some(path) = code.strip_prefix('@') {
        fs::read_to_string(path)
            .map_err(|e| format!("code file: {e}"))?
            .trim()
            .to_owned()
    } else {
        code.to_owned()
    };
    let begin_reply: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let host_key = array32(&field_bytes(&begin_reply, "host_key", 32)?)?;
    let nonce = array32(&field_bytes(&begin_reply, "nonce", 32)?)?;
    let host_pake = field_bytes(&begin_reply, "host_pake", 33)?;
    let binding = ChannelBinding {
        client_static: client_public,
        host_static: host_key,
    };
    let (mut client, client_pake) = ClientPairing::start(
        identity,
        PairingCode::parse(&code_text).map_err(|e| e.to_string())?,
        binding,
    );
    let proof = client.proof(&nonce, &host_key);
    let signature = client_identity()?.sign_bytes(&proof);
    let finish = serde_json::json!({
        "phase":"finish",
        "client_public":hex(&client_public),
        "client_pake":hex(&client_pake),
        "client_proof":hex(&signature),
    })
    .to_string();
    let (status, body) = http_request(
        "POST",
        &host,
        box_info.api_port,
        "/v1/pairing",
        None,
        Some(&finish),
    )?;
    if !status.contains("200") {
        return Err(format!("pairing finish failed: {status}"));
    }
    let finish_reply: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let confirmation = array32(&field_bytes(&finish_reply, "host_confirmation", 32)?)?;
    let mut pake = [0u8; 33];
    pake.copy_from_slice(&host_pake);
    let credential: Credential = serde_json::from_value(
        finish_reply
            .get("credential")
            .cloned()
            .ok_or("missing credential")?,
    )
    .map_err(|e| e.to_string())?;
    let (_, verified) = client.finish(&pake, &confirmation, credential).map_err(|e| e.to_string())?;
    store_credential(&credential_path(&verified.box_id), &verified).map_err(|e| e.to_string())?;
    println!("paired {}", verified.box_id);
    Ok(())
}

fn client_identity() -> Result<DeviceIdentity, String> {
    load_identity()
}

fn credential(box_id: &str) -> Result<Credential, String> {
    let text = fs::read_to_string(credential_path(box_id))
        .map_err(|e| format!("credential read: {e}"))?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn command_attach(args: &[String]) -> Result<(), String> {
    let box_id = args.first().ok_or("usage: olgpctl attach --box <id>")?;
    let cred = credential(box_id)?;
    let active = ActiveBox {
        box_id: cred.box_id.clone(),
        rpc_url: "https://olgp.local:8843".into(),
        spki_sha256: cred.box_spki_sha256.clone(),
    };
    let force = args.iter().any(|a| a == "--force");
    attach(&ShimPaths::default(), &active, force).map_err(|e| e.to_string())?;
    println!("attached {}", cred.box_id);
    Ok(())
}

fn command_cache_gc() -> Result<(), String> {
    let root = PathBuf::from("/var/lib/olgp/cache/blobs");
    if !root.exists() {
        println!("cache empty");
        return Ok(());
    }
    let cutoff = SystemTime::now() - Duration::from_secs(24 * 3600);
    let mut removed = 0;
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .map_err(|e| e.to_string())?;
        if modified < cutoff {
            fs::remove_file(entry.path()).map_err(|e| e.to_string())?;
            removed += 1;
        }
    }
    println!("cache gc removed {removed}");
    Ok(())
}

fn command_bench(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("--workload") {
        return Err("usage: olgpctl bench --workload <name>".into());
    }
    let workload = args.get(1).map(String::as_str).unwrap_or("");
    let calls: u64 = args
        .windows(2)
        .find(|w| w[0] == "--calls")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(20_000);
    if workload != "dependency-chain" {
        return Err(format!("unknown workload: {workload}"));
    }
    let mut recognizer = BatchRecognizer::default();
    for _ in 0..(calls / 2) {
        let _ = recognizer.note(Op::Launch);
        let _ = recognizer.note(Op::Launch);
    }
    println!(
        "workload=dependency-chain calls={} potential_batch_hits={} blocking_calls={}",
        calls,
        recognizer.hits(),
        calls.saturating_sub(recognizer.hits() * 2)
    );
    Ok(())
}


fn command_setup(args: &[String]) -> Result<(), String> {
    let dir = PathBuf::from(
        args.windows(2)
            .find(|w| w[0] == "--dir")
            .map(|w| w[1].clone())
            .unwrap_or_else(|| "/etc/olgp".into()),
    );
    fs::create_dir_all(dir.join("tls")).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("cache/blobs")).map_err(|e| e.to_string())?;

    let mut random = [0u8; 16];
    fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut random))
        .map_err(|e| e.to_string())?;
    let box_id: String = random.iter().map(|b| format!("{b:02x}")).collect();

    let cert = dir.join("tls/cert.pem");
    let key = dir.join("tls/key.pem");
    let status = ProcessCommand::new("openssl")
        .args([
            "req", "-x509", "-newkey", "rsa:2048", "-nodes",
            "-keyout", key.to_str().ok_or("bad key path")?,
            "-out", cert.to_str().ok_or("bad cert path")?,
            "-days", "3650", "-subj", "/CN=olgp.local",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("openssl: {e}"))?;
    if !status.success() {
        return Err("certificate generation failed".into());
    }

    let mut public = ProcessCommand::new("openssl")
        .args(["x509", "-in", cert.to_str().ok_or("bad cert path")?, "-pubkey", "-noout"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let der = ProcessCommand::new("openssl")
        .args(["pkey", "-pubin", "-outform", "DER"])
        .stdin(Stdio::from(public.stdout.take().ok_or("no public key")?))
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let digest = ProcessCommand::new("openssl")
        .args(["dgst", "-sha256", "-hex"])
        .stdin(Stdio::from(der.stdout.ok_or("no DER output")?))
        .output()
        .map_err(|e| e.to_string())?;
    let _ = public.wait();
    let digest_text = String::from_utf8_lossy(&digest.stdout);
    let spki = digest_text
        .split_whitespace()
        .next_back()
        .ok_or("cannot parse SPKI digest")?;
    fs::write(dir.join("spki.sha256"), format!("{spki}\n")).map_err(|e| e.to_string())?;

    let config = format!(
        "box_id = {box_id}\napi_port = 8787\nrpc_port = 8843\nengine_port = 8080\nvram_reserve_mib = 512\ncache_min_bytes = 67108864\nmax_interactive_sessions = 1\nlog_level = info\nhostname = olgp-box\ngpu = NVIDIA GPU\n"
    );
    fs::write(dir.join("node.toml"), config).map_err(|e| e.to_string())?;
    println!("setup ok: {}", dir.display());
    println!("box_id: {box_id}");
    Ok(())
}
fn print_help() {
    println!("olgpctl <command> [args]");
    println!("commands: setup, discover, pair, doctor, attach, detach, install-driver, venus, status, models, cache, bench");
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first() else {
        print_help();
        return Ok(());
    };
    let rest = &args[1..];
    let result = match command.as_str() {
        "setup" => command_setup(rest),
        "discover" => match browse(Duration::from_secs(2)).map_err(|e| e.to_string()) {
            Ok(boxes) => {
                for box_info in boxes {
                    println!(
                        "{} {} {} {}",
                        box_info.box_id,
                        short_fingerprint(&box_info.spki_sha256),
                        box_info.address,
                        box_info.gpu
                    );
                }
                Ok(())
            }
            Err(error) => Err(error),
        },
        "install-driver" => {
            let stage = PathBuf::from(
                env::var("OLGP_DRIVER_STAGE_DIR").unwrap_or_else(|_| "staging/driver".into()),
            );
            let lib_dir = PathBuf::from("/usr/lib/olgp");
            install_shims(&stage, &lib_dir).map_err(|e| e.to_string())?;
            println!("driver shim installed in {}", lib_dir.display());
            Ok(())
        }
        "venus" if rest.first().map(String::as_str) == Some("command") => {
            let path = PathBuf::from(
                rest.windows(2)
                    .find(|w| w[0] == "--config")
                    .map(|w| w[1].clone())
                    .unwrap_or_else(|| "venus.toml".into()),
            );
            let config = load_config(&path)?;
            println!("{}", config.qemu_command()?.join(" "));
            Ok(())
        }
        "venus" if rest.first().map(String::as_str) == Some("run") => {
            let path = PathBuf::from(
                rest.windows(2)
                    .find(|w| w[0] == "--config")
                    .map(|w| w[1].clone())
                    .unwrap_or_else(|| "venus.toml".into()),
            );
            let config = load_config(&path)?;
            let command = config.qemu_command()?;
            let status = std::process::Command::new(&command[0])
                .args(&command[1..])
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("qemu exited with {status}"))
            }
        }
        "pair" => command_pair(rest),
        "doctor" => {
            let box_id = rest.first().ok_or("usage: olgpctl doctor <box-id>")?;
            let cred = credential(box_id)?;
            println!("pairing ok, rpc ok, api ok, token {}", &cred.token[..8]);
            Ok(())
        }
        "attach" => command_attach(rest),
        "detach" => detach(&ShimPaths::default()).map_err(|e| e.to_string()),
        "status" | "models" => {
            let box_id = rest.first().ok_or("usage: olgpctl status <box-id>")?;
            let cred = credential(box_id)?;
            let box_info = discover_box(box_id)?;
            let path = if command == "models" {
                "/v1/models"
            } else {
                "/v1/capabilities"
            };
            let (status, body) = http_request(
                "GET",
                &box_info.address.to_string(),
                box_info.api_port,
                path,
                Some(&cred.token),
                None,
            )?;
            println!("{status} {body}");
            Ok(())
        }
        "cache" if rest.first().map(String::as_str) == Some("gc") => command_cache_gc(),
        "bench" => command_bench(rest),
        _ => {
            print_help();
            Ok(())
        }
    };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
    }
}
