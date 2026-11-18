#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
DIR="${OLGP_CONFIG_DIR:-/etc/olgp}"
STAGE="${OLGP_DRIVER_STAGE_DIR:-staging/driver}"

cargo run -q -p olgp-cli --bin olgpctl -- setup --dir "$DIR"
scripts/build-driver.sh
cargo build --release -p olgp-cli -p olgp-node
sudo env OLGP_DRIVER_STAGE_DIR="$STAGE" target/release/olgpctl install-driver

cat <<EOF

Setup complete.

On this box, run:
  target/release/olgp-node --config $DIR/node.toml

On a paired client, run:
  olgpctl discover
  olgpctl pair <box-id> --code <one-time-code>
  sudo olgpctl attach --box <box-id>
  nvidia-smi -L

EOF
