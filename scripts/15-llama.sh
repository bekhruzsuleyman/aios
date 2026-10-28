#!/usr/bin/env bash
# Local AI engine: llama-server from llama.cpp, built from source at a fixed
# tag into build/llama/ (installed to /usr/lib/aios/llama in the image by
# scripts/20-rootfs.sh). CPU only. Every x86-64 CPU variant is built and the
# best one is picked at run time, so it runs on old laptops and in VMs that
# hide AVX2. The source is cloned into third_party/llama.cpp.
set -euo pipefail
cd "$(dirname "$0")/.."
TAG=b11968
SRC=third_party/llama.cpp
OUT=$PWD/build/llama
if [ ! -d "$SRC/.git" ]; then
  git clone --depth 1 --branch "$TAG" https://github.com/ggml-org/llama.cpp.git "$SRC"
fi
[ "$(git -C "$SRC" describe --tags --exact-match 2>/dev/null)" = "$TAG" ] || {
  echo "$SRC is not at $TAG" >&2; exit 1; }
cmake -S "$SRC" -B "$SRC/build" -G Ninja -DCMAKE_BUILD_TYPE=Release \
  -DBUILD_SHARED_LIBS=ON -DGGML_NATIVE=OFF -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON \
  -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=ON \
  -DLLAMA_BUILD_UI=OFF -DLLAMA_USE_PREBUILT_UI=OFF -DLLAMA_OPENSSL=OFF -DLLAMA_CONNECT=OFF \
  -DCMAKE_INSTALL_PREFIX=/usr/lib/aios/llama -DCMAKE_INSTALL_RPATH='$ORIGIN/../lib' \
  -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON
cmake --build "$SRC/build" --target llama-server -j"$(nproc)"
rm -rf "$OUT" && mkdir -p "$OUT/bin" "$OUT/lib"
cp "$SRC/build/bin/llama-server" "$OUT/bin/"
cp -a "$SRC"/build/bin/*.so* "$OUT/lib/" 2>/dev/null || true
find "$SRC/build" -name '*.so*' \( -name 'libllama*' -o -name 'libggml*' -o -name 'libmtmd*' \) -exec cp -a {} "$OUT/lib/" \;
# llama-server looks for the CPU backends (one per CPU generation) next to
# itself.
mv "$OUT"/lib/libggml-cpu-*.so "$OUT/bin/"
strip --strip-unneeded "$OUT/bin/llama-server" "$OUT"/lib/*.so 2>/dev/null || true
echo "$TAG" > "$OUT/VERSION"
du -sh "$OUT"
