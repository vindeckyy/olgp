#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
CONFIG="${OLGP_VENUS_CONFIG:-virtual/venus.toml}"
exec cargo run -q -p olgp-cli --bin olgpctl -- venus run --config "$CONFIG"
