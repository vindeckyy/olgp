# v1/v2 route contract

Sixty public routes are defined in v1 and the preview v2 image-job namespace.
`scripts/check-contract.sh` verifies that this list matches
`crates/olgp-control/src/api.rs`.

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
| GET | `/v1/speakers` | Device Token |
| GET | `/v1/speakers/{speakerId}` | Device Token |
| PATCH | `/v1/speakers/{speakerId}` | Device Token |
| POST | `/v1/speakers/{speakerId}:play` | Device Token |
| POST | `/v1/speakers/{speakerId}:stop` | Device Token |
| GET | `/v1/speakers/{speakerId}/bluetooth` | Device Token |
| POST | `/v1/speakers/{speakerId}:startBluetoothPairing` | Device Token |
| WS | `/v1/speakers/{speakerId}/audio:stream` | Device Token |
| GET | `/v1/timers` | Device Token |
| POST | `/v1/timers` | Device Token |
| POST | `/v1/timers/{timerId}:cancel` | Device Token |
| POST | `/v1/timers/{timerId}:snooze` | Device Token |
| GET | `/v1/memos` | Device Token |
| POST | `/v1/memos` | Device Token |
| POST | `/v1/memos/{memoId}:dismiss` | Device Token |
| POST | `/v1/phones/{phoneId}:ring` | Device Token |
| GET | `/v1/audio-memory/status` | Device + Memory Token |
| GET | `/v1/audio-memory/health/ready` | Device + Memory Token |
| GET | `/v1/audio-memory/settings` | Device + Memory Token |
| PATCH | `/v1/audio-memory/settings` | Device + Memory Token |
| GET | `/v1/audio-memory/episodes` | Device + Memory Token |
| GET | `/v1/audio-memory/notes` | Device + Memory Token |
| POST | `/v1/audio-memory/notes` | Device + Memory Token |
| POST | `/v1/audio-memory:query` | Device + Memory Token |
| POST | `/v1/audio-memory:forget` | Device + Memory Token |
| POST | `/v1/audio-memory:clear` | Device + Memory Token |
| POST | `/v1/knowledge-bases` | Device Token |
| GET | `/v1/knowledge-bases/{knowledgeBaseId}` | Device Token |
| DELETE | `/v1/knowledge-bases/{knowledgeBaseId}` | Device Token |
| GET | `/v1/knowledge-bases/{knowledgeBaseId}/documents` | Device Token |
| POST | `/v1/knowledge-bases/{knowledgeBaseId}/documents:text` | Device Token |
| POST | `/v1/knowledge-bases/{knowledgeBaseId}/documents:file` | Device Token |
| POST | `/v1/knowledge-bases/{knowledgeBaseId}/documents:image` | Device Token |
| POST | `/v1/knowledge-bases/{knowledgeBaseId}:query` | Device Token |
| GET | `/v1/audio/recordings` | Device Token |
| POST | `/v1/audio/recordings` | Device Token |
| GET | `/v1/audio/recordings/{recordingId}` | Device Token |
| DELETE | `/v1/audio/recordings/{recordingId}` | Device Token |
| POST | `/v1/audio/recordings/{recordingId}/audio` | Device Token |
| POST | `/v1/audio/recordings/{recordingId}:finish` | Device Token |
| POST | `/v1/audio/recordings/{recordingId}:reprocess` | Device Token |
| POST | `/v2/images/jobs` | Device Token |
| GET | `/v2/images/jobs/{jobId}` | Device Token |
| GET | `/v2/images/jobs/{jobId}/result` | Device Token |
| POST | `/v2/images/jobs/{jobId}:cancel` | Device Token |

Voice events: `ready`, `text_input`, `commit`, `cancel`, `reset`,
`input_committed`, `stt_partial`, `stt_final`, `text_tentative`, `text`, `end`,
`error`. Binary frames are `0x01 + PCM S16LE`; the `ready.audio` object carries
format, sample rate, and channel count.

Status: core inference, pairing, speaker, timers, memos, and phone routes are
stable. Audio-memory, private RAG, recordings, and image jobs are implemented
previews; schemas may change but routes are present and authenticated.
