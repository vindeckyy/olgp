# TXT record reference

All TXT values are ASCII and at most 255 bytes. Keys are case sensitive.

| Key | Required | Example | Verified when |
|---|---|---|---|
| `v` | yes | `1` | before pairing |
| `id` | yes | `9f2a...c4` (32 hex) | before pairing |
| `fp` | yes | `sha256 hex` | before pairing |
| `api` | yes | `8787` | after pairing |
| `rpc` | yes | `8843` | after pairing |
| `caps` | yes | `cuda,nvml,batch,trace,nvme,voice` | after pairing |
| `gpu` | yes | GPU name reported by NVML | after pairing |
| `vram_mib` | yes | `4096` | after pairing |
| `box` | yes | `olgp-box` | after pairing |

A response with a missing required key, a non-hex `id`/`fp`, or a non-decimal
port is discarded. Duplicate keys are rejected. More than nine keys is
rejected in v1.
