# `tinywasm-parser`

This crate provides the parser and lowering pipeline that converts WebAssembly binaries into `tinywasm` modules.

## Features

- `std`: Enables the use of `std` and `std::io` for parsing from files and streams.
- `log`: Enables logging of the parsing process using the `log` crate.
- `parallel`: Enables multithreaded function parsing. Requires `std`.
- `validate`: Enables `wasmparser` validation. Enabled by default and configurable through `ParserOptions`.

## Usage

```rust
use tinywasm_parser::{ParseLimits, Parser, ParserOptions};

let bytes = include_bytes!("./file.wasm");

let parser = Parser::default();
let module = parser.parse_module_bytes(bytes)?;

let parser = Parser::new(ParserOptions::default().with_optimize(false));
let module = parser.parse_module_bytes(bytes)?;

// Select explicit bounds before accepting untrusted input. Keep validation on.
let limits = ParseLimits::new()
    .with_max_module_bytes(16 * 1024 * 1024)
    .with_max_section_items(100_000)
    .with_max_function_locals(10_000)
    .with_max_br_table_targets(4_096)
    .with_max_array_new_fixed_elements(4_096);
let parser = Parser::new(ParserOptions::default().with_limits(limits));
let module = parser.parse_module_bytes(bytes)?;

let module = parser.parse_module_file("path/to/file.wasm")?;
let mut stream = std::fs::File::open("path/to/file.wasm")?;
let module = parser.parse_module_stream(&mut stream)?;
```

If you just want the default configuration, the top-level `parse_bytes`, `parse_file`, and `parse_stream` helpers are thin wrappers around `Parser::default()`.

The limits are opt-in and disabled by default. They bound encoded input and
known parse-time amplification points; they are not a hard cap on peak memory
or elapsed time. A host should also bound guest runtime resources after parsing.
