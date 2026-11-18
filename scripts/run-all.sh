#!/usr/bin/env bash
set -uo pipefail

cd "$(dirname "$0")/.."
mkdir -p artifacts/e2e
REPORT=artifacts/e2e/report.json
PYTHON="${PYTHON:-python3}"

python3 - "$REPORT" <<'PY'
import json
import pathlib
import sys
pathlib.Path(sys.argv[1]).write_text("{}\n")
PY

record() {
  local name="$1" status="$2" detail="$3"
  python3 - "$REPORT" "$name" "$status" "$detail" <<'PY'
import json
import sys
path, name, status, detail = sys.argv[1:5]
data = json.loads(open(path).read() or "{}")
data[name] = {"status": status, "detail": detail}
open(path, "w").write(json.dumps(data, indent=2) + "\n")
PY
}

run() {
  local name="$1"; shift
  if "$@" >"/tmp/olgp-${name}.log" 2>&1; then
    record "$name" passed "log:/tmp/olgp-${name}.log"
  else
    record "$name" failed "log:/tmp/olgp-${name}.log"
  fi
}

skip() {
  record "$1" skipped "$2"
}

run rust_tests cargo test --workspace
run python_tests "$PYTHON" -m unittest discover -s tests/unit -v
run contract scripts/check-contract.sh
run lupine_patches scripts/check-lupine-patches.sh
run coverage scripts/check-coverage.sh
run driver_script bash -n scripts/build-driver.sh
run semantic_tests scripts/check-semantic.sh
run venus scripts/check-venus.sh
run quickstart_script bash -n scripts/quickstart.sh
run kernel scripts/check-kernel.sh
run bench_models scripts/bench-models.sh

run api_compat scripts/check-api-compat.sh
run packaging scripts/check-packaging.sh
if [[ "${OLGP_E2E:-0}" == "1" ]]; then
  run rtt scripts/bench-rtt.sh
  run cuda scripts/cuda-e2e.sh
else
  skip rtt "hardware run not requested; set OLGP_E2E=1 on the paired machines"
  skip cuda "hardware run not requested; set OLGP_E2E=1 on the paired machines"
fi
run voice scripts/voice-fixture.sh

python3 - "$REPORT" <<'PY'
import json
import sys
data = json.loads(open(sys.argv[1]).read())
failed = [name for name, row in data.items() if row["status"] == "failed"]
print(json.dumps(data, indent=2))
if failed:
    raise SystemExit(f"failed steps: {', '.join(failed)}")
PY
