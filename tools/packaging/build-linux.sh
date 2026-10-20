#!/usr/bin/env bash
# Builds the Linux x64 packages of the committed HEAD in the Ubuntu 24.04 builder container.
# Uncommitted changes are not included. Arguments go to tools/package.mjs (for example
# --identity acceptance); packages are written to $OUT (default target/packages).
set -euo pipefail

root=$(git rev-parse --show-toplevel)
commit=$(git -C "$root" rev-parse HEAD)
out=$(realpath -m "${OUT:-$root/target/packages}")
image=lizzieyzy-tauri-linux-builder:ubuntu24.04

docker build -t "$image" -f "$root/tools/packaging/linux-builder.Dockerfile" "$root/tools/packaging"

mkdir -p "$out"
input=$(mktemp -d)
trap 'rm -rf "$input"' EXIT
git -C "$root" bundle create "$input/source.bundle" HEAD

# Named volumes keep downloaded crates, npm packages and Tauri's bundling tools between builds.
docker run --rm \
  -v "$input:/in:ro" -v "$out:/out" \
  -v lizzieyzy-tauri-cargo-registry:/usr/local/cargo/registry \
  -v lizzieyzy-tauri-npm-cache:/root/.npm \
  -v lizzieyzy-tauri-tauri-cache:/root/.cache/tauri \
  "$image" bash -euo pipefail -c '
    git clone -q /in/source.bundle /src
    cd /src
    git checkout -q --detach "$0"
    npm ci --no-audit --no-fund
    node tools/package.mjs --out /out "${@:2}"
    chown -R "$1" /out
  ' "$commit" "$(id -u):$(id -g)" "$@"
