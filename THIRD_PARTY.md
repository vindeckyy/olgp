# Third-party components

| Component | Revision | License | Use |
|---|---|---|---|
| LUPINE | `74c5bbd0089b9ceb659699fb45fde866252682fb` | Apache-2.0 | CUDA/NVML shim and GPU-over-IP RPC substrate |
| InferSSD | `e6a9de5891842b9c4f8d1949c9c93e06dcaec472` | Apache-2.0 | SSD-backed MoE engine reference and runtime |
| Lumen crypto subset | local `lumen` workspace, read 2026-11-17 | MIT OR Apache-2.0 | Noise XX, SPAKE2, AEAD, pairing patterns |
| Lumen transport subset | local `lumen` workspace, read 2026-11-17 | MIT OR Apache-2.0 | Framing, FEC, pacing, loss estimation patterns |

Vendored copies keep their own LICENSE/NOTICE files. Model weights are not
redistributed and remain under their publishers' licenses.

The kernel module under `kernel/` is licensed `GPL-2.0-only`, separately from
the Apache-2.0 userspace and specification code.

The WiCi technical report is CC BY-NC-ND 4.0. No text, figures, or code were
copied from it; only independently implemented architectural behavior is used.
