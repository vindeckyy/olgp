#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
WORK="${OLGP_API_COMPAT_DIR:-/tmp/olgp-api-compat}"
mkdir -p "$WORK/tls"

python3 - <<PY
import pathlib
path = pathlib.Path("$WORK/node.toml")
path.write_text(
    "box_id = " + "a" * 32 + "\n"
    "api_port = 18787\n"
    "rpc_port = 18843\n"
    "engine_port = 18080\n"
    "vram_reserve_mib = 512\n"
    "vram_mib = 4096\n"
    "cache_min_bytes = 67108864\n"
    "max_interactive_sessions = 1\n"
    "log_level = info\n"
    "address = 127.0.0.1\n"
    "hostname = olgp-compat\n"
    "gpu = Test GPU\n"
)
PY

openssl req -x509 -newkey rsa:2048 -nodes \
  -keyout "$WORK/tls/key.pem" -out "$WORK/tls/cert.pem" \
  -days 1 -subj '/CN=olgp.local' >/dev/null 2>&1
openssl x509 -in "$WORK/tls/cert.pem" -pubkey -noout \
  | openssl pkey -pubin -outform DER 2>/dev/null \
  | openssl dgst -sha256 -hex | awk '{print $2}' > "$WORK/spki.sha256"

python3 tests/e2e/fake_engine.py 18080 >"$WORK/engine.log" 2>&1 &
engine=$!
OLGP_CONFIG="$WORK/node.toml" OLGP_SPKI_PATH="$WORK/spki.sha256" \
OLGP_PAIRING_CODE_PATH="$WORK/pairing.code" \
OLGP_CERT_PATH="$WORK/tls/cert.pem" OLGP_KEY_PATH="$WORK/tls/key.pem" \
OLGP_HOUSEHOLD_PATH="$WORK/household.json" \
OLGP_MEMORY_PATH="$WORK/memory.json" \
OLGP_SPEAKER_PATH="$WORK/speakers.json" \
OLGP_RAG_PATH="$WORK/rag.json" \
OLGP_IMAGES_PATH="$WORK/images.json" \
OLGP_RECORDINGS_ROOT="$WORK/recordings" \
cargo run -q -p olgp-node >"$WORK/node.log" 2>&1 &
node=$!
trap 'kill $engine $node >/dev/null 2>&1 || true' EXIT
sleep 3

curl -fsS -X POST http://127.0.0.1:18787/v1/chat/completions \
  -H 'Authorization: Bearer compat-token' \
  -H 'Content-Type: application/json' \
  --data '{"model":"qwen25-0p5","messages":[{"role":"user","content":"hello"}]}' \
  | grep -q 'compat ok'

curl -fsS http://127.0.0.1:18787/v1/models \
  -H 'Authorization: Bearer compat-token' | grep -q 'qwen25-0p5'

echo "api compatibility checks passed"
