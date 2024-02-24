# `tinywasm-parser`

This crate provides a parser that can parse WebAssembly modules into a TinyWasm module.
It uses [my fork](https://crates.io/crates/tinywasm-wasmparser) of the [`wasmparser`](https://crates.io/crates/wasmparser) crate that has been modified to be compatible with `no_std` environments.

## Features

- `std`: Enables the use of `std` and `std::io` for parsing from files and streams.
- `logging`: Enables logging of the parsing process using the `log` crate.

## Usage

```rust
use tinywasm_parser::Parser;
let bytes = include_bytes!("./file.wasm");

let parser = Parser::new();
let module = parser.parse_module_bytes(bytes).unwrap();
let mudule = parser.parse_module_file("path/to/file.wasm").unwrap();
let module = parser.parse_module_stream(&mut stream).unwrap();
```
