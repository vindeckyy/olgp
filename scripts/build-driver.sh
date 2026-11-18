#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
BUILD_DIR="${OLGP_DRIVER_BUILD_DIR:-build-driver}"
STAGE_DIR="${OLGP_DRIVER_STAGE_DIR:-staging/driver}"

cmake -S third_party/lupine -B "$BUILD_DIR" \
  -DLUPINE_BUILD_CUDA=ON \
  -DLUPINE_BUILD_NVML=ON \
  -DLUPINE_BUILD_HIP=OFF \
  -DLUPINE_BUILD_SERVER=OFF \
  -DOLGP_PATCHES=ON \
  -DCMAKE_BUILD_TYPE=Release
cmake --build "$BUILD_DIR" --parallel "${OLGP_BUILD_JOBS:-4}" \
  --target lupine_cuda_client lupine_nvml_client

mkdir -p "$STAGE_DIR"
cp "$BUILD_DIR/libcuda.so.1.0.0" "$STAGE_DIR/libcuda.so.1" 2>/dev/null || \
  cp "$BUILD_DIR/libcuda.so.1" "$STAGE_DIR/libcuda.so.1"
cp "$BUILD_DIR/libnvidia-ml.so.1.0.0" "$STAGE_DIR/libnvidia-ml.so.1" 2>/dev/null || \
  cp "$BUILD_DIR/libnvidia-ml.so.1" "$STAGE_DIR/libnvidia-ml.so.1"
cargo build --release -p olgp-driver
cp target/release/libolgp_driver.so "$STAGE_DIR/libolgp_driver.so"
chmod 0755 "$STAGE_DIR/libcuda.so.1" "$STAGE_DIR/libnvidia-ml.so.1" "$STAGE_DIR/libolgp_driver.so"
echo "driver shim staged in $STAGE_DIR"
