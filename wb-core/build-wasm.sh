#!/usr/bin/env bash
# Build the WASM artifact for wb-core.
#
# Note: if a non-rustup `rustc` (e.g. Homebrew) is first on PATH, it won't have
# the wasm32 std even after `rustup target add wasm32-unknown-unknown`. Invoke
# the rustup toolchain's binaries directly so the right sysroot is used.
set -euo pipefail
rustup target add wasm32-unknown-unknown
TC="$(rustup show home)/toolchains/$(rustup show active-toolchain | awk '{print $1}')/bin"
RUSTC="$TC/rustc" "$TC/cargo" build -p wb-core --release --target wasm32-unknown-unknown
echo "→ target/wasm32-unknown-unknown/release/wb_core.wasm"
