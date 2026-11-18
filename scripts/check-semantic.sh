#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
BUILD_DIR="${OLGP_SEMANTIC_BUILD_DIR:-/tmp/olgp-semantic-build}"
cmake -S third_party/lupine -B "$BUILD_DIR" \
  -DLUPINE_BUILD_CUDA=OFF -DLUPINE_BUILD_NVML=OFF -DLUPINE_BUILD_HIP=OFF \
  -DLUPINE_BUILD_SERVER=OFF -DOLGP_PATCHES=ON >/dev/null
cmake --build "$BUILD_DIR" --parallel 4 >/dev/null
ctest --test-dir "$BUILD_DIR" --output-on-failure -R olgp_semantic_test
