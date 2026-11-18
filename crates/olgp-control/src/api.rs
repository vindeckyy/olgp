//! The complete v1 API route registry.
//!
//! `scripts/check-contract.sh` reads this file, so keep the literals exact.

/// One API route: method and path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Route {
    pub method: &'static str,
    pub path: &'static str,
}

/// All 19 v1 routes.
pub const ROUTES: [Route; 19] = [
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
    fn route_count_is_nineteen() {
        assert_eq!(routes().len(), 19);
    }

    #[test]
    fn health_is_public() {
        assert!(routes().iter().any(|r| r.method == "GET" && r.path == "/v1/health/live"));
    }
}
