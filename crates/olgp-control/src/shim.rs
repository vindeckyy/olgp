//! System-wide CUDA/NVML shim installation.
//!
//! The paths are injectable so the behavior can be tested without touching the
//! host's dynamic linker configuration.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Paths used by attach/detach.
#[derive(Clone, Debug)]
pub struct ShimPaths {
    pub config: PathBuf,
    pub preload: PathBuf,
    pub backup: PathBuf,
    pub lib_dir: PathBuf,
}

impl Default for ShimPaths {
    fn default() -> Self {
        Self {
            config: PathBuf::from("/etc/olgp/active-box.json"),
            preload: PathBuf::from("/etc/ld.so.preload"),
            backup: PathBuf::from("/etc/ld.so.preload.olgp.bak"),
            lib_dir: PathBuf::from("/usr/lib/olgp"),
        }
    }
}

/// The selected box written into the active configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveBox {
    pub box_id: String,
    pub rpc_url: String,
    pub spki_sha256: String,
}

/// Errors from shim installation.
#[derive(Debug)]
pub enum ShimError {
    ForeignPreload(String),
    Io(io::Error),
}

impl std::fmt::Display for ShimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ForeignPreload(line) => {
                write!(f, "foreign ld.so.preload entry present: {line}")
            }
            Self::Io(error) => write!(f, "shim IO error: {error}"),
        }
    }
}

impl std::error::Error for ShimError {}

impl From<io::Error> for ShimError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn olgp_lines(paths: &ShimPaths) -> [PathBuf; 3] {
    [
        paths.lib_dir.join("libolgp_driver.so"),
        paths.lib_dir.join("libcuda.so.1"),
        paths.lib_dir.join("libnvidia-ml.so.1"),
    ]
}

fn read_preload(path: &Path) -> Result<Vec<String>, ShimError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    Ok(text.lines().map(str::to_owned).collect())
}

/// Install the system-wide shim configuration.
pub fn attach(paths: &ShimPaths, box_info: &ActiveBox, force: bool) -> Result<(), ShimError> {
    if let Some(parent) = paths.config.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = format!(
        "{{\n  \"rpc_url\": \"{}\",\n  \"box_id\": \"{}\",\n  \"spki_sha256\": \"{}\"\n}}\n",
        box_info.rpc_url, box_info.box_id, box_info.spki_sha256
    );
    fs::write(&paths.config, json)?;
    fs::set_permissions(&paths.config, fs::Permissions::from_mode(0o644))?;

    let olgp = olgp_lines(paths);
    let existing = read_preload(&paths.preload)?;
    let foreign: Vec<&String> = existing
        .iter()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !olgp.iter().any(|path| path.to_string_lossy() == line.trim()))
        .collect();
    if !foreign.is_empty() && !force {
        return Err(ShimError::ForeignPreload(foreign[0].clone()));
    }

    if existing.iter().any(|line| !line.trim().is_empty()) && !paths.backup.exists() {
        fs::copy(&paths.preload, &paths.backup)?;
    }
    let mut lines: Vec<String> = existing
        .iter()
        .filter(|line| olgp.iter().any(|path| path.to_string_lossy() == line.trim()))
        .cloned()
        .collect();
    for path in olgp {
        let line = path.to_string_lossy().into_owned();
        if !lines.iter().any(|item| item == &line) {
            lines.push(line);
        }
    }
    fs::write(&paths.preload, format!("{}\n", lines.join("\n")))?;
    fs::set_permissions(&paths.preload, fs::Permissions::from_mode(0o644))?;
    Ok(())
}

/// Install staged CUDA and NVML shims into the system library directory.
///
/// # Errors
/// Returns an IO error when a staged library is missing or cannot be copied.
pub fn install_shims(stage_dir: &Path, lib_dir: &Path) -> Result<(), ShimError> {
    fs::create_dir_all(lib_dir)?;
    for name in ["libolgp_driver.so", "libcuda.so.1", "libnvidia-ml.so.1"] {
        let source = stage_dir.join(name);
        let target = lib_dir.join(name);
        fs::copy(&source, &target)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
/// Remove only the shim lines installed by `attach`.
pub fn detach(paths: &ShimPaths) -> Result<(), ShimError> {
    if paths.backup.exists() {
        let original = fs::read(&paths.backup)?;
        fs::write(&paths.preload, original)?;
        fs::remove_file(&paths.backup)?;
    } else {
        let olgp = olgp_lines(paths);
        let remaining: Vec<String> = read_preload(&paths.preload)?
            .into_iter()
            .filter(|line| !olgp.iter().any(|path| path.to_string_lossy() == line.trim()))
            .collect();
        if remaining.is_empty() {
            if paths.preload.exists() {
                fs::remove_file(&paths.preload)?;
            }
        } else {
            fs::write(&paths.preload, format!("{}\n", remaining.join("\n")))?;
        }
    }
    if paths.config.exists() {
        fs::remove_file(&paths.config)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temp_paths() -> ShimPaths {
        let unique = format!(
            "olgp-shim-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(&root).unwrap();
        ShimPaths {
            config: root.join("active-box.json"),
            preload: root.join("ld.so.preload"),
            backup: root.join("ld.so.preload.olgp.bak"),
            lib_dir: root.join("lib"),
        }
    }

    fn box_info() -> ActiveBox {
        ActiveBox {
            box_id: "abc".into(),
            rpc_url: "https://olgp.local:8843".into(),
            spki_sha256: "deadbeef".into(),
        }
    }

    #[test]
    fn attach_writes_config_and_three_preload_lines() {
        let paths = temp_paths();
        attach(&paths, &box_info(), false).unwrap();
        let config = fs::read_to_string(&paths.config).unwrap();
        assert!(config.contains("https://olgp.local:8843"));
        let preload = fs::read_to_string(&paths.preload).unwrap();
        assert_eq!(preload.lines().count(), 3);
        assert!(preload.contains("libcuda.so.1"));
    }

    #[test]
    fn attach_refuses_foreign_preload_without_force() {
        let paths = temp_paths();
        fs::write(&paths.preload, "/usr/lib/other.so\n").unwrap();
        assert!(matches!(
            attach(&paths, &box_info(), false),
            Err(ShimError::ForeignPreload(_))
        ));
        attach(&paths, &box_info(), true).unwrap();
        assert!(paths.backup.exists());
    }

    #[test]
    fn detach_restores_original_preload() {
        let paths = temp_paths();
        let original = "/usr/lib/other.so\n";
        fs::write(&paths.preload, original).unwrap();
        attach(&paths, &box_info(), true).unwrap();
        detach(&paths).unwrap();
        assert_eq!(fs::read_to_string(&paths.preload).unwrap(), original);
        assert!(!paths.config.exists());
    }

    #[test]
    fn attach_is_idempotent() {
        let paths = temp_paths();
        attach(&paths, &box_info(), false).unwrap();
        attach(&paths, &box_info(), false).unwrap();
        let preload = fs::read_to_string(&paths.preload).unwrap();
        assert_eq!(preload.lines().count(), 3);
    }

    #[test]
    fn detach_without_backup_removes_only_olgp_lines() {
        let paths = temp_paths();
        fs::write(&paths.preload, "/usr/lib/other.so\n").unwrap();
        attach(&paths, &box_info(), true).unwrap();
        fs::remove_file(&paths.backup).unwrap();
        detach(&paths).unwrap();
        assert!(!paths.preload.exists());
    }

    #[test]
    fn detach_without_preload_is_clean() {
        let paths = temp_paths();
        detach(&paths).unwrap();
        assert!(!paths.preload.exists());
    }

    #[test]
    fn default_paths_are_stable() {
        let paths = ShimPaths::default();
        assert_eq!(paths.preload, PathBuf::from("/etc/ld.so.preload"));
        assert_eq!(paths.config, PathBuf::from("/etc/olgp/active-box.json"));
    }

    #[test]
    fn error_messages_are_stable() {
        let foreign = ShimError::ForeignPreload("/x.so".into());
        assert!(format!("{foreign}").contains("/x.so"));
        let io_error: ShimError = io::Error::other("broken").into();
        assert!(format!("{io_error}").contains("shim IO error"));
    }

    #[test]
    fn attach_reports_invalid_parent() {
        let mut paths = temp_paths();
        paths.config = PathBuf::from("/dev/null/olgp/active-box.json");
        assert!(matches!(
            attach(&paths, &box_info(), false),
            Err(ShimError::Io(_))
        ));
    }

    #[test]
    fn detach_preserves_foreign_lines() {
        let paths = temp_paths();
        let olgp = olgp_lines(&paths);
        let existing = format!(
            "/usr/lib/other.so\n{}\n{}\n",
            olgp[0].to_string_lossy(),
            olgp[1].to_string_lossy()
        );
        fs::write(&paths.preload, existing).unwrap();
        detach(&paths).unwrap();
        assert_eq!(
            fs::read_to_string(&paths.preload).unwrap(),
            "/usr/lib/other.so\n"
        );
    }

    #[test]
    fn install_shims_copies_all_libraries() {
        let root = std::env::temp_dir().join(format!("olgp-install-{}", std::process::id()));
        let stage = root.join("stage");
        let lib = root.join("lib");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("libcuda.so.1"), b"cuda").unwrap();
        fs::write(stage.join("libolgp_driver.so"), b"driver").unwrap();
        fs::write(stage.join("libnvidia-ml.so.1"), b"nvml").unwrap();
        install_shims(&stage, &lib).unwrap();
        assert_eq!(fs::read(lib.join("libcuda.so.1")).unwrap(), b"cuda");
        assert_eq!(fs::read(lib.join("libolgp_driver.so")).unwrap(), b"driver");
        assert_eq!(fs::read(lib.join("libnvidia-ml.so.1")).unwrap(), b"nvml");
    }
}
