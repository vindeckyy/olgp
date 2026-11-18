#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
mkdir -p target

THRESHOLD=95
PYTHON="${PYTHON:-python3}"

cargo llvm-cov --workspace --lib --lcov \
  --output-path target/lcov-control.info \
  --ignore-filename-regex 'third_party|/usr/|rustc/' >/dev/null

PYTHON_THRESHOLD="$THRESHOLD" "$PYTHON" - <<'PY'
import os
import pathlib

threshold = float(os.environ["PYTHON_THRESHOLD"])
keep = {
    "crates/olgp-control/src/api.rs",
    "crates/olgp-control/src/bench.rs",
    "crates/olgp-control/src/config.rs",
    "crates/olgp-control/src/pairing.rs",
    "crates/olgp-control/src/sched.rs",
    "crates/olgp-control/src/shim.rs",
    "crates/olgp-control/src/household.rs",
    "crates/olgp-control/src/images.rs",
    "crates/olgp-control/src/memory.rs",
    "crates/olgp-control/src/product.rs",
    "crates/olgp-control/src/rag.rs",
    "crates/olgp-control/src/recordings.rs",
    "crates/olgp-control/src/speaker.rs",
    "crates/olgp-driver/src/lib.rs",
    "crates/olgp-control/src/telemetry.rs",
    "crates/olgp-control/src/voice.rs",
    "crates/olgp-control/src/venus.rs",
    "crates/olgp-control/src/registry.rs",
    "crates/olgp-control/src/netlink.rs",
}
found = covered = 0
current = None
line_count = None
for raw in pathlib.Path("target/lcov-control.info").read_text().splitlines():
    if raw.startswith("SF:"):
        current = raw[3:]
        line_count = None
    elif raw.startswith("LF:"):
        line_count = int(raw[3:])
    elif raw.startswith("LH:") and current and line_count is not None:
        if any(current.endswith(suffix) for suffix in keep):
            covered += int(raw[3:])
            found += line_count
        current = None
        line_count = None
percent = 100.0 * covered / found if found else 0.0
print(f"rust coverage: {covered}/{found} lines ({percent:.2f}%)")
if percent < threshold:
    raise SystemExit(f"rust coverage {percent:.2f}% is below {threshold:.0f}%")
PY

if ! "$PYTHON" -c 'import coverage' >/dev/null 2>&1; then
  echo "coverage module is required for the Python 95% gate" >&2
  exit 1
fi
"$PYTHON" -m coverage erase
"$PYTHON" -m coverage run \
  --source=engine/olgp-engine/src,sdk/python/olgp \
  -m unittest discover -s tests/unit >/dev/null
"$PYTHON" -m coverage json -o target/coverage-python.json >/dev/null
THRESHOLD="$THRESHOLD" "$PYTHON" - <<'PY'
import json
import os

threshold = float(os.environ["THRESHOLD"])
data = json.loads(open("target/coverage-python.json").read())
percent = data["totals"]["percent_covered"]
covered = data["totals"]["covered_lines"]
found = data["totals"]["num_statements"]
print(f"python coverage: {covered}/{found} lines ({percent:.2f}%)")
if percent < threshold:
    raise SystemExit(f"python coverage {percent:.2f}% is below {threshold:.0f}%")
PY

echo "coverage gate passed: ${THRESHOLD}%"
