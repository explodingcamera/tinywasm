#!/usr/bin/env bash
cargo build --example wasm-rust --profile profiling
samply record -r 10000 ./target/profiling/examples/wasm-rust $@
