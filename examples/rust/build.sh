#!/usr/bin/env bash
cd "$(dirname "$0")" || exit

bins=("host_fn" "hello" "fibonacci" "print" "tinywasm" "argon2id")
out_dir="./target/wasm32-unknown-unknown/wasm"
dest_dir="out"

rust_features="+sign-ext,+simd128,+reference-types,+bulk-memory,+bulk-memory-opt,+multimemory,+call-indirect-overlong,+mutable-globals,+multivalue,+sign-ext,+nontrapping-fptoint,+extended-const,+tail-call"
# wasmopt_features="--enable-reference-types --enable-bulk-memory --enable-mutable-globals --enable-multivalue --enable-sign-ext --enable-nontrapping-float-to-int"

# ensure out dir exists
mkdir -p "$dest_dir"

# build no_std
cargo build --target wasm32-unknown-unknown --package rust-wasm-examples --profile=wasm --bin tinywasm_no_std --no-default-features
cp "$out_dir/tinywasm_no_std.wasm" "$dest_dir/"

for bin in "${bins[@]}"; do
    RUSTFLAGS="-C target-feature=$rust_features -C panic=abort" cargo build --target wasm32-unknown-unknown --package rust-wasm-examples --profile=wasm --bin "$bin"

    cp "$out_dir/$bin.wasm" "$dest_dir/"
    # wasm-opt "$dest_dir/$bin.wasm" -o "$dest_dir/$bin.opt.wasm" -O3 $wasmopt_features
done
