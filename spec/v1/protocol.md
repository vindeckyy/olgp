# OLGP v1 protocol

OLGP is a clean-room local GPU protocol. It exposes a nearby NVIDIA GPU to
Linux clients as a CUDA/NVML-shaped device and also provides an OpenAI-compatible
HTTP API. This document is normative for version 1.

## 1. Scope and trust model

The box and clients are on one trusted LAN. mDNS is unauthenticated and is used
only for discovery. Every operation that matters is authenticated after pairing.
The CUDA/NVML RPC channel is LUPINE-compatible HTTP/2 over TLS on TCP 8843.
The API channel is HTTP/1.1 JSON and WebSocket on TCP 8787. The inference engine
binds only to 127.0.0.1:8080 and the unproxied LUPINE server binds only to
127.0.0.1:14833.

A client sees an OS-wide GPU device through `/etc/ld.so.preload` of the OLGP
`libcuda.so.1` and `libnvidia-ml.so.1` shims. This is userspace virtualization,
not a PCI device. Static binaries and kernel code that bypass the shims cannot
see the remote GPU.

## 2. Discovery

Service type: `_olgp._tcp.local.`
Instance name: `olgp-<boxid>._olgp._tcp.local.`

TXT records:

| Key | Value |
|---|---|
| `v` | `1` |
| `id` | 32 lowercase hex characters |
| `fp` | SHA-256 of the box SPKI, lowercase hex |
| `api` | API port, default `8787` |
| `rpc` | RPC gateway port, default `8843` |
| `caps` | comma-separated capability set |
| `gpu` | GPU name |
| `vram_mib` | VRAM in MiB, decimal |
| `box` | hostname |

Only `id`, `v`, and `fp` are trusted before pairing. All other TXT fields are
hints verified through `GET /v1/capabilities` after pairing.

Capability tokens in v1: `cuda`, `nvml`, `batch`, `trace`, `nvme`, `voice`.
Unknown tokens are ignored on read and rejected by strict validation.

## 3. Pairing

The control port 8843 uses Noise `XX_25519_ChaChaPoly_SHA256`. The client is the
initiator. The host shows a fresh six-digit SPAKE2 code through
`olgpctl pairing show`. The code never crosses the wire.

The binding value `B` is the client's Noise static public key followed by the
host's Noise static public key, in the order each endpoint saw them. The client
proves possession of its Ed25519 identity by signing
`olgp-pairing-proof-v1 || nonce || host_key || device_key || B`. The host signs
`olgp-host-proof-v1 || host_key || device_key || B` in `HelloAck`.

Host pin file: `$HOME/.config/olgp/boxes/<boxid>.json`, mode 0600, fields
`box_id`, `host_key`, `box_spki_sha256`, `paired_at`, `caps`.

Trusted client store: `/var/lib/olgp/trusted_clients.json`, mode 0600.

First pairing window: 15 minutes after `olgp-node` starts. A client with no host
pin meeting a host that already trusts it is refused. The window can be
reopened for five minutes with `POST /v1/pairing:open` by an authenticated
writer and closed with `POST /v1/pairing:close`. A pairing attempt is refused
after five failed attempts in one window.

Key separation uses HKDF-SHA256 with distinct info strings:
`olgp-control-h2c-v1`, `olgp-control-c2h-v1`, `olgp-api-h2c-v1`, and
`olgp-api-c2h-v1`. Each direction has its own AEAD counter.

## 4. Channels and ports

| Port | Transport | Scope | Purpose |
|---|---|---|---|
| 8787 | TCP, HTTP/1.1 + WS | LAN | API, pairing, metrics |
| 8843 | TCP, HTTP/2 + TLS | LAN | CUDA/NVML RPC gateway |
| 8080 | TCP, HTTP/1.1 | loopback | inference engine |
| 14833 | TCP, HTTP/2 | loopback | unproxied LUPINE server |

## 5. API contract

Public probes: `GET /v1/health/live`, `GET /v1/capabilities`,
`GET /v1/pairing/status`. Protected routes require
`Authorization: Bearer <token>`. OpenAI-compatible clients pass the token as
`api_key`.

Errors use `{"error":{"code":"...","message":"...","retryable":false}}`.
OpenAI-compatible routes also include `type`, `param`, and `retryable`.
Retry only when `retryable` is true.

The complete v1 route set is listed in `contracts.md`.

## 6. Semantic transport

OLGP extension opcodes start at `OLGP_OP_BASE = 0x8000`:

| Opcode | Name |
|---|---|
| `0x8001` | `CACHE_QUERY` |
| `0x8002` | `CACHE_PUT` |
| `0x8003` | `BATCH` |
| `0x8004` | `TRACE_BEGIN` |
| `0x8005` | `TRACE_REPLAY` |
| `0x8006` | `TRACE_ABORT` |
| `0x8007` | `HANDLE_RETAIN` |
| `0x8008` | `HANDLE_RELEASE` |
| `0x8009` | `LEASE` |

Batch payload: `count:u16`, repeated `op:u16`, `payload_len:u32`, `payload`.
Integers are little-endian. One batch is one RPC and one response wait.

Batch patterns: `MEMCPY_H2D_ASYNC -> STREAM_SYNC`,
`MODULE_LOAD_DATA -> CTX_PUSH`, `LAUNCH -> LAUNCH`. The first call may be held
for at most 200 microseconds. If the pattern does not match, the held call is
replayed normally. Success is never synthesized for `MEMCPY_DTOH`, `EVENT_QUERY`,
`STREAM_QUERY`, allocation, free, or context destruction.

Independent `LAUNCH -> LAUNCH` pairs are also pipelined at the RPC scope layer:
the client sends the second launch with `RPC_SCOPE_NONE`, so it does not wait
behind the first launch unless a real dependency is declared.
Trace identifier: first 16 bytes of SHA-256 over `opcode_sequence ||
parameter_template`. SHA-256 is used because the RPC substrate already links
OpenSSL; content-addressed weight blobs continue to use BLAKE3.
A trace has at most 512 operations and a 64 KiB dynamic parameter vector.
On mismatch the client sends `TRACE_ABORT`, replays normally, and invalidates the trace.

## 7. Handles and residency

Handle classes: `1=immutable_weight`, `2=kv_cache`, `3=activation`,
`4=framebuffer`. Only `immutable_weight` with a verified BLAKE3 hash may be
shared across tenants. Raw addresses never cross machines. `HANDLE_RETAIN` and
`HANDLE_RELEASE` maintain refcounts. Eviction of a referenced handle returns
`OLGP_BUSY`.

VRAM holds active layers, hot experts, KV cache, and immediate tensors. RAM
holds warm state and staging buffers. SSD holds immutable weights and cold
experts only. KV cache is never written to SSD. Eviction order: idle KV, cold
immutable weights, warm weights, never active KV.

## 8. Latency model

`T = T_local + N*RTT + D/Rate`. The protocol reduces `N` by batching and trace
replay, reduces `D` by content-addressed caching and logical handles, and
preserves true dependencies exactly at synchronization points.
