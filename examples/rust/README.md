# WebAssembly Rust Examples

This is a separate crate that generates WebAssembly from Rust code.
It is used by the `wasm-rust` example.

Requires the `wasm32-unknown-unknown` target to be installed.

To build the example artifacts used by `cargo run --example wasm-rust -- <name>`, run `./examples/rust/build.sh`.
That script also requires `binaryen` and `wabt` to be installed.
