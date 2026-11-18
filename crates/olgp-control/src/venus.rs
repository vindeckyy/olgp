//! virtio-gpu Venus virtual GPU configuration.
//!
//! Venus exposes a real `VkPhysicalDevice` inside a Linux guest through
//! virtio-gpu. It is intentionally separate from the CUDA/NVML shim path.

use std::path::{Path, PathBuf};

/// Venus virtual GPU configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VenusConfig {
    pub qemu: PathBuf,
    pub image: PathBuf,
    pub memory_mib: u32,
    pub hostmem_mib: u32,
    pub cpus: u32,
    pub ssh_port: u16,
    pub renderer: Option<PathBuf>,
}

impl VenusConfig {
    /// Validate values before generating commands.
    ///
    /// # Errors
    /// Returns a description when a value is outside the supported range.
    pub fn validate(&self) -> Result<(), String> {
        if self.memory_mib < 1024 {
            return Err("memory_mib must be at least 1024".into());
        }
        if self.hostmem_mib < 256 || self.hostmem_mib > 8 * 1024 {
            return Err("hostmem_mib must be between 256 and 8192".into());
        }
        if self.cpus == 0 || self.cpus > 64 {
            return Err("cpus must be between 1 and 64".into());
        }
        if self.ssh_port == 0 {
            return Err("ssh_port must be nonzero".into());
        }
        Ok(())
    }

    /// Build the QEMU command for a Venus-enabled guest.
    ///
    /// # Errors
    /// Returns a description when the configuration is invalid.
    pub fn qemu_command(&self) -> Result<Vec<String>, String> {
        self.validate()?;
        let device = format!(
            "virtio-gpu-gl,hostmem={}M,blob=true,venus=true",
            self.hostmem_mib
        );
        let memory = format!("{}M", self.memory_mib);
        let hostfwd = format!("hostfwd=tcp::{}-:22", self.ssh_port);
        let mut command = vec![
            self.qemu.to_string_lossy().into_owned(),
            "-enable-kvm".into(),
            "-M".into(),
            "q35".into(),
            "-smp".into(),
            self.cpus.to_string(),
            "-m".into(),
            memory,
            "-cpu".into(),
            "host".into(),
            "-device".into(),
            device,
            "-vga".into(),
            "none".into(),
            "-display".into(),
            "sdl,gl=on,show-cursor=on".into(),
            "-netdev".into(),
            format!("user,{hostfwd}"),
            "-device".into(),
            "virtio-net-pci,netdev=net0".into(),
        ];
        if let Some(renderer) = &self.renderer {
            command.push("-display".into());
            command.push(format!("gl=on,glx-ptr=never,renderer={}", renderer.display()));
        }
        command.push("-hda".into());
        command.push(self.image.to_string_lossy().into_owned());
        Ok(command)
    }
}

/// Parse a `key=value` configuration file for Venus.
///
/// # Errors
/// Returns a description when a required field is missing or malformed.
pub fn load_config(path: &Path) -> Result<VenusConfig, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("config read: {e}"))?;
    let mut values = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or("malformed venus config")?;
        values.insert(key.trim().to_owned(), value.trim().trim_matches('"').to_owned());
    }
    let get = |key: &str| values.get(key).cloned().ok_or_else(|| format!("missing {key}"));
    Ok(VenusConfig {
        qemu: PathBuf::from(get("qemu")?),
        image: PathBuf::from(get("image")?),
        memory_mib: get("memory_mib")?.parse().map_err(|_| "bad memory_mib")?,
        hostmem_mib: get("hostmem_mib")?.parse().map_err(|_| "bad hostmem_mib")?,
        cpus: get("cpus")?.parse().map_err(|_| "bad cpus")?,
        ssh_port: get("ssh_port")?.parse().map_err(|_| "bad ssh_port")?,
        renderer: values.get("renderer").map(PathBuf::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> VenusConfig {
        VenusConfig {
            qemu: PathBuf::from("/usr/bin/qemu-system-x86_64"),
            image: PathBuf::from("/images/guest.qcow2"),
            memory_mib: 4096,
            hostmem_mib: 4096,
            cpus: 4,
            ssh_port: 2222,
            renderer: None,
        }
    }

    #[test]
    fn command_contains_venus_device() {
        let command = config().qemu_command().unwrap();
        assert!(command.iter().any(|a| a.contains("venus=true")));
        assert!(command.iter().any(|a| a.contains("blob=true")));
        assert!(command.iter().any(|a| a == "virtio-net-pci,netdev=net0"));
    }

    #[test]
    fn renderer_is_optional() {
        let mut cfg = config();
        cfg.renderer = Some(PathBuf::from("/usr/bin/virgl_test_server"));
        let command = cfg.qemu_command().unwrap();
        assert!(command.iter().any(|a| a.contains("virgl_test_server")));
    }

    #[test]
    fn validation_rejects_bad_values() {
        let mut cfg = config();
        cfg.memory_mib = 512;
        assert!(cfg.validate().is_err());
        let mut cfg = config();
        cfg.hostmem_mib = 16;
        assert!(cfg.qemu_command().is_err());
        let mut cfg = config();
        cfg.cpus = 0;
        assert!(cfg.validate().is_err());
        let mut cfg = config();
        cfg.ssh_port = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn load_config_reads_all_fields() {
        let root = std::env::temp_dir().join(format!("olgp-venus-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("venus.toml");
        std::fs::write(
            &path,
            "qemu=/usr/bin/qemu-system-x86_64\nimage=/images/guest.qcow2\nmemory_mib=4096\nhostmem_mib=4096\ncpus=4\nssh_port=2222\nrenderer=/usr/bin/virgl_test_server\n",
        )
        .unwrap();
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.cpus, 4);
        assert!(cfg.renderer.is_some());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn load_config_rejects_bad_input() {
        let root = std::env::temp_dir().join(format!("olgp-venus-bad-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("venus.toml");
        std::fs::write(&path, "bad line\n").unwrap();
        assert!(load_config(&path).is_err());
        std::fs::write(&path, "qemu=x\n").unwrap();
        assert!(load_config(&path).is_err());
        std::fs::write(
            &path,
            "qemu=x\nimage=y\nmemory_mib=4096\nhostmem_mib=4096\ncpus=4\nssh_port=2222\n",
        )
        .unwrap();
        let cfg = load_config(&path).unwrap();
        assert!(cfg.renderer.is_none());
        std::fs::remove_file(path).unwrap();
    }
}
