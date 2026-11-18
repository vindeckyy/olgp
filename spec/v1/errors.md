# Error codes

| Code | HTTP | retryable | Meaning |
|---|---|---|---|
| `invalid_request` | 400 | false | Malformed request |
| `unsupported_model` | 400 | false | Model alias is not in the allowlist |
| `insufficient_memory` | 503 | false | KV and working set cannot be admitted |
| `not_ready` | 503 | true | Engine is warming or busy |
| `unauthorized` | 401 | false | Missing, revoked, or invalid token |
| `pairing_denied` | 403 | false | Wrong code or untrusted client |
| `lease_busy` | 409 | true | Requested lease class is occupied |
| `handle_busy` | 409 | true | Handle has a nonzero refcount |
| `trace_mismatch` | 409 | false | Replayed trace did not validate |
| `upstream_unavailable` | 502 | true | Loopback engine or LUPINE is down |
| `internal_error` | 500 | false | Unexpected server failure |

`retryable` is authoritative. OpenAI-compatible routes preserve `message`,
`type`, `param`, and `code` in addition to the OLGP fields.
