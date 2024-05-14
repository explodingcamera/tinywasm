#!/usr/bin/env bash
cd "$(dirname "$0")"

bins=("hello" "fibonacci" "print" "tinywasm" "argon2id")
exclude_wat=("tinywasm")
out_dir="./target/wasm32-unknown-unknown/wasm"
dest_dir="out"

features="+reference-types,+bulk-memory,+mutable-globals"

# ensure out dir exists
mkdir -p "$dest_dir"

for bin in "${bins[@]}"; do
    RUSTFLAGS="-C target-feature=$features -C panic=abort" cargo build --target wasm32-unknown-unknown --package rust-wasm-examples --profile=wasm --bin "$bin"

    cp "$out_dir/$bin.wasm" "$dest_dir/"
    wasm-opt "$dest_dir/$bin.wasm" -o "$dest_dir/$bin.wasm" -O3 --enable-bulk-memory --enable-reference-types --enable-mutable-globals

    if [[ ! " ${exclude_wat[@]} " =~ " $bin " ]]; then
        wasm2wat "$dest_dir/$bin.wasm" -o "$dest_dir/$bin.wat"
    fi
done
