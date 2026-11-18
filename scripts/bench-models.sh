#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
OUT="${OLGP_BENCH_OUT:-artifacts/e2e/models.csv}"
mkdir -p "$(dirname "$OUT")"

echo "model,engine,context,batch,min_vram_mib" > "$OUT"
python3 - "$OUT" <<'PY'
import sys
import tomllib

profiles = tomllib.load(open("engine/olgp-engine/profiles.toml", "rb"))
alias = {
    "small": "qwen25-0p5",
    "medium": "qwen25-1p5",
    "moe": "qwen15-moe-a2p7",
}
with open(sys.argv[1], "a") as out:
    for name, profile in profiles.items():
        out.write(
            f"{alias.get(name, name)},{profile['engine']},{profile['context']},"
            f"{profile['batch']},{profile['min_vram_mib']}\n"
        )
PY

if command -v olgpctl >/dev/null 2>&1 && [[ -n "${OLGP_BENCH_BOX:-}" ]]; then
  olgpctl status "$OLGP_BENCH_BOX"
fi

echo "model benchmark manifest: $OUT"
