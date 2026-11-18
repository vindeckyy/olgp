//! The complete v1 API route registry.
//!
//! `scripts/check-contract.sh` reads this file, so keep the literals exact.

/// One API route: method and path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
}

/// All 28 v1 routes.
pub const ROUTES: [Route; 28] = [
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
    Route { method: "GET", path: "/v1/timers" },
    Route { method: "POST", path: "/v1/timers" },
    Route { method: "GET", path: "/v1/memos" },
    Route { method: "POST", path: "/v1/memos" },
    Route { method: "GET", path: "/v1/audio-memory/status" },
    Route { method: "GET", path: "/v1/audio-memory/settings" },
    Route { method: "PATCH", path: "/v1/audio-memory/settings" },
    Route { method: "GET", path: "/v1/audio-memory/episodes" },
    Route { method: "GET", path: "/v1/audio-memory/notes" },
    Route { method: "POST", path: "/v1/audio-memory/notes" },
    Route { method: "POST", path: "/v1/audio-memory:query" },
    Route { method: "POST", path: "/v1/audio-memory:forget" },
    Route { method: "POST", path: "/v1/audio-memory:clear" },
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
    fn route_count_is_twenty_eight() {
        assert_eq!(routes().len(), 28);
    }

    #[test]
    fn health_is_public() {
        assert!(routes().iter().any(|r| r.method == "GET" && r.path == "/v1/health/live"));
    }
}
