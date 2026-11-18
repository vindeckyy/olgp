#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
import json
import pathlib

manifest = json.loads(pathlib.Path("packaging/first-boot.json").read_text())
assert manifest["version"] == 1
assert manifest["services"]["api"] == 8787
assert manifest["services"]["rpc"] == 8843
assert manifest["pairing"]["max_attempts"] == 5
assert len(manifest["discovery"]["txt_keys"]) == 9
for key in ["config", "tls_cert", "tls_key", "spki"]:
    assert key in manifest["paths"], key
print("packaging manifest ok")
PY
