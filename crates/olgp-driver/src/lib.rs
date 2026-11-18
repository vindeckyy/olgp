//! First-party OLGP driver helper.
//!
//! This library is loaded into CUDA/NVML client processes before the patched
//! LUPINE shims. It owns the selected box configuration and exposes a small C
//! ABI that `olgpctl` and launchers can call before a GPU workload starts.

use std::ffi::{c_char, c_int, CString};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

const DEFAULT_CONFIG: &str = "/etc/olgp/active-box.json";
const DEFAULT_SHIM_DIR: &str = "/usr/lib/olgp";

static VERSION: &[u8] = b"olgp-driver 0.1.0\0";
static RPC_URL: OnceLock<CString> = OnceLock::new();

/// Path to the active box configuration.
#[must_use]
pub fn config_path() -> PathBuf {
    std::env::var("OLGP_ACTIVE_BOX")
        .map_or_else(|_| PathBuf::from(DEFAULT_CONFIG), PathBuf::from)
}

/// Path to the installed shim directory.
#[must_use]
pub fn shim_dir() -> PathBuf {
    std::env::var("OLGP_SHIM_DIR")
        .map_or_else(|_| PathBuf::from(DEFAULT_SHIM_DIR), PathBuf::from)
}

fn json_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let at = text.find(&needle)?;
    let tail = &text[at + needle.len()..];
    let colon = at + needle.len() + tail.find(':')?;
    let first = text[colon + 1..].find('"')? + colon + 1;
    let second = text[first + 1..].find('"')? + first + 1;
    Some(text[first + 1..second].to_owned())
}

/// Read the configured RPC URL without external dependencies.
///
/// # Errors
/// Returns an error when the config is absent or malformed.
pub fn rpc_url() -> Result<String, String> {
    let text = fs::read_to_string(config_path()).map_err(|e| format!("active box read: {e}"))?;
    json_string(&text, "rpc_url").ok_or_else(|| "active box has no rpc_url".into())
}

/// Verify that the driver configuration and both shims are present.
///
/// # Errors
/// Returns a description of the first missing or invalid component.
pub fn check() -> Result<(), String> {
    let url = rpc_url()?;
    if !url.starts_with("https://") {
        return Err("rpc_url must use https".into());
    }
    for name in ["libcuda.so.1", "libnvidia-ml.so.1"] {
        let path: PathBuf = shim_dir().join(name);
        if !path.is_file() {
            return Err(format!("missing shim: {}", path.display()));
        }
    }
    Ok(())
}

/// C ABI: stable driver version string.
#[unsafe(no_mangle)]
pub extern "C" fn olgp_driver_version() -> *const c_char {
    VERSION.as_ptr().cast()
}

/// C ABI: check driver readiness. Returns 0 on success.
#[unsafe(no_mangle)]
pub extern "C" fn olgp_driver_check() -> c_int {
    match check() {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// C ABI: configured RPC URL, or an empty string when unavailable.
#[unsafe(no_mangle)]
pub extern "C" fn olgp_driver_rpc_url() -> *const c_char {
    RPC_URL
        .get_or_init(|| {
            CString::new(rpc_url().unwrap_or_default()).unwrap_or_default()
        })
        .as_ptr()
}

/// Read a string field from a small JSON object.
#[must_use]
pub fn parse_config_field(text: &str, key: &str) -> Option<String> {
    json_string(text, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    use std::sync::{LazyLock, Mutex};
    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "olgp-driver-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn version_symbol_is_stable() {
        let version = unsafe { std::ffi::CStr::from_ptr(olgp_driver_version()) };
        assert!(version.to_str().unwrap().starts_with("olgp-driver"));
    }

    #[test]
    fn parse_config_field_reads_json_string() {
        assert_eq!(
            parse_config_field("{\"rpc_url\": \"https://box:8843\"}", "rpc_url"),
            Some("https://box:8843".into())
        );
        assert_eq!(parse_config_field("{}", "rpc_url"), None);
    }

    #[test]
    fn check_requires_config_and_shims() {
        let root = temp_root("check");
        let _guard = ENV_LOCK.lock().unwrap();
        let config = root.join("active-box.json");
        let shims = root.join("shims");
        fs::write(&config, "{\"rpc_url\": \"https://box:8843\"}").unwrap();
        fs::create_dir_all(&shims).unwrap();
        fs::write(shims.join("libcuda.so.1"), b"x").unwrap();
        fs::write(shims.join("libnvidia-ml.so.1"), b"x").unwrap();

        unsafe {
            std::env::set_var("OLGP_ACTIVE_BOX", &config);
            std::env::set_var("OLGP_SHIM_DIR", &shims);
        }
        assert!(check().is_ok());
        assert_eq!(olgp_driver_check(), 0);

        fs::remove_file(shims.join("libnvidia-ml.so.1")).unwrap();
        assert!(check().is_err());
        assert_eq!(olgp_driver_check(), 1);

        unsafe {
            std::env::remove_var("OLGP_ACTIVE_BOX");
            std::env::remove_var("OLGP_SHIM_DIR");
        }
    }

    #[test]
    fn check_rejects_non_https_url() {
        let root = temp_root("scheme");
        let _guard = ENV_LOCK.lock().unwrap();
        let config = root.join("active-box.json");
        fs::write(&config, "{\"rpc_url\": \"http://box:8843\"}").unwrap();
        unsafe { std::env::set_var("OLGP_ACTIVE_BOX", &config); }
        assert!(check().is_err());
        unsafe { std::env::remove_var("OLGP_ACTIVE_BOX"); }
    }

    #[test]
    fn rpc_url_symbol_returns_c_string() {
        let root = temp_root("url");
        let _guard = ENV_LOCK.lock().unwrap();
        let config = root.join("active-box.json");
        fs::write(&config, "{\"rpc_url\": \"https://box:8843\"}").unwrap();
        unsafe { std::env::set_var("OLGP_ACTIVE_BOX", &config); }
        assert_eq!(rpc_url().unwrap(), "https://box:8843");
        let ptr = olgp_driver_rpc_url();
        assert!(!ptr.is_null());
        unsafe { std::env::remove_var("OLGP_ACTIVE_BOX"); }
    }
}
