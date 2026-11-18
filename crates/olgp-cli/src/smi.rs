use std::env;
use std::process::Command;

fn workload_summary(name: &str) -> Result<String, String> {
    match name {
        "qna" | "spatial" => Ok(format!("workload={name} state=active residency=managed")),
        other => Err(format!("unknown workload: {other}")),
    }
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Some(index) = args.iter().position(|arg| arg == "--workload") {
        let name = args.get(index + 1).ok_or("missing workload name")?;
        println!("{}", workload_summary(name)?);
        return Ok(());
    }

    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.used,memory.total",
            "--format=csv,noheader",
        ])
        .output()
        .map_err(|e| format!("nvidia-smi: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "nvidia-smi failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_workloads_have_managed_residency() {
        assert!(workload_summary("qna").unwrap().contains("residency=managed"));
        assert!(workload_summary("spatial").unwrap().contains("state=active"));
        assert!(workload_summary("unknown").is_err());
    }
}
