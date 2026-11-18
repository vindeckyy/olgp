//! mDNS/DNS-SD discovery for OLGP boxes.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

/// Service type advertised by OLGP boxes.
pub const SERVICE_TYPE: &str = "_olgp._tcp.local.";

/// A box found on the LAN.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoxInfo {
    pub box_id: String,
    pub spki_sha256: String,
    pub address: IpAddr,
    pub api_port: u16,
    pub rpc_port: u16,
    pub capabilities: Vec<String>,
    pub gpu: String,
    pub vram_mib: u32,
    pub hostname: String,
}

/// Parse one TXT property map into a `BoxInfo`.
///
/// # Errors
/// Returns a description when a required field is absent or malformed.
pub fn parse_txt(
    address: IpAddr,
    _port: u16,
    properties: &HashMap<String, String>,
) -> Result<BoxInfo, String> {
    let field = |key: &str| properties.get(key).map(String::as_str);
    let box_id = field("id").ok_or("missing id")?;
    let spki = field("fp").ok_or("missing fp")?;
    if field("v") != Some("1") {
        return Err("unsupported version".into());
    }
    if box_id.len() != 32 || !box_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("id must be 32 hex characters".into());
    }
    if spki.is_empty() || !spki.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("fp must be hex".into());
    }
    let api_port = field("api").unwrap_or("8787").parse::<u16>().map_err(|_| "bad api port")?;
    let rpc_port = field("rpc").unwrap_or("8843").parse::<u16>().map_err(|_| "bad rpc port")?;
    if properties.len() > 9 {
        return Err("too many TXT keys".into());
    }
    Ok(BoxInfo {
        box_id: box_id.to_owned(),
        spki_sha256: spki.to_owned(),
        address,
        api_port,
        rpc_port,
        capabilities: field("caps").unwrap_or("").split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect(),
        gpu: field("gpu").unwrap_or("").to_owned(),
        vram_mib: field("vram_mib").unwrap_or("0").parse::<u32>().map_err(|_| "bad vram_mib")?,
        hostname: field("box").unwrap_or("").to_owned(),
    })
}

/// Advertise one box. Dropping the value withdraws the advertisement.
pub struct Advertiser {
    _daemon: ServiceDaemon,
}

impl std::fmt::Debug for Advertiser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Advertiser").finish_non_exhaustive()
    }
}

impl Advertiser {
    /// Publish the TXT records defined in `spec/v1/txt-record.md`.
    ///
    /// # Errors
    /// Returns a description when the daemon or registration fails.
    pub fn advertise(info: &BoxInfo, instance: &str) -> Result<Self, String> {
        let daemon = ServiceDaemon::new().map_err(|e| format!("mDNS daemon: {e}"))?;
        let mut properties = HashMap::new();
        properties.insert("v".into(), "1".into());
        properties.insert("id".into(), info.box_id.clone());
        properties.insert("fp".into(), info.spki_sha256.clone());
        properties.insert("api".into(), info.api_port.to_string());
        properties.insert("rpc".into(), info.rpc_port.to_string());
        properties.insert("caps".into(), info.capabilities.join(","));
        properties.insert("gpu".into(), info.gpu.clone());
        properties.insert("vram_mib".into(), info.vram_mib.to_string());
        properties.insert("box".into(), info.hostname.clone());
        let host = if info.hostname.ends_with(".local.") {
            info.hostname.clone()
        } else {
            format!("{}.local.", info.hostname)
        };
        let service = ServiceInfo::new(
            SERVICE_TYPE,
            instance,
            &host,
            info.address,
            info.api_port,
            properties,
        )
        .map_err(|e| format!("mDNS service info: {e}"))?;
        daemon.register(service).map_err(|e| format!("mDNS register: {e}"))?;
        Ok(Self { _daemon: daemon })
    }
}

/// Browse for boxes for `timeout`.
///
/// # Errors
/// Returns a description when the daemon or browse operation fails.
pub fn browse(timeout: Duration) -> Result<Vec<BoxInfo>, String> {
    let daemon = ServiceDaemon::new().map_err(|e| format!("mDNS daemon: {e}"))?;
    let receiver = daemon.browse(SERVICE_TYPE).map_err(|e| format!("mDNS browse: {e}"))?;
    let mut found: Vec<BoxInfo> = Vec::new();
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(ServiceEvent::ServiceResolved(service)) => {
                let props = service.get_properties();
                let mut properties = HashMap::new();
                for key in ["v", "id", "fp", "api", "rpc", "caps", "gpu", "vram_mib", "box"] {
                    if let Some(value) = props.get_property_val_str(key) {
                        properties.insert(key.to_owned(), value.to_owned());
                    }
                }
                for address in service.get_addresses() {
                    if let Ok(info) = parse_txt(address.to_ip_addr(), service.get_port(), &properties)
                    {
                        if !found.contains(&info) {
                            found.push(info);
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let _ = daemon.shutdown();
    Ok(found)
}

/// Display fingerprint the same way on both sides: first 12 and last 12 hex.
#[must_use]
pub fn short_fingerprint(fingerprint: &str) -> String {
    if fingerprint.len() <= 24 {
        return fingerprint.to_owned();
    }
    format!("{}…{}", &fingerprint[..12], &fingerprint[fingerprint.len() - 12..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn properties() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("v".into(), "1".into());
        map.insert("id".into(), "a".repeat(32));
        map.insert("fp".into(), "b".repeat(64));
        map.insert("api".into(), "8787".into());
        map.insert("rpc".into(), "8843".into());
        map.insert("caps".into(), "cuda,nvml".into());
        map.insert("gpu".into(), "GPU".into());
        map.insert("vram_mib".into(), "4096".into());
        map.insert("box".into(), "box".into());
        map
    }

    #[test]
    fn parses_all_nine_txt_keys() {
        let info = parse_txt(IpAddr::V4(Ipv4Addr::LOCALHOST), 8787, &properties()).unwrap();
        assert_eq!(info.box_id, "a".repeat(32));
        assert_eq!(info.api_port, 8787);
        assert_eq!(info.capabilities, vec!["cuda", "nvml"]);
    }

    #[test]
    fn rejects_bad_id_and_extra_keys() {
        let mut props = properties();
        props.insert("id".into(), "short".into());
        assert!(parse_txt(IpAddr::V4(Ipv4Addr::LOCALHOST), 8787, &props).is_err());
        let mut props = properties();
        props.insert("extra".into(), "x".into());
        assert!(parse_txt(IpAddr::V4(Ipv4Addr::LOCALHOST), 8787, &props).is_err());
    }

    #[test]
    fn short_fingerprint_is_stable() {
        assert_eq!(short_fingerprint(&"b".repeat(64)), format!("{}…{}", "b".repeat(12), "b".repeat(12)));
    }
}
