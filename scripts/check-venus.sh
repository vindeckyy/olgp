#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bash -n scripts/build-virglrenderer.sh
bash -n scripts/venus-vm.sh
cargo run -q -p olgp-cli --bin olgpctl -- venus command --config virtual/venus.toml \
  | grep -q 'virtio-gpu-gl,hostmem=4096M,blob=true,venus=true'
echo "venus config ok"
