//! Link measurement and selection for multi-link local transport.

use std::fs;
use std::path::Path;

/// Measured link state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    pub name: String,
    pub rtt_us: u64,
    pub loss_pct: u32,
    pub throughput_mbps: u64,
    pub active: bool,
}

impl Link {
    fn score(&self) -> u64 {
        if !self.active {
            return u64::MAX;
        }
        self.rtt_us + u64::from(self.loss_pct) * 10_000 + 1_000_000 / self.throughput_mbps.max(1)
    }
}

/// Select the best active link.
#[must_use]
pub fn select<'a>(links: &'a [Link]) -> Option<&'a Link> {
    links.iter().min_by_key(|link| link.score()).filter(|link| link.active)
}

/// Parse a simple `name,rtt_us,loss_pct,throughput_mbps,active` CSV file.
///
/// # Errors
/// Returns an error when a row is malformed.
pub fn load(path: &Path) -> Result<Vec<Link>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut links = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<&str> = line.trim().split(',').collect();
        if parts.len() != 5 {
            return Err(format!("malformed link row: {line}"));
        }
        links.push(Link {
            name: parts[0].to_owned(),
            rtt_us: parts[1].parse().map_err(|_| "bad rtt_us")?,
            loss_pct: parts[2].parse().map_err(|_| "bad loss_pct")?,
            throughput_mbps: parts[3].parse().map_err(|_| "bad throughput_mbps")?,
            active: parts[4] == "1",
        });
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(name: &str, rtt: u64, loss: u32, mbps: u64, active: bool) -> Link {
        Link { name: name.into(), rtt_us: rtt, loss_pct: loss, throughput_mbps: mbps, active }
    }

    #[test]
    fn selects_lowest_latency_active_link() {
        let links = [
            link("wifi7-a", 900, 0, 1000, true),
            link("wifi7-b", 1200, 0, 1000, true),
            link("eth", 500, 0, 1000, false),
        ];
        assert_eq!(select(&links).unwrap().name, "wifi7-a");
    }

    #[test]
    fn loss_penalty_changes_selection() {
        let links = [
            link("a", 900, 5, 1000, true),
            link("b", 1200, 0, 1000, true),
        ];
        assert_eq!(select(&links).unwrap().name, "b");
    }

    #[test]
    fn no_active_link_returns_none() {
        let links = [link("a", 100, 0, 1000, false)];
        assert!(select(&links).is_none());
    }

    #[test]
    fn parses_link_file() {
        let root = std::env::temp_dir().join(format!("olgp-links-{}", std::process::id()));
        let path = root.join("links.csv");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&path, "wifi7-a,900,0,1000,1\neth,500,0,1000,0\n").unwrap();
        let links = load(&path).unwrap();
        assert_eq!(links.len(), 2);
        assert_eq!(select(&links).unwrap().name, "wifi7-a");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_malformed_rows() {
        let root = std::env::temp_dir().join(format!("olgp-links-bad-{}", std::process::id()));
        let path = root.join("links.csv");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&path, "bad,row\n").unwrap();
        assert!(load(&path).is_err());
        std::fs::write(&path, "a,not-a-number,0,1000,1\n").unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
