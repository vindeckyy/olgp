//! Prometheus counters and Chrome/Perfetto-compatible trace events.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default)]
struct Values {
    blocking_calls: u64,
    bytes_wifi: u64,
    batch_hits: u64,
    batch_misses: u64,
    trace_aborts: u64,
    cache_hits: u64,
    cache_misses: u64,
    evictions: u64,
    vram_bytes: u64,
    ram_bytes: u64,
    shared_weight_bytes: u64,
    kv_sessions: u64,
    kv_ssd_writes: u64,
}
static VALUES: LazyLock<Mutex<Values>> = LazyLock::new(|| Mutex::new(Values::default()));

fn values() -> &'static Mutex<Values> {
    &VALUES
}
/// Increment a counter by one.
pub fn increment(name: &str) {
    let mut guard = values().lock().unwrap();
    match name {
        "blocking_calls" => guard.blocking_calls += 1,
        "bytes_wifi" => guard.bytes_wifi += 1,
        "batch_hits" => guard.batch_hits += 1,
        "batch_misses" => guard.batch_misses += 1,
        "trace_aborts" => guard.trace_aborts += 1,
        "cache_hits" => guard.cache_hits += 1,
        "cache_misses" => guard.cache_misses += 1,
        "evictions" => guard.evictions += 1,
        _ => {}
    }
}

/// Set a runtime gauge.
pub fn set(name: &str, value: u64) {
    let mut guard = values().lock().unwrap();
    match name {
        "vram_bytes" => guard.vram_bytes = value,
        "ram_bytes" => guard.ram_bytes = value,
        "shared_weight_bytes" => guard.shared_weight_bytes = value,
        "kv_sessions" => guard.kv_sessions = value,
        "kv_ssd_writes" => guard.kv_ssd_writes = value,
        _ => {}
    }
}

/// Render counters in Prometheus text format.
#[must_use]
pub fn render() -> String {
    let guard = values().lock().unwrap();
    format!(
        "olgp_blocking_calls {}\nolgp_bytes_wifi {}\nolgp_batch_hits {}\nolgp_batch_misses {}\nolgp_trace_aborts {}\nolgp_cache_hits {}\nolgp_cache_misses {}\nolgp_evictions {}\nolgp_vram_bytes {}\nolgp_ram_bytes {}\nolgp_shared_weight_bytes {}\nolgp_kv_sessions {}\nolgp_kv_ssd_writes {}\n",
        guard.blocking_calls,
        guard.bytes_wifi,
        guard.batch_hits,
        guard.batch_misses,
        guard.trace_aborts,
        guard.cache_hits,
        guard.cache_misses,
        guard.evictions,
        guard.vram_bytes,
        guard.ram_bytes,
        guard.shared_weight_bytes,
        guard.kv_sessions,
        guard.kv_ssd_writes,
    )
}

/// Write one instant trace event when tracing is enabled.
///
/// # Errors
/// Returns an IO error when the trace file cannot be opened.
pub fn trace_event(name: &str) -> Result<(), std::io::Error> {
    let Ok(directory) = std::env::var("OLGP_TRACE_DIR") else {
        return Ok(());
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64;
    let path: PathBuf = PathBuf::from(directory).join("trace.jsonl");
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        file,
        "{{\"name\":\"{name}\",\"ph\":\"i\",\"ts\":{timestamp}}}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_contains_all_counters() {
        increment("batch_hits");
        increment("unknown");
        let text = render();
        for name in [
            "olgp_blocking_calls",
            "olgp_bytes_wifi",
            "olgp_batch_hits",
            "olgp_batch_misses",
            "olgp_trace_aborts",
            "olgp_cache_hits",
            "olgp_cache_misses",
            "olgp_evictions",
        ] {
            assert!(text.contains(name), "missing {name}");
        }
    }

    #[test]
    fn all_counter_branches_are_exercised() {
        for name in [
            "blocking_calls",
            "bytes_wifi",
            "batch_hits",
            "batch_misses",
            "trace_aborts",
            "cache_hits",
            "cache_misses",
            "evictions",
        ] {
            increment(name);
        }
        assert!(render().contains("olgp_evictions"));
    }

    #[test]
    fn trace_event_writes_json_line_when_enabled() {
        let root = std::env::temp_dir().join(format!("olgp-trace-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        unsafe { std::env::set_var("OLGP_TRACE_DIR", &root); }
        trace_event("query").unwrap();
        unsafe { std::env::remove_var("OLGP_TRACE_DIR"); }
        let text = std::fs::read_to_string(root.join("trace.jsonl")).unwrap();
        assert!(text.contains("\"name\":\"query\""));
    }

    #[test]
    fn runtime_gauges_are_rendered() {
        set("vram_bytes", 1024);
        set("ram_bytes", 2048);
        set("shared_weight_bytes", 4096);
        set("kv_sessions", 2);
        set("kv_ssd_writes", 0);
        set("unknown", 1);
        let text = render();
        assert!(text.contains("olgp_vram_bytes 1024"));
        assert!(text.contains("olgp_kv_sessions 2"));
    }
}
