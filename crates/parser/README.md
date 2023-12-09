# `tinywasm_parser`

This crate provides a parser that can parse WebAssembly modules into a TinyWasm module. It is based on 
[`wasmparser_nostd`](https://crates.io/crates/wasmparser_nostd) and used by [`tinywasm`](https://crates.io/crates/tinywasm).

## Features

- `std`: Enables the use of `std` and `std::io` for parsing from files and streams.
- `logging`: Enables logging of the parsing process using the `log` crate.

## Usage

```rust
use tinywasm_parser::{Parser, TinyWasmModule};
let bytes = include_bytes!("./file.wasm");

let parser = Parser::new();
let module: TinyWasmModule = parser.parse_module_bytes(bytes).unwrap();
let mudule: TinyWasmModule = parser.parse_module_file("path/to/file.wasm").unwrap(); 
let module: TinyWasmModule = parser.parse_module_stream(&mut stream).unwrap();
```
