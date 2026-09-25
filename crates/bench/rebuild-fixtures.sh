#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

RUSTFLAGS="-C target-feature=+simd128,+relaxed-simd,+reference-types,+bulk-memory,+mutable-globals,+multivalue,+sign-ext,+nontrapping-fptoint" \
    cargo build --manifest-path guest/Cargo.toml --release --target wasm32-unknown-unknown --locked
for bin in argon2id compression json nested; do
    wasm-opt "guest/target/wasm32-unknown-unknown/release/$bin.wasm" \
        -o "guest/target/$bin.opt.wasm" -O3 \
        --enable-simd --enable-relaxed-simd --enable-tail-call --enable-extended-const \
        --enable-reference-types --enable-bulk-memory --enable-mutable-globals \
        --enable-multivalue --enable-sign-ext --enable-nontrapping-float-to-int \
        --duplicate-function-elimination
    cp "guest/target/$bin.opt.wasm" "fixtures/$bin.wasm"
done
