use std::env;
use std::path::PathBuf;
use std::process::{Child, Command};

use olgp_control::config::Config;
use olgp_control::discovery::Advertiser;
use olgp_control::gateway;

fn spawn_optional(name: &str) -> Option<Child> {
    let command = env::var(name).ok()?;
    let mut parts = command.split_whitespace();
    let program = parts.next()?;
    let mut child = Command::new(program);
    for argument in parts {
        child.arg(argument);
    }
    match child.spawn() {
        Ok(child) => Some(child),
        Err(error) => {
            eprintln!("olgp-node: cannot start {name}: {error}");
            None
        }
    }
}

fn main() -> Result<(), String> {
    let config_path = PathBuf::from(
        env::var("OLGP_CONFIG").unwrap_or_else(|_| "/etc/olgp/node.toml".into()),
    );
    let config = Config::load(&config_path)?;
    let spki_path = PathBuf::from(
        env::var("OLGP_SPKI_PATH").unwrap_or_else(|_| "/etc/olgp/spki.sha256".into()),
    );
    let mut info = config.box_info(&spki_path)?;
    if let Ok(vram) = env::var("OLGP_VRAM_MIB") {
        info.vram_mib = vram.parse().map_err(|_| "bad OLGP_VRAM_MIB".to_owned())?;
    }

    let mut children = Vec::new();
    if let Some(child) = spawn_optional("OLGP_LUPINE_CMD") {
        children.push(child);
    }
    if let Some(child) = spawn_optional("OLGP_ENGINE_CMD") {
        children.push(child);
    }

    let advertiser = Advertiser::advertise(&info, &format!("olgp-{}", &info.box_id[..8]))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {e}"))?;
    let result = runtime.block_on(gateway::run(config, info));
    drop(advertiser);
    for child in &mut children {
        let _ = child.kill();
    }
    result
}
