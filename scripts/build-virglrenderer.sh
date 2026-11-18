#!/usr/bin/env bash
set -euo pipefail

ROOT="${VIRGLRENDERER_SRC:-third_party/virglrenderer}"
BUILD="${VIRGLRENDERER_BUILD:-build-virglrenderer}"

if [[ ! -d "$ROOT/.git" ]]; then
  git clone https://gitlab.freedesktop.org/virgl/virglrenderer.git "$ROOT"
fi
meson setup "$BUILD" "$ROOT" -Dvenus=true
meson compile -C "$BUILD"
echo "virglrenderer built in $BUILD"
echo "vtest server: $BUILD/vtest/virgl_test_server"
