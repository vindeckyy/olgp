#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
INTERFACE="${OLGP_TEST_INTERFACE:-}"
if [[ -z "$INTERFACE" ]]; then
  echo "OLGP_TEST_INTERFACE is required" >&2
  exit 2
fi

if ! command -v tc >/dev/null 2>&1; then
  echo "tc is required for the emulated round trip" >&2
  exit 2
fi

sudo tc qdisc replace dev "$INTERFACE" root netem delay 750us
trap 'sudo tc qdisc del dev "$INTERFACE" root >/dev/null 2>&1 || true' EXIT

cargo run -q -p olgp-cli -- bench --workload dependency-chain --calls 20000
