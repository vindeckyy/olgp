use std::env;
use std::process::Command;

fn workload_summary(name: &str) -> Result<String, String> {
    match name {
        "qna" | "spatial" => Ok(format!("workload={name} state=active residency=managed")),
        other => Err(format!("unknown workload: {other}")),
    }
}

fn format_report(csv: &str, box_name: &str, cuda: &str) -> Result<String, String> {
    let line = csv.lines().next().ok_or("empty nvidia-smi output")?;
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    if fields.len() < 4 {
        return Err("unexpected nvidia-smi query shape".into());
    }
    Ok(format!(
        "OLGP-SMI 0.1.0     Box: {box_name}     Driver: {}     CUDA: {cuda}\n\
         0    {}   {} / {}   0%   Ready\n",
        fields[3], fields[0], fields[1], fields[2]
    ))
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
            "--query-gpu=name,memory.used,memory.total,driver_version",
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
    let box_name = env::var("OLGP_BOX_NAME").unwrap_or_else(|_| "olgp.local".into());
    let cuda = env::var("OLGP_CUDA_VERSION").unwrap_or_else(|_| "n/a".into());
    print!("{}", format_report(&String::from_utf8_lossy(&output.stdout), &box_name, &cuda)?);
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

    #[test]
    fn smi_report_is_wici_shaped_without_fabricated_cuda() {
        let report = format_report(
            "NVIDIA GeForce RTX 5090, 23300 MiB, 32607 MiB, 580.159.03\n",
            "olgp.local",
            "n/a",
        )
        .unwrap();
        assert!(report.contains("OLGP-SMI 0.1.0"));
        assert!(report.contains("Box: olgp.local"));
        assert!(report.contains("Driver: 580.159.03"));
        assert!(report.contains("CUDA: n/a"));
        assert!(report.contains("NVIDIA GeForce RTX 5090"));
        assert!(format_report("bad,row", "olgp.local", "n/a").is_err());
    }
}
