# OLGP engine supervisor

The supervisor starts the loopback model server. It never binds to the LAN.

## Profiles

`profiles.toml` defines:

- `qwen25-0p5` — dense GGUF through llama.cpp.
- `qwen25-1p5` — dense GGUF through llama.cpp.
- `qwen15-moe-a2p7` — MoE GGUF through InferSSD.
- `turbo` — large-model profile through InferSSD, configured by environment.

For `turbo`, set:

```sh
export OLGP_TURBO_GGUF=/path/to/large-model.gguf
```

The turbo profile does not bundle weights. It uses the supplied local GGUF and
the same hash-sidecar verification path as the other profiles.

## Benchmark manifest

```sh
scripts/bench-models.sh
```

This writes `artifacts/e2e/models.csv` with model, engine, context, batch, and
minimum memory for each profile. If `OLGP_BENCH_BOX` is set and `olgpctl` is on
`PATH`, it also records the box status.

## Voice streaming

`src/asr.py` provides a deterministic 500 ms chunk adapter for `voice-v1`.
`EchoBackend` is used by tests. When `sherpa-onnx` is installed,
`sherpa_backend_factory()` returns a callable that can be used with
`StreamingAsr` for streaming transcription. The deterministic gate in
`src/voice.py` uses only online-visible features and can emit `text_tentative`
before final transcription.

The WebSocket gateway handles `text_input`, `stt_partial`, `text_tentative`,
`commit`, `cancel`, `reset`, `text`, and `end`. A client can cancel an early
answer by sending `cancel` during a turn, which is the barge-in path.
