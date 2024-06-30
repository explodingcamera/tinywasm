#!/usr/bin/env bash
cargo build --example wasm-rust --profile profiling
samply record ./target/profiling/examples/wasm-rust $@
