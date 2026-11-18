# v1 route contract

Nineteen routes are defined in v1. `scripts/check-contract.sh` verifies that
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

Voice events: `ready`, `text_input`, `commit`, `cancel`, `reset`,
`input_committed`, `stt_partial`, `stt_final`, `text_tentative`, `text`, `end`,
`error`. Binary frames are `0x01 + PCM S16LE`.
