//! Node configuration loaded from `/etc/olgp/node.toml`.

use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

use crate::discovery::BoxInfo;

/// Runtime configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    pub box_id: String,
    pub api_port: u16,
    pub rpc_port: u16,
    pub engine_port: u16,
    pub vram_reserve_mib: u32,
    pub cache_min_bytes: u64,
    pub max_interactive_sessions: u32,
    pub log_level: String,
    pub address: IpAddr,
    pub hostname: String,
    pub gpu: String,
}

impl Config {
    /// Load and validate a node configuration file.
    ///
    /// # Errors
    /// Returns a description when a required key is missing or invalid.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("config read: {e}"))?;
        let mut values = std::collections::HashMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or("malformed config line")?;
            let value = value.trim().trim_matches('"').to_owned();
            values.insert(key.trim().to_owned(), value);
        }
        let get = |key: &str| -> Result<String, String> {
            values.get(key).cloned().ok_or_else(|| format!("missing key {key}"))
        };
        let box_id = get("box_id")?;
        if box_id.len() != 32 || !box_id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("box_id must be 32 hex characters".into());
        }
        let address = values
            .get("address")
            .map_or(Ok(IpAddr::from([127, 0, 0, 1])), |value| {
                value.parse::<IpAddr>().map_err(|_| "bad address".to_owned())
            })?;
        Ok(Self {
            box_id,
            api_port: get("api_port")?.parse().map_err(|_| "bad api_port")?,
            rpc_port: get("rpc_port")?.parse().map_err(|_| "bad rpc_port")?,
            engine_port: get("engine_port")?.parse().map_err(|_| "bad engine_port")?,
            vram_reserve_mib: get("vram_reserve_mib")?.parse().map_err(|_| "bad vram_reserve_mib")?,
            cache_min_bytes: get("cache_min_bytes")?.parse().map_err(|_| "bad cache_min_bytes")?,
            max_interactive_sessions: get("max_interactive_sessions")?
                .parse()
                .map_err(|_| "bad max_interactive_sessions")?,
            log_level: get("log_level")?,
            address,
            hostname: values.get("hostname").cloned().unwrap_or_else(|| "olgp-box".into()),
            gpu: values.get("gpu").cloned().unwrap_or_else(|| "NVIDIA GPU".into()),
        })
    }

    /// Build discovery information from the configuration and the SPKI file.
    ///
    /// # Errors
    /// Returns a description when the SPKI pin cannot be read.
    pub fn box_info(&self, spki_path: &Path) -> Result<BoxInfo, String> {
        let spki = fs::read_to_string(spki_path)
            .map_err(|e| format!("spki read: {e}"))?
            .trim()
            .to_owned();
        if spki.is_empty() || !spki.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("spki pin must be hex".into());
        }
        Ok(BoxInfo {
            box_id: self.box_id.clone(),
            spki_sha256: spki,
            address: self.address,
            api_port: self.api_port,
            rpc_port: self.rpc_port,
            capabilities: vec![
                "cuda".into(),
                "nvml".into(),
                "batch".into(),
                "trace".into(),
                "nvme".into(),
                "voice".into(),
            ],
            gpu: self.gpu.clone(),
            vram_mib: 0,
            hostname: self.hostname.clone(),
        })
    }

    /// Path to the TLS certificate used by the RPC gateway.
    pub fn cert_path(&self) -> PathBuf {
        std::env::var("OLGP_CERT_PATH").map_or_else(
            |_| PathBuf::from("/etc/olgp/tls/cert.pem"),
            PathBuf::from,
        )
    }

    /// Path to the TLS private key used by the RPC gateway.
    pub fn key_path(&self) -> PathBuf {
        std::env::var("OLGP_KEY_PATH").map_or_else(
            |_| PathBuf::from("/etc/olgp/tls/key.pem"),
            PathBuf::from,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_text() -> &'static str {
        "box_id = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\napi_port = 8787\nrpc_port = 8843\nengine_port = 8080\nvram_reserve_mib = 512\ncache_min_bytes = 67108864\nmax_interactive_sessions = 1\nlog_level = info\n"
    }

    #[test]
    fn parses_complete_config() {
        let root = std::env::temp_dir().join(format!("olgp-config-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("node.toml");
        fs::write(&path, config_text()).unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.api_port, 8787);
        assert_eq!(config.cache_min_bytes, 67_108_864);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_bad_box_id() {
        let root = std::env::temp_dir().join(format!("olgp-config-bad-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("node.toml");
        fs::write(&path, config_text().replace("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "bad")).unwrap();
        assert!(Config::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn missing_key_is_rejected() {
        let root = std::env::temp_dir().join(format!("olgp-config-missing-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("node.toml");
        fs::write(&path, "box_id = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n").unwrap();
        assert!(Config::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn bad_port_is_rejected() {
        let root = std::env::temp_dir().join(format!("olgp-config-port-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("node.toml");
        fs::write(&path, config_text().replace("api_port = 8787", "api_port = bad")).unwrap();
        assert!(Config::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn box_info_uses_spki_file() {
        let root = std::env::temp_dir().join(format!("olgp-config-spki-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let config_path = root.join("node.toml");
        fs::write(&config_path, config_text()).unwrap();
        let spki = root.join("spki.sha256");
        fs::write(&spki, "abc123\n").unwrap();
        let config = Config::load(&config_path).unwrap();
        let info = config.box_info(&spki).unwrap();
        assert_eq!(info.spki_sha256, "abc123");
        assert_eq!(info.capabilities.len(), 6);
        fs::remove_file(spki).unwrap();
        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn box_info_rejects_bad_spki() {
        let root = std::env::temp_dir().join(format!("olgp-config-badspki-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let config_path = root.join("node.toml");
        fs::write(&config_path, config_text()).unwrap();
        let spki = root.join("spki.sha256");
        fs::write(&spki, "not-hex\n").unwrap();
        let config = Config::load(&config_path).unwrap();
        assert!(config.box_info(&spki).is_err());
        fs::remove_file(spki).unwrap();
        fs::remove_file(config_path).unwrap();
    }

    #[test]
    fn malformed_config_line_is_rejected() {
        let root = std::env::temp_dir().join(format!("olgp-config-malformed-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("node.toml");
        fs::write(&path, "this is not a key value\n").unwrap();
        assert!(Config::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn bad_address_is_rejected() {
        let root = std::env::temp_dir().join(format!("olgp-config-address-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("node.toml");
        fs::write(&path, format!("{}address = not-an-ip\n", config_text())).unwrap();
        assert!(Config::load(&path).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn tls_paths_use_environment_overrides() {
        let config = Config {
            box_id: "a".repeat(32),
            api_port: 8787,
            rpc_port: 8843,
            engine_port: 8080,
            vram_reserve_mib: 512,
            cache_min_bytes: 64,
            max_interactive_sessions: 1,
            log_level: "info".into(),
            address: [127, 0, 0, 1].into(),
            hostname: "box".into(),
            gpu: "GPU".into(),
        };
        unsafe {
            std::env::set_var("OLGP_CERT_PATH", "/tmp/olgp-cert-test.pem");
            std::env::set_var("OLGP_KEY_PATH", "/tmp/olgp-key-test.pem");
        }
        assert_eq!(config.cert_path(), PathBuf::from("/tmp/olgp-cert-test.pem"));
        assert_eq!(config.key_path(), PathBuf::from("/tmp/olgp-key-test.pem"));
        unsafe {
            std::env::remove_var("OLGP_CERT_PATH");
            std::env::remove_var("OLGP_KEY_PATH");
        }
    }
}
