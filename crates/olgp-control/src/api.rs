//! The complete v1/v2 product and protocol route registry.
//!
//! `scripts/check-contract.sh` reads this file, so keep the literals exact.

/// One API route: method and path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
}

/// All 60 public routes.
pub const ROUTES: [Route; 60] = [
    Route { method: "GET", path: "/v1/health/live" },
    Route { method: "GET", path: "/v1/capabilities" },
    Route { method: "GET", path: "/v1/pairing/status" },
    Route { method: "POST", path: "/v1/pairing" },
    Route { method: "POST", path: "/v1/pairing:open" },
    Route { method: "POST", path: "/v1/pairing:close" },
    Route { method: "GET", path: "/v1/models" },
    Route { method: "POST", path: "/v1/responses" },
    Route { method: "POST", path: "/v1/chat/completions" },
    Route { method: "POST", path: "/v1/embeddings" },
    Route { method: "POST", path: "/v1/audio/transcriptions" },
    Route { method: "POST", path: "/v1/audio/speech" },
    Route { method: "WS", path: "/v1/realtime" },
    Route { method: "POST", path: "/api/chat" },
    Route { method: "POST", path: "/api/generate" },
    Route { method: "GET", path: "/v1/speakers" },
    Route { method: "GET", path: "/v1/speakers/{speakerId}" },
    Route { method: "PATCH", path: "/v1/speakers/{speakerId}" },
    Route { method: "POST", path: "/v1/speakers/{speakerId}:play" },
    Route { method: "POST", path: "/v1/speakers/{speakerId}:stop" },
    Route { method: "GET", path: "/v1/speakers/{speakerId}/bluetooth" },
    Route { method: "POST", path: "/v1/speakers/{speakerId}:startBluetoothPairing" },
    Route { method: "WS", path: "/v1/speakers/{speakerId}/audio:stream" },
    Route { method: "GET", path: "/v1/timers" },
    Route { method: "POST", path: "/v1/timers" },
    Route { method: "POST", path: "/v1/timers/{timerId}:cancel" },
    Route { method: "POST", path: "/v1/timers/{timerId}:snooze" },
    Route { method: "GET", path: "/v1/memos" },
    Route { method: "POST", path: "/v1/memos" },
    Route { method: "POST", path: "/v1/memos/{memoId}:dismiss" },
    Route { method: "POST", path: "/v1/phones/{phoneId}:ring" },
    Route { method: "GET", path: "/v1/audio-memory/status" },
    Route { method: "GET", path: "/v1/audio-memory/health/ready" },
    Route { method: "GET", path: "/v1/audio-memory/settings" },
    Route { method: "PATCH", path: "/v1/audio-memory/settings" },
    Route { method: "GET", path: "/v1/audio-memory/episodes" },
    Route { method: "GET", path: "/v1/audio-memory/notes" },
    Route { method: "POST", path: "/v1/audio-memory/notes" },
    Route { method: "POST", path: "/v1/audio-memory:query" },
    Route { method: "POST", path: "/v1/audio-memory:forget" },
    Route { method: "POST", path: "/v1/audio-memory:clear" },
    Route { method: "POST", path: "/v1/knowledge-bases" },
    Route { method: "GET", path: "/v1/knowledge-bases/{knowledgeBaseId}" },
    Route { method: "DELETE", path: "/v1/knowledge-bases/{knowledgeBaseId}" },
    Route { method: "GET", path: "/v1/knowledge-bases/{knowledgeBaseId}/documents" },
    Route { method: "POST", path: "/v1/knowledge-bases/{knowledgeBaseId}/documents:text" },
    Route { method: "POST", path: "/v1/knowledge-bases/{knowledgeBaseId}/documents:file" },
    Route { method: "POST", path: "/v1/knowledge-bases/{knowledgeBaseId}/documents:image" },
    Route { method: "POST", path: "/v1/knowledge-bases/{knowledgeBaseId}:query" },
    Route { method: "GET", path: "/v1/audio/recordings" },
    Route { method: "POST", path: "/v1/audio/recordings" },
    Route { method: "GET", path: "/v1/audio/recordings/{recordingId}" },
    Route { method: "DELETE", path: "/v1/audio/recordings/{recordingId}" },
    Route { method: "POST", path: "/v1/audio/recordings/{recordingId}/audio" },
    Route { method: "POST", path: "/v1/audio/recordings/{recordingId}:finish" },
    Route { method: "POST", path: "/v1/audio/recordings/{recordingId}:reprocess" },
    Route { method: "POST", path: "/v2/images/jobs" },
    Route { method: "GET", path: "/v2/images/jobs/{jobId}" },
    Route { method: "GET", path: "/v2/images/jobs/{jobId}/result" },
    Route { method: "POST", path: "/v2/images/jobs/{jobId}:cancel" },
];

/// Return the route registry.
#[must_use]
pub fn routes() -> &'static [Route] {
    &ROUTES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_count_is_sixty() {
        assert_eq!(routes().len(), 60);
    }

    #[test]
    fn health_is_public() {
        assert!(routes().iter().any(|r| r.method == "GET" && r.path == "/v1/health/live"));
    }
}
