#!/usr/bin/env bash
set -euo pipefail
workspace=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$workspace"
apk add --no-cache alpine-sdk abuild bash curl git python3 perl-json \
    autoconf automake libtool flex bison cmake ninja pkgconf spirv-headers vulkan-headers \
    alsa-lib-dev eudev-dev wayland-dev libxkbcommon-dev fontconfig-dev \
    patchelf openssl zstd wine-dev vulkan-loader
export CARGO_HOME="$RUNNER_TEMP/cargo-home"
export RUSTUP_HOME="$RUNNER_TEMP/rustup-home"
curl --fail --location https://sh.rustup.rs -o "$RUNNER_TEMP/rustup.sh"
sh "$RUNNER_TEMP/rustup.sh" -y --profile minimal --default-toolchain "$OPENZT2_RUST_VERSION"
export PATH="$CARGO_HOME/bin:$PATH"
export OPENZT2_RELEASE_HOST=1
export OPENZT2_NATIVE_DEPENDENCY_DIR="$RUNNER_TEMP/openzt2-native"
export KAI_CARGO_TARGET_ROOT="$RUNNER_TEMP/cargo-targets"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS='-C target-feature=-crt-static'
bash tools/release/build-native.sh
export MOJOSHADER_DIR="$OPENZT2_NATIVE_DEPENDENCY_DIR/include"
export PKG_CONFIG_PATH="$OPENZT2_NATIVE_DEPENDENCY_DIR/lib/pkgconfig"
export LIBRARY_PATH="$OPENZT2_NATIVE_DEPENDENCY_DIR/lib"
export LD_LIBRARY_PATH="$OPENZT2_NATIVE_DEPENDENCY_DIR/lib"
export CARGO_TARGET_DIR="$KAI_CARGO_TARGET_ROOT/openzt2-reimplementation"
cargo install --locked --features cli cargo-about --version 0.9.0
python3 tools/release/package.py package --target linux-x86_64-musl
