# v1 route contract

Twenty-eight routes are defined in v1. `scripts/check-contract.sh` verifies that
this list matches `crates/olgp-control/src/api.rs` and `proto/olgp/v1/*.proto`.

| Method | Path | Auth |
|---|---|---|
| GET | `/v1/health/live` | none |
| GET | `/v1/capabilities` | none |
| GET | `/v1/pairing/status` | none |
| POST | `/v1/pairing` | pairing window |
| POST | `/v1/pairing:open` | paired writer |
| POST | `/v1/pairing:close` | paired writer |
| GET | `/v1/models` | Device Token |
| POST | `/v1/responses` | Device Token |
| POST | `/v1/chat/completions` | Device Token |
| POST | `/v1/embeddings` | Device Token |
| POST | `/v1/audio/transcriptions` | Device Token |
| POST | `/v1/audio/speech` | Device Token |
| WS | `/v1/realtime?mode=voice\|chat` | Device Token |
| POST | `/api/chat` | Device Token |
| POST | `/api/generate` | Device Token |
| GET | `/v1/timers` | Device Token |
| POST | `/v1/timers` | Device Token |
| GET | `/v1/memos` | Device Token |
| POST | `/v1/memos` | Device Token |
| GET | `/v1/audio-memory/status` | Device + Memory Token |
| GET | `/v1/audio-memory/settings` | Device + Memory Token |
| PATCH | `/v1/audio-memory/settings` | Device + Memory Token |
| GET | `/v1/audio-memory/episodes` | Device + Memory Token |
| GET | `/v1/audio-memory/notes` | Device + Memory Token |
| POST | `/v1/audio-memory/notes` | Device + Memory Token |
| POST | `/v1/audio-memory:query` | Device + Memory Token |
| POST | `/v1/audio-memory:forget` | Device + Memory Token |
| POST | `/v1/audio-memory:clear` | Device + Memory Token |

Voice events: `ready`, `text_input`, `commit`, `cancel`, `reset`,
`input_committed`, `stt_partial`, `stt_final`, `text_tentative`, `text`, `end`,
`error`. Binary frames are `0x01 + PCM S16LE`.
