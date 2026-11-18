//! Honest image-job lifecycle backed by a local JSON file.
//!
//! No image model is bundled or provisioned here. Jobs stay in a provisioning
//! state until a real model is supplied, and results are returned only from
//! bytes actually stored on disk.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const OPERATIONS: [&str; 5] = ["generate", "edit", "face_swap", "reframe", "detect"];

fn validate_request(operation: &str, request: &str) -> Result<(), String> {
    let value: Value = serde_json::from_str(request).map_err(|_| "invalid_request".to_owned())?;
    let parameters = value.get("parameters");
    let mut integer = |key: &str| -> Option<u64> {
        parameters.and_then(|p| p.get(key)).and_then(Value::as_u64)
    };
    if let (Some(width), Some(height)) = (integer("width"), integer("height")) {
        let (min, max, step) = if operation == "reframe" { (96, 1280, 1) } else { (64, 2048, 64) };
        for dimension in [width, height] {
            if !(min..=max).contains(&dimension) || dimension % step != 0 {
                return Err(format!("invalid_parameter: {dimension}"));
            }
        }
    }
    if let Some(steps) = integer("steps") {
        if !(1..=100).contains(&steps) {
            return Err("invalid_parameter: steps".into());
        }
    }
    if let Some(fps) = integer("fps") {
        if !(8..=60).contains(&fps) {
            return Err("invalid_parameter: fps".into());
        }
    }
    if let Some(frames) = integer("frames") {
        if !(8..=240).contains(&frames) {
            return Err("invalid_parameter: frames".into());
        }
    }
    if let Some(inputs) = value.get("inputs").and_then(Value::as_object) {
        for (key, input) in inputs {
            if key == "image" || key == "source" || key == "target" {
                let text = input.as_str().unwrap_or("");
                if text.len() > 20 * 1024 * 1024 {
                    return Err("invalid_parameter: input too large".into());
                }
            }
        }
    }
    Ok(())
}

/// One image job. Nothing in this module advances a job to `succeeded`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ImageJob {
    pub id: String,
    pub operation: String,
    pub status: String,
    pub phase: String,
    pub error_code: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    next_id: u64,
    jobs: BTreeMap<String, ImageJob>,
    // Bytes and media type written only by a real model; never synthesized.
    results: BTreeMap<String, (Vec<u8>, String)>,
}

/// Image jobs stored under a caller-supplied path.
#[derive(Debug)]
pub struct ImageStore {
    path: PathBuf,
    state: PersistedState,
}

fn write_state(state: &PersistedState, path: &Path) -> Result<(), String> {
    let text = serde_json::to_string_pretty(state).map_err(|e| format!("image state encode: {e}"))?;
    fs::write(path, text).map_err(|e| format!("image state write: {e}"))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("image state permissions: {e}"))
}

impl ImageStore {
    /// Load state from `path`, creating an empty owner-only file when absent.
    ///
    /// # Errors
    /// Returns a message when the parent directory cannot be created, the file
    /// cannot be read or written, or the stored JSON is invalid.
    pub fn load(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("image state directory: {e}"))?;
        }
        let missing = !path.exists();
        let state = if missing {
            PersistedState::default()
        } else {
            let text = fs::read_to_string(path).map_err(|e| format!("image state read: {e}"))?;
            serde_json::from_str(&text).map_err(|e| format!("image state parse: {e}"))?
        };
        if missing {
            write_state(&state, path)?;
        }
        Ok(Self { path: path.to_path_buf(), state })
    }

    /// Create a job for a supported operation.
    ///
    /// With no model provisioned, every job is `model_missing` in the
    /// `provisioning_required` phase and produces no media on its own.
    ///
    /// # Errors
    /// Returns `unsupported_operation` for an operation outside the allowlist,
    /// `invalid_request` for an empty request, or an IO/encode message when the
    /// state file cannot be updated.
    pub fn create(&mut self, operation: &str, request: &str) -> Result<ImageJob, String> {
        if !OPERATIONS.contains(&operation) {
            return Err(format!("unsupported_operation: {operation}"));
        }
        if request.trim().is_empty() {
            return Err("invalid_request".to_owned());
        }
        validate_request(operation, request)?;
        let mut next = self.state.clone();
        next.next_id += 1;
        let job = ImageJob {
            id: format!("img-{}", next.next_id),
            operation: operation.to_owned(),
            status: "model_missing".to_owned(),
            phase: "provisioning_required".to_owned(),
            error_code: Some("model_missing".to_owned()),
        };
        next.jobs.insert(job.id.clone(), job.clone());
        write_state(&next, &self.path)?;
        self.state = next;
        Ok(job)
    }

    /// Look up a job by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ImageJob> {
        self.state.jobs.get(id)
    }

    /// Return the bytes and media type stored for a succeeded job.
    ///
    /// Missing jobs, unfinished jobs, and succeeded jobs without stored bytes
    /// all fail; no placeholder media is produced.
    ///
    /// # Errors
    /// Returns `unknown_job` for an unknown id and `result_unavailable` unless
    /// the job succeeded with stored bytes.
    pub fn result(&self, id: &str) -> Result<(Vec<u8>, String), String> {
        let job = self.state.jobs.get(id).ok_or_else(|| "unknown_job".to_owned())?;
        if job.status != "succeeded" {
            return Err("result_unavailable".to_owned());
        }
        self.state
            .results
            .get(id)
            .cloned()
            .ok_or_else(|| "result_unavailable".to_owned())
    }

    /// Cancel a job, moving it to `cancelled` / `done`.
    ///
    /// Returns `None` when `id` is unknown.
    ///
    /// # Errors
    /// Returns an IO/encode message when the state file cannot be updated.
    pub fn cancel(&mut self, id: &str) -> Result<Option<ImageJob>, String> {
        let Some(current) = self.state.jobs.get(id) else {
            return Ok(None);
        };
        let mut updated = current.clone();
        updated.status = "cancelled".to_owned();
        updated.phase = "done".to_owned();
        let mut next = self.state.clone();
        next.jobs.insert(id.to_owned(), updated.clone());
        write_state(&next, &self.path)?;
        self.state = next;
        Ok(Some(updated))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    struct StateFile {
        dir: PathBuf,
    }

    impl StateFile {
        fn new(name: &str) -> Self {
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "olgp-images-{}-{}-{}",
                name,
                std::process::id(),
                seq
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self { dir }
        }

        fn path(&self) -> PathBuf {
            self.dir.join("jobs.json")
        }
    }

    impl Drop for StateFile {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn create_persists_provisioning_state() {
        let file = StateFile::new("create");
        let mut store = ImageStore::load(&file.path()).unwrap();
        let job = store.create("generate", "{}").unwrap();
        assert_eq!(job.id, "img-1");
        assert_eq!(job.operation, "generate");
        assert_eq!(job.status, "model_missing");
        assert_eq!(job.phase, "provisioning_required");
        assert_eq!(job.error_code.as_deref(), Some("model_missing"));

        let reloaded = ImageStore::load(&file.path()).unwrap();
        assert_eq!(reloaded.get("img-1"), Some(&job));
        assert!(reloaded.get("missing").is_none());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(file.path()).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn unsupported_operation_or_empty_request_is_rejected() {
        let file = StateFile::new("reject");
        let mut store = ImageStore::load(&file.path()).unwrap();
        assert_eq!(store.create("upscale", "{}"), Err("unsupported_operation: upscale".into()));
        assert_eq!(store.create("generate", "  "), Err("invalid_request".into()));
        for op in OPERATIONS {
            assert!(store.create(op, "{}").is_ok());
        }
        assert_eq!(store.get("img-5").map(|j| j.operation.as_str()), Some("detect"));
    }

    #[test]
    fn result_is_unavailable_for_unfinished_jobs() {
        let file = StateFile::new("result");
        let mut store = ImageStore::load(&file.path()).unwrap();
        let job = store.create("edit", "{}").unwrap();
        assert_eq!(store.result(&job.id), Err("result_unavailable".into()));
        assert_eq!(store.result("missing"), Err("unknown_job".into()));
        store.cancel(&job.id).unwrap();
        assert_eq!(store.result(&job.id), Err("result_unavailable".into()));
    }

    #[test]
    fn cancel_marks_job_done_and_persists() {
        let file = StateFile::new("cancel");
        let mut store = ImageStore::load(&file.path()).unwrap();
        let job = store.create("face_swap", "{}").unwrap();
        let cancelled = store.cancel(&job.id).unwrap().unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert_eq!(cancelled.phase, "done");
        assert!(store.cancel("missing").unwrap().is_none());

        let reloaded = ImageStore::load(&file.path()).unwrap();
        assert_eq!(reloaded.get(&job.id), Some(&cancelled));
    }

    #[test]
    fn ids_keep_incrementing_after_reload() {
        let file = StateFile::new("ids");
        let mut store = ImageStore::load(&file.path()).unwrap();
        store.create("generate", "{}").unwrap();
        store.create("generate", "{}").unwrap();
        let mut store = ImageStore::load(&file.path()).unwrap();
        assert_eq!(store.create("generate", "{}").unwrap().id, "img-3");
    }

    #[test]
    fn result_returns_only_bytes_read_from_disk() {
        let file = StateFile::new("stored");
        let bytes = vec![0x89, 0x50, 0x4e, 0x47];
        let mut state = PersistedState {
            next_id: 1,
            jobs: BTreeMap::from([(
                "img-1".to_owned(),
                ImageJob {
                    id: "img-1".to_owned(),
                    operation: "generate".to_owned(),
                    status: "succeeded".to_owned(),
                    phase: "done".to_owned(),
                    error_code: None,
                },
            )]),
            results: BTreeMap::from([("img-1".to_owned(), (bytes.clone(), "image/png".to_owned()))]),
        };
        write_state(&state, &file.path()).unwrap();
        let store = ImageStore::load(&file.path()).unwrap();
        assert_eq!(store.result("img-1").unwrap(), (bytes, "image/png".to_owned()));

        state.results.clear();
        write_state(&state, &file.path()).unwrap();
        let store = ImageStore::load(&file.path()).unwrap();
        assert_eq!(store.result("img-1"), Err("result_unavailable".into()));
    }

    #[test]
    fn corrupt_state_is_reported() {
        let file = StateFile::new("corrupt");
        fs::write(file.path(), "not json").unwrap();
        assert!(ImageStore::load(&file.path()).is_err());
    }

    #[test]
    fn image_parameter_bounds_are_enforced() {
        let file = StateFile::new("bounds");
        let mut store = ImageStore::load(&file.path()).unwrap();
        assert!(store.create("generate", "{\"parameters\":{\"width\":512,\"height\":512}}").is_ok());
        assert!(store.create("generate", "{\"parameters\":{\"width\":100,\"height\":512}}").is_err());
        assert!(store.create("generate", "{\"parameters\":{\"steps\":101}}").is_err());
        assert!(store.create("reframe", "{\"parameters\":{\"width\":100,\"height\":100}}").is_ok());
        assert!(store.create("reframe", "{\"parameters\":{\"fps\":4}}").is_err());
    }

    #[test]
    fn image_validation_branches_are_exercised() {
        let file = StateFile::new("bounds2");
        let mut store = ImageStore::load(&file.path()).unwrap();
        assert!(store.create("reframe", "{\"parameters\":{\"fps\":8,\"frames\":8}}").is_ok());
        assert!(store.create("reframe", "{\"parameters\":{\"frames\":241}}").is_err());
        assert!(store.create("detect", "{\"inputs\":{\"image\":\"data:image/png;base64,AA\"}}").is_ok());
        let huge = "x".repeat(20 * 1024 * 1024 + 1);
        assert!(store.create("detect", &format!("{{\"inputs\":{{\"image\":\"{huge}\"}}}}")).is_err());
        assert!(store.create("generate", "not-json").is_err());
    }
}
