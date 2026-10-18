#!/usr/bin/env bash
set -euo pipefail
workspace=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$workspace"
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')
output="$workspace/build/releases/nix-x86_64"
mkdir -p "$output"
nix build .#packages.x86_64-linux.default --out-link "$RUNNER_TEMP/openzt2-nix"
readlink -f "$RUNNER_TEMP/openzt2-nix" > "$output/OpenZT2-$version-nix-x86_64.store-path"
mapfile -t closure < <(nix-store --query --requisites "$RUNNER_TEMP/openzt2-nix")
nix-store --export "${closure[@]}" | zstd -T2 -o "$output/OpenZT2-$version-nix-x86_64.nar.zst"
python3 tools/release/release_targets.py checksums nix-x86_64 "$output"
