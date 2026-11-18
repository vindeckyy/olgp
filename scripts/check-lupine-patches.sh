#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PY'
import pathlib

required = {
    "third_party/lupine/transport.cpp": [
        "olgp_read_json_string",
        "/etc/olgp/active-box.json",
        "olgp_verify_spki_pin",
        "olgp_servers = olgp_active_endpoint()",
        "handshake_ok && olgp_verify_spki_pin(session)",
    ],
    "third_party/lupine/CMakeLists.txt": [
        "option(OLGP_PATCHES",
        "add_compile_definitions(OLGP_PATCHES=1)",
        "olgp_semantic.cpp",
        "olgp_semantic_test",
        "olgp_dispatch.cpp",
        "olgp_dispatch_test",
    ],
    "third_party/lupine/rpc.h": [
        "OLGP_OP_BASE = 0x8000",
        "OLGP_OP_LEASE = OLGP_OP_BASE + 9",
    ],
    "third_party/lupine/cuda_client_rpc.cpp": ["olgp_note_call(op)"],
    "third_party/lupine/cache.h": ["olgp_handle_class", "olgp_refcount"],
    "third_party/lupine/olgp_semantic.h": [
        "class BatchRecognizer",
        "class TraceRecorder",
        "class HandleTable",
    ],
    "third_party/lupine/olgp_dispatch.h": [
        "encode_batch",
        "decode_batch",
        "class TraceSessions",
        "class ContentCache",
    ],
    "third_party/lupine/olgp_dispatch.cpp": [
        "olgp_dispatch_cache_query",
        "olgp_dispatch_cache_put",
        "olgp_dispatch_batch_wire_size",
    ],
    "third_party/lupine/UPSTREAM_PIN": [
        "74c5bbd0089b9ceb659699fb45fde866252682fb",
    ],
}
for relative, needles in required.items():
    text = pathlib.Path(relative).read_text()
    for needle in needles:
        if needle not in text:
            raise SystemExit(f"missing {needle!r} in {relative}")
print("lupine patches ok")
PY
