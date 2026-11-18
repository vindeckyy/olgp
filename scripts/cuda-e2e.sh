#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
FIXTURE="${OLGP_CUDA_FIXTURE:-tests/e2e/cuda_vecadd.cu}"
OUTPUT="${OLGP_CUDA_OUTPUT:-/tmp/olgp-cuda-vecadd}"
SHIM="${OLGP_CUDA_SHIM:-/usr/lib/olgp/libcuda.so.1}"

if [[ ! -x "${NVCC:-nvcc}" ]]; then
  echo "nvcc is required for the CUDA shim check" >&2
  exit 2
fi
if [[ ! -f "$SHIM" ]]; then
  echo "install the driver shim with: olgpctl install-driver" >&2
  exit 2
fi

"${NVCC:-nvcc}" -o "$OUTPUT" "$FIXTURE"
LD_PRELOAD="$SHIM" "$OUTPUT" | grep -q '^sum ok$'
echo "cuda shim check passed"
