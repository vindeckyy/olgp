//! Product-facing authenticated routes layered on the protocol gateway.
//!
//! This module keeps first-party device state local to the box. Preview
//! operations report honest unavailability instead of synthesizing results.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::household::{Household, Memo, Timer};
use crate::images::ImageStore;
use crate::memory::{AudioMemory, Settings};
use crate::rag::RagStore;
use crate::recordings::RecordingStore;
use crate::speaker::SpeakerStore;

/// State backing the product routes.
pub struct ProductState {
    household: Mutex<Household>,
    memory: Mutex<AudioMemory>,
    speakers: Mutex<SpeakerStore>,
    rag: Mutex<RagStore>,
    images: Mutex<ImageStore>,
    recordings: Mutex<RecordingStore>,
}

fn env_path(name: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| default.to_owned()))
}

impl ProductState {
    /// Load all product state from environment-configured paths.
    ///
    /// # Errors
    /// Returns the first store that cannot be opened.
    pub fn load() -> Result<Self, String> {
        Ok(Self {
            household: Mutex::new(Household::load(&env_path(
                "OLGP_HOUSEHOLD_PATH",
                "/var/lib/olgp/household.json",
            ))?),
            memory: Mutex::new(AudioMemory::load(&env_path(
                "OLGP_MEMORY_PATH",
                "/var/lib/olgp/audio-memory.json",
            ))?),
            speakers: Mutex::new(SpeakerStore::load(&env_path(
                "OLGP_SPEAKER_PATH",
                "/var/lib/olgp/speakers.json",
            ))?),
            rag: Mutex::new(RagStore::load(&env_path(
                "OLGP_RAG_PATH",
                "/var/lib/olgp/knowledge-bases.json",
            ))?),
            images: Mutex::new(ImageStore::load(&env_path(
                "OLGP_IMAGES_PATH",
                "/var/lib/olgp/images.json",
            ))?),
            recordings: Mutex::new(RecordingStore::load(&env_path(
                "OLGP_RECORDINGS_ROOT",
                "/var/lib/olgp/recordings",
            ))?),
        })
    }

    /// Load product state from explicit paths, used by deterministic tests.
    ///
    /// # Errors
    /// Returns the first store that cannot be opened.
    #[cfg(test)]
    pub fn load_from_paths(root: &std::path::Path) -> Result<Self, String> {
        Ok(Self {
            household: Mutex::new(Household::load(&root.join("household.json"))?),
            memory: Mutex::new(AudioMemory::load(&root.join("memory.json"))?),
            speakers: Mutex::new(SpeakerStore::load(&root.join("speakers.json"))?),
            rag: Mutex::new(RagStore::load(&root.join("rag.json"))?),
            images: Mutex::new(ImageStore::load(&root.join("images.json"))?),
            recordings: Mutex::new(RecordingStore::load(&root.join("recordings"))?),
        })
    }

    /// Handle one product route.
    ///
    /// Returns `None` when the route belongs to another subsystem.
    #[must_use]
    pub fn handle(&self, method: &str, path: &str, head: &str, body: &str) -> Option<Vec<u8>> {
        if path.starts_with("/v1/audio-memory") && !memory_authorized(head) {
            return Some(err("unauthorized", "Memory Token required", "401 Unauthorized", false));
        }
        let request = serde_json::from_str::<Value>(body).ok();
        match (method, path) {
            ("GET", "/v1/speakers") => Some(ok(json!(self.speakers.lock().unwrap().speakers()))),
            ("GET", "/v1/speakers/{speakerId}") => None,
            _ => {
                if let Some(id) = bounded(path, "/v1/speakers/", ":play") {
                    if method == "POST" {
                        let url = field(request.as_ref(), "url").unwrap_or_default();
                        return Some(mutation(self.speakers.lock().unwrap().play(id, &url)));
                    }
                }
                if let Some(id) = bounded(path, "/v1/speakers/", ":stop") {
                    if method == "POST" {
                        return Some(mutation(self.speakers.lock().unwrap().stop(id)));
                    }
                }
                if let Some(id) = bounded(path, "/v1/speakers/", ":startBluetoothPairing") {
                    if method == "POST" {
                        return Some(mutation(
                            self.speakers.lock().unwrap().start_bluetooth_pairing(id),
                        ));
                    }
                }
                if let Some(id) = bounded(path, "/v1/speakers/", "/bluetooth") {
                    if method == "GET" {
                        return Some(mutation(self.speakers.lock().unwrap().bluetooth(id)));
                    }
                }
                if let Some(id) = bounded(path, "/v1/speakers/", "") {
                    let mut store = self.speakers.lock().unwrap();
                    match method {
                        "GET" => {
                            return Some(store.speaker(id).map_or_else(
                                || err("not_found", "unknown speaker", "404 Not Found", false),
                                |speaker| ok(json!(speaker)),
                            ));
                        }
                        "PATCH" => {
                            let volume = number(request.as_ref(), "volume")
                                .and_then(|value| u8::try_from(value).ok())
                                .unwrap_or(u8::MAX);
                            return Some(mutation(store.update_volume(id, volume)));
                        }
                        _ => {}
                    }
                }
                self.household_handle(method, path, request.as_ref())
                    .or_else(|| self.memory_handle(method, path, request.as_ref()))
                    .or_else(|| self.rag_handle(method, path, request.as_ref()))
                    .or_else(|| self.recording_handle(method, path, request.as_ref()))
                    .or_else(|| self.image_handle(method, path, request.as_ref()))
            }
        }
    }

    fn household_handle(
        &self,
        method: &str,
        path: &str,
        request: Option<&Value>,
    ) -> Option<Vec<u8>> {
        let mut store = self.household.lock().unwrap();
        match (method, path) {
            ("GET", "/v1/timers") => Some(ok(json!(store.timers()))),
            ("POST", "/v1/timers") => {
                let timer = Timer {
                    id: format!("t{}", now_nanos()),
                    kind: field(request, "kind").unwrap_or_else(|| "timer".into()),
                    due_at_ms: number(request, "dueAtMs").unwrap_or(0),
                    title: field(request, "title").unwrap_or_default(),
                    device_id: field(request, "deviceId"),
                    cancelled: false,
                };
                Some(mutation(store.add_timer(timer.clone()).map(|()| json!(timer))))
            }
            ("GET", "/v1/memos") => Some(ok(json!(store.memos()))),
            ("POST", "/v1/memos") => {
                let memo = Memo {
                    id: format!("m{}", now_nanos()),
                    title: field(request, "title").unwrap_or_default(),
                    device_id: field(request, "deviceId"),
                    dismissed: false,
                };
                Some(mutation(store.add_memo(memo.clone()).map(|()| json!(memo))))
            }
            _ => {
                if let Some(id) = bounded(path, "/v1/timers/", ":cancel") {
                    if method == "POST" {
                        return Some(mutation(
                            store.cancel_timer(id).map(|found| json!({"cancelled":found})),
                        ));
                    }
                }
                if let Some(id) = bounded(path, "/v1/timers/", ":snooze") {
                    if method == "POST" {
                        let delay = number(request, "delaySec").unwrap_or(60);
                        return Some(mutation(
                            store.snooze_timer(id, delay * 1000).map(|found| json!({"snoozed":found})),
                        ));
                    }
                }
                if let Some(id) = bounded(path, "/v1/memos/", ":dismiss") {
                    if method == "POST" {
                        return Some(mutation(
                            store.dismiss_memo(id).map(|found| json!({"dismissed":found})),
                        ));
                    }
                }
                if let Some(id) = bounded(path, "/v1/phones/", ":ring") {
                    if method == "POST" {
                        let duration = number(request, "durationSec").unwrap_or(5) as u32;
                        return Some(mutation(
                            store.ring_phone(id, duration).map(|record| json!({"ring":record})),
                        ));
                    }
                }
                None
            }
        }
    }

    fn memory_handle(
        &self,
        method: &str,
        path: &str,
        request: Option<&Value>,
    ) -> Option<Vec<u8>> {
        let mut store = self.memory.lock().unwrap();
        match (method, path) {
            ("GET", "/v1/audio-memory/status") => Some(ok(json!({
                "ok": true,
                "enabled": store.settings().mode == "active",
                "paused": store.settings().mode == "paused",
                "mode": store.settings().mode,
                "rawAudioRetention": "none",
                "longTermRetention": "until_forgotten",
                "counts": {
                    "observations": store.episodes().len(),
                    "episodes": store.episodes().len(),
                    "notes": store.notes().len(),
                },
            }))),
            ("GET", "/v1/audio-memory/health/ready") => {
                let mode = store.settings().mode.as_str();
                if store.ready() && mode != "paused" {
                    Some(ok(json!({
                        "ok": true,
                        "service": "audio-memory",
                        "ingestReady": true,
                        "captureReady": mode == "active",
                        "retrievalReady": true,
                        "answerReady": true,
                    })))
                } else {
                    Some(err("not_ready", "audio memory unavailable", "503 Service Unavailable", true))
                }
            }
            ("GET", "/v1/audio-memory/settings") => Some(ok(json!(store.settings()))),
            ("PATCH", "/v1/audio-memory/settings") => {
                let current = store.settings();
                let settings = Settings {
                    mode: field(request, "mode").unwrap_or_else(|| current.mode.clone()),
                    retention_days: number(request, "retention_days")
                        .unwrap_or(u64::from(current.retention_days)) as u32,
                    revision: number(request, "revision")
                        .map(|value| value as u32)
                        .unwrap_or_else(|| current.revision.saturating_add(1)),
                    capture_epoch: field(request, "captureEpoch")
                        .or_else(|| field(request, "capture_epoch"))
                        .unwrap_or_else(|| current.capture_epoch.clone()),
                };
                Some(mutation(store.set_settings(settings).map(|()| json!({"accepted":true}))))
            }
            ("GET", "/v1/audio-memory/episodes") => Some(ok(json!(store.episodes()))),
            ("GET", "/v1/audio-memory/notes") => Some(ok(json!(store.notes()))),
            ("POST", "/v1/audio-memory/notes") => {
                let id = format!("n{}", now_nanos());
                let text = field(request, "text").unwrap_or_default();
                let kind = field(request, "kind").unwrap_or_else(|| "fact".into());
                Some(mutation(
                    store.add_note(id.clone(), text, kind).map(|()| json!({"id":id})),
                ))
            }
            ("POST", "/v1/audio-memory:query") => {
                let question = field(request, "question").unwrap_or_default();
                Some(ok(json!({"notes":store.query(&question)})))
            }
            ("POST", "/v1/audio-memory:forget") => {
                let id = field(request, "id").unwrap_or_default();
                Some(mutation(store.forget(&id).map(|removed| json!({"removed":removed}))))
            }
            ("POST", "/v1/audio-memory:clear") => {
                Some(mutation(store.clear().map(|()| json!({"removed":true}))))
            }
            _ => None,
        }
    }

    fn rag_handle(&self, method: &str, path: &str, request: Option<&Value>) -> Option<Vec<u8>> {
        let mut store = self.rag.lock().unwrap();
        if (method, path) == ("POST", "/v1/knowledge-bases") {
            let name = field(request, "name").unwrap_or_else(|| "Knowledge base".into());
            return Some(mutation(store.create(&name)));
        }
        if let Some(id) = bounded(path, "/v1/knowledge-bases/", "") {
            let (base_id, action) = if let Some(base) = id.strip_suffix(":query") {
                (base, ":query")
            } else {
                let mut parts = id.splitn(2, '/');
                (parts.next().unwrap_or(""), parts.next().unwrap_or(""))
            };
            match (method, action) {
                ("GET", "") => {
                    return Some(store.get(base_id).map_or_else(
                        || err("not_found", "unknown knowledge base", "404 Not Found", false),
                        |base| ok(json!(base)),
                    ));
                }
                ("DELETE", "") => {
                    return Some(mutation(store.delete(base_id).map(|found| json!({"deleted":found}))));
                }
                ("GET", "documents") => {
                    return Some(mutation(store.documents(base_id)));
                }
                ("POST", "documents:text") => {
                    return Some(mutation(store.add_text(
                        base_id,
                        &field(request, "filename").unwrap_or_else(|| "document.txt".into()),
                        &field(request, "text").unwrap_or_default(),
                    )));
                }
                ("POST", "documents:file") => {
                    let bytes = field(request, "content").unwrap_or_default().into_bytes();
                    return Some(mutation(store.add_file(
                        base_id,
                        &field(request, "filename").unwrap_or_else(|| "file.bin".into()),
                        &field(request, "mime").unwrap_or_else(|| "application/octet-stream".into()),
                        &bytes,
                    )));
                }
                ("POST", "documents:image") => {
                    return Some(mutation(store.add_image(
                        base_id,
                        &field(request, "filename").unwrap_or_else(|| "image.png".into()),
                        &field(request, "value").unwrap_or_default(),
                    )));
                }
                ("POST", ":query") => {
                    let question = field(request, "query").or_else(|| field(request, "question"))
                        .unwrap_or_default();
                    let top_k = number(request, "topK").unwrap_or(5) as usize;
                    return Some(mutation(store.query(base_id, &question, top_k)));
                }
                _ => {}
            }
        }
        None
    }

    fn recording_handle(&self, method: &str, path: &str, request: Option<&Value>) -> Option<Vec<u8>> {
        let mut store = self.recordings.lock().unwrap();
        match (method, path) {
            ("GET", "/v1/audio/recordings") => Some(ok(json!(store.list()))),
            ("POST", "/v1/audio/recordings") => {
                let retain = request.and_then(|v| v.get("retainAudio")).and_then(Value::as_bool)
                    .unwrap_or(false);
                Some(mutation(store.create(retain)))
            }
            _ => {
                if let Some(id) = bounded(path, "/v1/audio/recordings/", ":finish") {
                    if method == "POST" {
                        let root = store.root().to_path_buf();
                        return Some(match external_asr(&root, id) {
                            Ok(segments) => mutation(store.finish_with(id, segments)),
                            Err(message) if message == "asr_unavailable" => mutation(store.finish(id)),
                            Err(message) => err("asr_failed", &message, "422 Unprocessable Entity", true),
                        });
                    }
                }
                if let Some(id) = bounded(path, "/v1/audio/recordings/", ":reprocess") {
                    if method == "POST" {
                        let root = store.root().to_path_buf();
                        return Some(match external_asr(&root, id) {
                            Ok(segments) => mutation(store.reprocess_with(id, segments)),
                            Err(message) if message == "asr_unavailable" => mutation(store.reprocess(id)),
                            Err(message) => err("asr_failed", &message, "422 Unprocessable Entity", true),
                        });
                    }
                }
                if let Some(id) = bounded(path, "/v1/audio/recordings/", "/audio") {
                    if method == "POST" {
                        let seq = number(request, "seq").unwrap_or(0) as u32;
                        let pcm = field(request, "pcm").unwrap_or_default();
                        return Some(mutation(
                            store.append_audio(id, seq, pcm.as_bytes()).map(|()| json!({"accepted":true})),
                        ));
                    }
                }
                if let Some(id) = bounded(path, "/v1/audio/recordings/", "") {
                    match method {
                        "GET" => {
                            return Some(store.get(id).map_or_else(
                                || err("not_found", "unknown recording", "404 Not Found", false),
                                |recording| ok(json!(recording)),
                            ));
                        }
                        "DELETE" => {
                            return Some(mutation(store.delete(id).map(|found| json!({"deleted":found}))));
                        }
                        _ => {}
                    }
                }
                None
            }
        }
    }

    fn image_handle(&self, method: &str, path: &str, request: Option<&Value>) -> Option<Vec<u8>> {
        let mut store = self.images.lock().unwrap();
        if (method, path) == ("POST", "/v2/images/frames:process") {
            return Some(err(
                "model_missing",
                "image frame worker is not provisioned",
                "503 Service Unavailable",
                true,
            ));
        }
        if (method, path) == ("POST", "/v2/images/jobs") {
            let operation = field(request, "operation").unwrap_or_else(|| "generate".into());
            let payload = request.unwrap_or(&Value::Null).to_string();
            return Some(match store.create(&operation, &payload) {
                Ok(job) => reply("202 Accepted", &json!(job).to_string()),
                Err(message) => err("invalid_request", &message, "400 Bad Request", false),
            });
        }
        if let Some(id) = bounded(path, "/v2/images/jobs/", ":cancel") {
            if method == "POST" {
                return Some(match store.cancel(id) {
                    Ok(job) => reply("202 Accepted", &json!({"job":job}).to_string()),
                    Err(message) => err("invalid_request", &message, "400 Bad Request", false),
                });
            }
        }
        if let Some(id) = bounded(path, "/v2/images/jobs/", "/result") {
            if method == "GET" {
                return Some(match store.result(id) {
                    Ok((bytes, content_type)) => {
                        let mut reply = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                            bytes.len()
                        ).into_bytes();
                        reply.extend_from_slice(&bytes);
                        reply
                    }
                    Err(message) => err("result_unavailable", &message, "409 Conflict", false),
                });
            }
        }
        if let Some(id) = bounded(path, "/v2/images/jobs/", "") {
            if method == "GET" {
                return Some(store.get(id).map_or_else(
                    || err("not_found", "unknown image job", "404 Not Found", false),
                    |job| ok(json!(job)),
                ));
            }
        }
        None
    }
}

fn bounded<'a>(path: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let rest = path.strip_prefix(prefix)?;
    if suffix.is_empty() {
        return (!rest.is_empty()).then_some(rest);
    }
    rest.strip_suffix(suffix).filter(|id| !id.is_empty())
}

fn field(request: Option<&Value>, key: &str) -> Option<String> {
    request?.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn number(request: Option<&Value>, key: &str) -> Option<u64> {
    request?.get(key).and_then(Value::as_u64)
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn external_asr(root: &Path, id: &str) -> Result<Vec<String>, String> {
    let command = std::env::var("OLGP_ASR_CMD").map_err(|_| "asr_unavailable".to_owned())?;
    external_asr_with(&command, root, id)
}

fn external_asr_with(command: &str, root: &Path, id: &str) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{command} \"$1\" \"$2\""))
        .arg("olgp-asr")
        .arg(root)
        .arg(id)
        .output()
        .map_err(|e| format!("asr_spawn: {e}"))?;
    if !output.status.success() {
        return Err("asr_failed".to_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => match map.get("segments") {
            Some(Value::Array(items)) => Ok(items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()),
            _ => Err("asr_bad_output".to_owned()),
        },
        Ok(Value::Array(items)) => Ok(items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()),
        _ if text.is_empty() => Err("asr_empty".to_owned()),
        _ => Ok(vec![text]),
    }
}


fn memory_authorized(head: &str) -> bool {
    head.lines().any(|line| {
        let lower = line.to_ascii_lowercase();
        lower.starts_with("x-olgp-memory-token:") || lower.starts_with("x-wici-memory-token:")
    })
}

fn ok(body: Value) -> Vec<u8> {
    reply("200 OK", &body.to_string())
}

fn mutation<T: serde::Serialize>(result: Result<T, String>) -> Vec<u8> {
    match result {
        Ok(value) => ok(json!({"accepted":true,"result":value})),
        Err(message) => err("invalid_request", &message, "400 Bad Request", false),
    }
}

fn err(code: &str, message: &str, status: &str, retryable: bool) -> Vec<u8> {
    reply(status, &json!({"error":{"code":code,"message":message,"retryable":retryable}}).to_string())
}

fn reply(status: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_extracts_ids() {
        assert_eq!(bounded("/v1/timers/abc:cancel", "/v1/timers/", ":cancel"), Some("abc"));
        assert_eq!(bounded("/v1/timers/", "/v1/timers/", ":cancel"), None);
    }

    #[test]
    fn memory_header_detection_is_case_insensitive() {
        assert!(memory_authorized("X-Wici-Memory-Token: abc"));
        assert!(memory_authorized("x-olgp-memory-token: abc"));
        assert!(!memory_authorized("Authorization: Bearer abc"));
    }

    fn body_of(reply: &[u8]) -> String {
        let text = String::from_utf8_lossy(reply);
        text.split("\r\n\r\n").nth(1).unwrap_or("").to_owned()
    }

    fn id_of(reply: &[u8]) -> String {
        let value: Value = serde_json::from_str(&body_of(reply)).unwrap();
        value["id"]
            .as_str()
            .or_else(|| value["result"]["id"].as_str())
            .unwrap()
            .to_owned()
    }

    fn status_of(reply: &[u8]) -> &str {
        let text = std::str::from_utf8(reply).unwrap();
        text.split_whitespace().nth(1).unwrap_or("")
    }

    #[test]
    fn product_routes_are_wired() {
        let root = std::env::temp_dir().join(format!("olgp-product-{}-routes", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let state = ProductState::load_from_paths(&root).unwrap();
        let auth = "Authorization: Bearer token";
        let memory_auth = "Authorization: Bearer token\r\nX-Wici-Memory-Token: token";
        let speaker = "wici-speaker-001";

        assert_eq!(status_of(&state.handle("GET", "/v1/speakers", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", &format!("/v1/speakers/{speaker}"), auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("PATCH", &format!("/v1/speakers/{speaker}"), auth, "{\"volume\":35}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/speakers/{speaker}:play"), auth, "{\"url\":\"https://example.test/a.wav\"}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/speakers/{speaker}:stop"), auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", &format!("/v1/speakers/{speaker}/bluetooth"), auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/speakers/{speaker}:startBluetoothPairing"), auth, "").unwrap()), "200");

        let timer = id_of(&state.handle("POST", "/v1/timers", auth, "{\"title\":\"Tea\",\"dueAtMs\":123,\"deviceId\":\"dev\"}").unwrap());
        assert_eq!(status_of(&state.handle("GET", "/v1/timers", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/timers/{timer}:snooze"), auth, "{\"delaySec\":60}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/timers/{timer}:cancel"), auth, "").unwrap()), "200");
        let memo = id_of(&state.handle("POST", "/v1/memos", auth, "{\"title\":\"note\"}").unwrap());
        assert_eq!(status_of(&state.handle("GET", "/v1/memos", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/memos/{memo}:dismiss"), auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/phones/phone-1:ring", auth, "{\"durationSec\":5}").unwrap()), "200");

        assert_eq!(status_of(&state.handle("GET", "/v1/audio-memory/status", auth, "").unwrap()), "401");
        assert_eq!(status_of(&state.handle("GET", "/v1/audio-memory/status", memory_auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", "/v1/audio-memory/health/ready", memory_auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", "/v1/audio-memory/settings", memory_auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("PATCH", "/v1/audio-memory/settings", memory_auth, "{\"mode\":\"active\",\"retention_days\":7}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", "/v1/audio-memory/episodes", memory_auth, "").unwrap()), "200");
        let note = id_of(&state.handle("POST", "/v1/audio-memory/notes", memory_auth, "{\"text\":\"remember\"}").unwrap());
        assert_eq!(status_of(&state.handle("GET", "/v1/audio-memory/notes", memory_auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/audio-memory:query", memory_auth, "{\"question\":\"remember\"}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/audio-memory:forget", memory_auth, &format!("{{\"id\":\"{note}\"}}")).unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/audio-memory:clear", memory_auth, "").unwrap()), "200");

        let kb = id_of(&state.handle("POST", "/v1/knowledge-bases", auth, "{\"name\":\"Docs\"}").unwrap());
        assert_eq!(status_of(&state.handle("GET", &format!("/v1/knowledge-bases/{kb}"), auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/knowledge-bases/{kb}/documents:text"), auth, "{\"filename\":\"a.txt\",\"text\":\"alpha beta\"}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/knowledge-bases/{kb}/documents:file"), auth, "{\"filename\":\"a.bin\",\"mime\":\"text/plain\",\"content\":\"gamma\"}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/knowledge-bases/{kb}/documents:image"), auth, "{\"filename\":\"a.png\",\"value\":\"data:image/png;base64,AA\"}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", &format!("/v1/knowledge-bases/{kb}/documents"), auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/knowledge-bases/{kb}:query"), auth, "{\"query\":\"alpha\"}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("DELETE", &format!("/v1/knowledge-bases/{kb}"), auth, "").unwrap()), "200");

        let recording = id_of(&state.handle("POST", "/v1/audio/recordings", auth, "{\"retainAudio\":true}").unwrap());
        assert_eq!(status_of(&state.handle("GET", "/v1/audio/recordings", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/audio/recordings/{recording}/audio"), auth, "{\"seq\":0,\"pcm\":\"AA\"}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/audio/recordings/{recording}:finish"), auth, "").unwrap()), "400");
        assert_eq!(status_of(&state.handle("POST", &format!("/v1/audio/recordings/{recording}:reprocess"), auth, "").unwrap()), "400");
        assert_eq!(status_of(&state.handle("GET", &format!("/v1/audio/recordings/{recording}"), auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("DELETE", &format!("/v1/audio/recordings/{recording}"), auth, "").unwrap()), "200");

        assert_eq!(status_of(&state.handle("POST", "/v2/images/frames:process", auth, "{}").unwrap()), "503");
        let job = id_of(&state.handle("POST", "/v2/images/jobs", auth, "{\"operation\":\"generate\"}").unwrap());
        assert_eq!(status_of(&state.handle("GET", &format!("/v2/images/jobs/{job}"), auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", &format!("/v2/images/jobs/{job}/result"), auth, "").unwrap()), "409");
        assert_eq!(status_of(&state.handle("POST", &format!("/v2/images/jobs/{job}:cancel"), auth, "").unwrap()), "202");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn load_uses_environment_paths() {
        let root = std::env::temp_dir().join(format!("olgp-product-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        unsafe {
            std::env::set_var("OLGP_HOUSEHOLD_PATH", root.join("household.json"));
            std::env::set_var("OLGP_MEMORY_PATH", root.join("memory.json"));
            std::env::set_var("OLGP_SPEAKER_PATH", root.join("speakers.json"));
            std::env::set_var("OLGP_RAG_PATH", root.join("rag.json"));
            std::env::set_var("OLGP_IMAGES_PATH", root.join("images.json"));
            std::env::set_var("OLGP_RECORDINGS_ROOT", root.join("recordings"));
        }
        let state = ProductState::load();
        unsafe {
            std::env::remove_var("OLGP_HOUSEHOLD_PATH");
            std::env::remove_var("OLGP_MEMORY_PATH");
            std::env::remove_var("OLGP_SPEAKER_PATH");
            std::env::remove_var("OLGP_RAG_PATH");
            std::env::remove_var("OLGP_IMAGES_PATH");
            std::env::remove_var("OLGP_RECORDINGS_ROOT");
        }
        assert!(state.is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn product_error_paths_are_stable() {
        let root = std::env::temp_dir().join(format!("olgp-product-errors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let state = ProductState::load_from_paths(&root).unwrap();
        let auth = "Authorization: Bearer token";
        let memory_auth = "Authorization: Bearer token\r\nX-Wici-Memory-Token: token";

        assert_eq!(status_of(&state.handle("GET", "/v1/speakers/missing", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("PATCH", "/v1/speakers/missing", auth, "{\"volume\":35}").unwrap()), "400");
        assert_eq!(status_of(&state.handle("POST", "/v1/speakers/missing:play", auth, "{\"url\":\"u\"}").unwrap()), "400");
        assert_eq!(status_of(&state.handle("POST", "/v1/speakers/missing:stop", auth, "").unwrap()), "400");
        assert_eq!(status_of(&state.handle("GET", "/v1/speakers/missing/bluetooth", auth, "").unwrap()), "400");
        assert_eq!(status_of(&state.handle("POST", "/v1/speakers/missing:startBluetoothPairing", auth, "").unwrap()), "400");
        assert_eq!(status_of(&state.handle("PATCH", "/v1/speakers/wici-speaker-001", auth, "{\"volume\":101}").unwrap()), "400");

        assert_eq!(status_of(&state.handle("POST", "/v1/timers/missing:snooze", auth, "{\"delaySec\":1}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/timers/missing:cancel", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/memos/missing:dismiss", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/phones/phone-1:ring", auth, "{\"durationSec\":61}").unwrap()), "400");

        assert_eq!(status_of(&state.handle("PATCH", "/v1/audio-memory/settings", memory_auth, "{}").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/audio-memory:query", memory_auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("POST", "/v1/audio-memory:forget", memory_auth, "").unwrap()), "200");

        assert_eq!(status_of(&state.handle("POST", "/v1/knowledge-bases", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", "/v1/knowledge-bases/missing", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("DELETE", "/v1/knowledge-bases/missing", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", "/v1/knowledge-bases/missing/documents", auth, "").unwrap()), "400");
        assert_eq!(status_of(&state.handle("POST", "/v1/knowledge-bases/missing:query", auth, "{}").unwrap()), "400");

        assert_eq!(status_of(&state.handle("GET", "/v1/audio/recordings/missing", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("DELETE", "/v1/audio/recordings/missing", auth, "").unwrap()), "200");
        assert_eq!(status_of(&state.handle("GET", "/v2/images/jobs/missing", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("GET", "/v2/images/jobs/missing/result", auth, "").unwrap()), "409");
        assert_eq!(status_of(&state.handle("POST", "/v2/images/jobs", auth, "{\"operation\":\"bogus\"}").unwrap()), "400");

        assert!(state.handle("DELETE", "/v1/speakers/wici-speaker-001", auth, "").is_none());
        assert_eq!(status_of(&state.handle("GET", "/v1/speakers/wici-speaker-001:play", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("GET", "/v1/speakers/wici-speaker-001:stop", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("GET", "/v1/speakers/wici-speaker-001:startBluetoothPairing", auth, "").unwrap()), "404");
        assert!(state.handle("POST", "/v1/speakers/wici-speaker-001/bluetooth", auth, "").is_none());
        assert!(state.handle("GET", "/v1/timers/missing:snooze", auth, "").is_none());
        assert!(state.handle("GET", "/v1/timers/missing:cancel", auth, "").is_none());
        assert!(state.handle("GET", "/v1/memos/missing:dismiss", auth, "").is_none());
        assert!(state.handle("GET", "/v1/phones/phone-1:ring", auth, "").is_none());
        assert_eq!(status_of(&state.handle("GET", "/v1/knowledge-bases/x:unknown", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("GET", "/v1/audio/recordings/x:finish", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("GET", "/v1/audio/recordings/x:reprocess", auth, "").unwrap()), "404");
        assert_eq!(status_of(&state.handle("GET", "/v1/audio/recordings/x/audio", auth, "").unwrap()), "404");

        assert!(state.handle("GET", "/v1/unknown", auth, "").is_none());
        assert!(field(None, "x").is_none());
        assert!(number(None, "x").is_none());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_asr_parses_transcript_segments() {
        let root = Path::new("/tmp");
        let segments = external_asr_with(
            r#"printf '{"segments":["hello","world"]}'"#,
            root,
            "rec-1",
        )
        .unwrap();
        assert_eq!(segments, vec!["hello", "world"]);
        assert_eq!(
            external_asr_with(r#"printf '["one","two"]'"#, root, "rec-1").unwrap(),
            vec!["one", "two"]
        );
        assert_eq!(
            external_asr_with("printf 'plain text'", root, "rec-1").unwrap(),
            vec!["plain text"]
        );
        assert!(external_asr_with("printf ''", root, "rec-1").is_err());
        assert!(external_asr_with("printf '{}'", root, "rec-1").is_err());
        assert!(external_asr_with("false", root, "rec-1").is_err());
    }
}
