# WiCi public-surface parity

This table is a clean-room audit of public WiCi pages as of 2026-11-17. It does
not claim wire compatibility. "Implemented" means OLGP has code and tests for
the observable behavior; "partial" means a local or deterministic subset;
"structural" means a product/hardware feature that is intentionally outside a
protocol repository.

Sources: `https://wici.ai/wici-one`, `https://wici.ai/technology`,
`https://developer.wici.ai/`, and the linked Image v2 contract generator.

| Area | Public claim | OLGP state | Status |
|---|---|---|---|
| API routes | 64 public v1/v2/engine routes | All 64 in `spec/v1/contracts.md`, registry, gateway, and product modules | implemented |
| Auth | Bearer Device Token; Memory Token for audio memory | Trusted-client store validates tokens; memory header checked | implemented |
| Pairing | 15-minute first window, 5-minute invitations, scoped token | Noise/SPAKE2 pairing, displayed code, revocable token store | implemented |
| Speaker | State, volume, play/stop, Bluetooth, PCM WS | Persisted speaker state and PCM WS upgrade | implemented |
| Timers/memos/phone | Create/list/cancel/snooze/dismiss/ring | Persisted JSON store with bounds checks | implemented |
| Audio Memory | Opt-in status, settings, notes, query, forget, clear | Persisted store, documented payloads, preview boundary | implemented |
| Private RAG | Knowledge bases, text/file/image ingestion, cited query | Local keyword retrieval with citations and deletion | partial |
| Recordings | Resumable PCM upload and transcript lifecycle | Durable chunks; finish reports `asr_unavailable` without a fake transcript | partial |
| Image jobs | Async generate/edit/face_swap/reframe/detect | Jobs persist honest `model_missing`; no fabricated media | partial |
| Image frames | One-shot detect frames | Route returns `model_missing`; SDK helpers map the contract | partial |
| Semantic transport | Batching, trace replay, content cache | C++ dispatch with batch wire codec, trace sessions, and cache counters | partial |
| Scheduler | VRAM/RAM residency, parked processes | Lease scheduler plus shared-weight registry in metrics | partial |
| Voice | voice-v1 events, PCM, early answers, interruption | WS events, deterministic gate, configurable ASR config, barge-in | partial |
| Recordings ASR | Transcript lifecycle with external ASR | `OLGP_ASR_CMD` hook stores real segments; honest `asr_unavailable` otherwise | partial |
| GPU recognition | CUDA/NVML shims on Linux | LUPINE-derived shims with SPKI pin and system-wide preload | partial |
| Custom DRM | `/dev/dri` from a virtual PCI/platform device | GPL module with ioctls; static-checked only | partial |
| Venus | Real Vulkan device in a guest | QEMU command generation and config validation | partial |
| SDKs | TypeScript and Python documented functions | Four HTTP SDKs plus Rust core; product and engine helpers | implemented |
| CLI probes | `wici-smi --workload`, `wici runtime check` | `olgp-smi --workload` and `olgpctl runtime check` | implemented |
| Image results | Verified bytes, `result.save()`, content type | `result_bytes` / `ResultBytesAsync` / typed bytes across SDKs | implemented |
| Search headers | Caller-owned Tavily provider/key headers | All SDKs support optional `X-Wici-Search-*` headers | implemented |
| Audio capture | `audio.recordings` capture/resume helpers | All SDKs expose start/append/resume aliases over the 7 recording routes | implemented |
| Realtime SDKs | `realtime.connect(mode)` with control frames and PCM | All five SDKs expose session clients; gateway accepts query-token upgrades | implemented |
| Hardware SKU | RTX 5060 Ti/5090, Wi-Fi 7, NVMe, enclosure, PSU, cooling | Not reproducible in a protocol repo | structural |
| Driver platforms | macOS, Windows, Vulkan ICD, D3D, games/CAD/video apps | Not implemented; no misleading stub ICD | structural/gap |
| Benchmarks | Product tok/s, FPS, export, encode, latency tables | No equivalent hardware claims; benchmark harness reports local results | structural |

OLGP supports both pairing request shapes. The stronger Noise XX / SPAKE2 /
Ed25519 / six-digit-code exchange remains the default CLI path. `POST /v1/pairing`
also accepts the documented `expectedDeviceId` and `clientName` fields during
the same bounded first-pair or invitation window and returns a scoped
`engine:read` / `engine:write` token.

## Verified live

- `scripts/check-contract.sh`: `contract ok: 65 routes, 9 TXT keys`.
- `scripts/run-all.sh`: deterministic checks pass; hardware RTT and CUDA remain explicit skips.
- Linux GPU box: mDNS discovery, SPAKE2 pairing, credential mode `0600`,
  `doctor`, capabilities with `vram_mib=4096`, TLS listener, and five wrong-code
  rejections with no credential written.
- Live engine session: `POST /engine/sessions` returned a ready session and `GET /engine/sessions` listed it.
- Documented pairing: `expectedDeviceId`/`clientName` issued a scoped token, closed the window, and the token reached product and engine routes (`200/200/201`).
- C++ `olgp_semantic_test` and `olgp_dispatch_test` pass.
- Live WebSocket: `/v1/realtime?mode=voice&token=...` returned `101 Switching Protocols`; without a token it returned `401 Unauthorized`.
- SDK static checks: Swift `swiftc -parse` and TypeScript `Bun.build` pass; C# has no local compiler.

## Remaining structural non-parity

Purpose-built hardware, Wi-Fi 7 radio/firmware, macOS/Windows kernel drivers,
Vulkan/D3D compatibility, proprietary model weights, private beta SDK binaries,
and vendor benchmark numbers are not clean-room protocol deliverables. OLGP
does not claim to be WiCi One or to interoperate with it.
