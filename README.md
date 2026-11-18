# OLGP — Open Local GPU Protocol

Apache-2.0 clean-room local GPU protocol. A paired Linux client sees a nearby
NVIDIA GPU through the CUDA and NVML shims without per-app environment
variables. An OpenAI-compatible API is also provided.

This is userspace GPU virtualization: the remote GPU appears through
`libcuda.so.1` and `libnvidia-ml.so.1`, not as a PCI device in `lspci`.
For a literal paravirtualized GPU, see `virtual/README.md`: virtio-gpu Venus
exposes a real Vulkan `VkPhysicalDevice` inside a Linux guest. The CUDA/NVML
shim path remains separate.

## Layout

- `spec/v1/` — normative protocol, TXT, error, and route contracts.
- `crates/olgp-control/` — discovery, pairing, scheduler, gateway, telemetry.
- `crates/olgp-node/` — box daemon.
- `crates/olgp-cli/` — `olgpctl` and the shim attach/detach logic.
- `engine/olgp-engine/` — loopback model supervisor.
- `sdk/python/`, `sdk/typescript/` — thin HTTP clients.
- `third_party/` — pinned Apache-2.0/MIT OR Apache-2.0 dependencies.
- `sdk/swift/` — Swift HTTP client.
- `sdk/python/olgp/torch_support.py` — `olgp.gpu(...)` device-like helper.
- `sdk/csharp/` — C# HTTP client.
- `crates/olgp-control/src/netlink.rs` — measured multi-link selection.

## Quickstart

```sh
scripts/quickstart.sh
```

This generates TLS material and node config, builds the CUDA/NVML shims plus
the first-party `olgp-driver` helper, installs them, and prints the remaining
pairing commands.

## Build and test

```sh
cargo test --workspace
python3 -m unittest discover -s tests/unit -v
scripts/check-contract.sh
scripts/check-lupine-patches.sh
scripts/check-semantic.sh
scripts/check-api-compat.sh
scripts/check-coverage.sh
scripts/run-all.sh
```

`scripts/check-coverage.sh` enforces a 95% line-coverage gate. It measures the
deterministic first-party Rust logic and all first-party Python modules.
Network runtime glue, executable entrypoints, generated code, and vendored
third-party code are exercised by the integration checks instead.

## Pairing and GPU attachment

Build and install the driver package before attaching a client:

```sh
scripts/build-driver.sh
sudo olgpctl install-driver
```

The package contains the first-party `libolgp_driver.so` helper for box
selection and readiness checks, plus the patched LUPINE `libcuda.so.1` and
`libnvidia-ml.so.1` backends.

The CUDA shim end-to-end check is `scripts/cuda-e2e.sh`; it requires a paired
GPU machine and the installed shim.

```sh
olgpctl discover
olgpctl pair <box-id> --code <six-digit-code>
olgpctl doctor <box-id>
sudo olgpctl attach --box <box-id>
nvidia-smi -L
sudo olgpctl detach
```

`attach` is system-wide and reversible; it keeps a backup of the dynamic
linker preload file and refuses to overwrite foreign entries without `--force`.

## Public endpoints

The box exposes `GET /v1/health/live`, `GET /v1/capabilities`, `GET /metrics`,
pairing routes, and Device-Token-protected OpenAI/Ollama-compatible routes on
port `8787`. CUDA/NVML RPC uses TLS on port `8843` and is reverse-proxied to the
local GPU server.


Spec: `spec/v1/protocol.md`. License: Apache-2.0. Third-party inventory:
`THIRD_PARTY.md`.
