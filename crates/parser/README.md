# `tinywasm-parser`

This crate provides the parser and lowering pipeline that converts WebAssembly binaries into `tinywasm` modules.

## Features

- `std`: Enables the use of `std` and `std::io` for parsing from files and streams.
- `log`: Enables logging of the parsing process using the `log` crate.
- `parallel`: Enables multithreaded parsing and validation when `std` is available.

## Usage

```rust
use tinywasm_parser::{Parser, ParserOptions};

let bytes = include_bytes!("./file.wasm");

let parser = Parser::new();
let module = parser.parse_module_bytes(bytes)?;

let parser = Parser::with_options(ParserOptions::default().with_rewrite_optimization(false));
let module = parser.parse_module_bytes(bytes)?;

let module = parser.parse_module_file("path/to/file.wasm")?;
let mut stream = std::fs::File::open("path/to/file.wasm")?;
let module = parser.parse_module_stream(&mut stream)?;
```

If you just want the default configuration, the top-level `parse_bytes`, `parse_file`, and `parse_stream` helpers are thin wrappers around `Parser::new()`.
