#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

python3 - <<'PY'
import pathlib
import re

contract = pathlib.Path("spec/v1/contracts.md").read_text()
api = pathlib.Path("crates/olgp-control/src/api.rs").read_text()
txt = pathlib.Path("spec/v1/txt-record.md").read_text()
proto = "\n".join(p.read_text() for p in pathlib.Path("proto/olgp/v1").glob("*.proto"))

route_rows = re.findall(r"^\| (GET|POST|WS) \| `([^`]+)` \|", contract, re.M)
route_count = len(route_rows)
api_routes = set(re.findall(r'method: "(\w+)", path: "([^"]+)"', api))
contract_routes = {(method, path.split("?")[0]) for method, path in route_rows}
if route_count != 19:
    raise SystemExit(f"contract has {route_count} routes, expected 19")
missing = contract_routes - api_routes
extra = api_routes - contract_routes
if missing or extra:
    raise SystemExit(f"route mismatch: missing={sorted(missing)} extra={sorted(extra)}")

txt_keys = set(re.findall(r"^\| `([a-z_]+)` \|", txt, re.M))
if len(txt_keys) != 9:
    raise SystemExit(f"TXT keys has {len(txt_keys)} entries, expected 9")
for key in ("PairingRequest", "CapabilitiesResponse", "LeaseRequest", "Counters"):
    if f"message {key}" not in proto:
        raise SystemExit(f"missing proto message {key}")

print(f"contract ok: {route_count} routes, {len(txt_keys)} TXT keys")
PY
