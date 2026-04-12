# `tinywasm-parser`

This crate provides a compiler that can convert WebAssembly modules into a `tinywasm` modules.

## Features

- `std`: Enables the use of `std` and `std::io` for parsing from files and streams.
- `log`: Enables logging of the parsing process using the `log` crate.

## Usage

```rust
use tinywasm_parser::Parser;
let bytes = include_bytes!("./file.wasm");

let parser = Parser::new();
let module = parser.parse_module_bytes(bytes).unwrap();
let module = parser.parse_module_file("path/to/file.wasm").unwrap();
let module = parser.parse_module_stream(&mut stream).unwrap();
```
