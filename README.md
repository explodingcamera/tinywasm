> [!WARNING]
> This is the `next` branch and contains unreleased changes. See [`v0.10.0`](https://github.com/explodingcamera/tinywasm/tree/v0.10.0) for the latest released version.

# <b>`tinywasm`</b>

[![Documentation](https://img.shields.io/badge/docs-latest-blue?style=flat-square)](https://docs.rs/tinywasm/latest/tinywasm/) [![Build](https://img.shields.io/github/actions/workflow/status/explodingcamera/tinywasm/test.yaml?branch=next&style=flat-square&label=build)](https://github.com/explodingcamera/tinywasm/actions/workflows/test.yaml?query=branch%3Anext) [![Crates.io](https://img.shields.io/crates/v/tinywasm.svg?logo=rust&style=flat-square)](https://crates.io/crates/tinywasm) [![Crates.io](https://img.shields.io/crates/l/tinywasm.svg?style=flat-square)](./LICENSE-APACHE)

## Why `tinywasm`?

- **Tiny**: Small by design, while still passing the full WebAssembly 3.0 core test suite.
- **Portable**: Runs anywhere Rust can target, supports `no_std`, has minimal dependencies[^dependencies], and can itself compile to WebAssembly.
- **Safe by default**: Written entirely in safe Rust[^unsafe].

[^dependencies]: The two main external components are [`wasmparser`](https://crates.io/crates/wasmparser) for WebAssembly parsing and validation, and [`postcard`](https://crates.io/crates/postcard) for `.twasm` archives.

[^unsafe]: The optional `simd-x86` feature is the only exception. It uses `unsafe` internally for selected x86 SIMD intrinsics.

## Installation

```toml
[dependencies]
tinywasm = "0.10"
```

## Usage

```rust
use tinywasm::{ModuleInstance, Store};

// Load a module from bytes
let wasm = include_bytes!("../examples/wasm/add.wasm");
let module = tinywasm::parse_bytes(wasm)?;

// Create a new store
let mut store = Store::default();

// Instantiate the module
let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

// Call an exported function with typed parameters
let func = instance.func::<(i32, i32), i32>(&store, "add")?;
let result = func.call(&mut store, (1, 2))?;

assert_eq!(result, 3);
```

See the [examples](./examples) directory and [documentation](https://docs.rs/tinywasm) for more information.

## Cargo Features

- **`std`:** Enables `std` and parsing from files and streams. Enabled by default.
- **`log`:** Enables integration with the `log` crate. Enabled by default.
- **`parser`:** Enables `tinywasm-parser` and top-level parse helpers. Enabled by default.
- **`validate`:** Enables WebAssembly validation while parsing. Enabled by default and configurable through [`ParserOptions`](https://docs.rs/tinywasm/latest/tinywasm/parser/struct.ParserOptions.html).
- **`archive`:** Enables serialization and deserialization of the internal `twasm` format. Enabled by default.
- **`canonicalize-nans`:** Uses a [canonical NaN](https://en.wikipedia.org/wiki/NaN#Canonical_NaN) for normalized NaN results. Enabled by default.
- **`debug`:** Derives `Debug` for runtime types. Enabled by default.
- **`parallel-parser`:** Parallelizes function parsing when `std` is enabled. Enabled by default.
- **`guest-debug`:** Exposes module-internal by-index inspection APIs (`*_by_index`).
- **`send`:** Makes stores and store-local handles movable across threads
- **`simd-x86`:** Enables x86-specific SIMD intrinsics and uses `unsafe` internally.

With default features disabled, `tinywasm` supports `no_std + alloc` and depends only on `libm`.

Use [`Engine`](https://docs.rs/tinywasm/latest/tinywasm/engine/struct.Engine.html) and [`engine::Config`](https://docs.rs/tinywasm/latest/tinywasm/engine/struct.Config.html) for non-default fuel accounting, stack sizing, or GC collection thresholds. A configured `ResourceLimiter` can allow, reject, or trap memory and table allocation or growth requests, and GC object allocations.

## Precompiled Modules

TinyWasm can serialize a parsed module to its version-specific `.twasm` format. Loading an archive skips WebAssembly parsing, validation, and optimization.

Applications that only load `.twasm` can remove the parser and validator from their binary by only enabling the `archive` feature. Depending on the target and release profile, this can produce binaries smaller than 300 KB.

## Untrusted Input

WebAssembly [validation](https://webassembly.github.io/spec/core/valid/index.html) is enabled by default through the `validate` feature. Keep this feature enabled and leave `ParserOptions::validation` enabled for modules from untrusted sources. Without validation, parsing can produce modules that violate runtime assumptions and may panic during instantiation or execution.

Validation does not limit parsing or execution resources. Hosts that run untrusted code should also set input limits, configure stack and `ResourceLimiter` limits, and use fuel- or time-budgeted execution as needed.

Loading `.twasm` checks the archive header and encoding but does not run WebAssembly validation or verify TinyWasm's runtime invariants. Load archives only from trusted sources. For untrusted input, parse a WebAssembly binary with validation enabled.

## WebAssembly Proposal Support

TinyWasm generally implements non-JavaScript core proposals at [phase 4](https://github.com/WebAssembly/proposals#phase-4---standardize-the-feature-wg) or later, with some proposals implemented earlier. The table shows current support and known exceptions.

| Proposal                                                                                                         | Status | `tinywasm` Version |
| ---------------------------------------------------------------------------------------------------------------- | ------ | ------------------ |
| [**Import/Export of Mutable Globals**](https://github.com/WebAssembly/mutable-global)                            | 🟢     | 0.2.0              |
| [**Multi-value**](https://github.com/WebAssembly/multi-value)                                                    | 🟢     | 0.2.0              |
| [**Non-trapping Float-to-int Conversions**](https://github.com/WebAssembly/nontrapping-float-to-int-conversions) | 🟢     | 0.2.0              |
| [**Sign-extension Operators**](https://github.com/WebAssembly/sign-extension-ops)                                | 🟢     | 0.2.0              |
| [**Bulk Memory Operations**](https://github.com/WebAssembly/bulk-memory-operations)                              | 🟢     | 0.4.0              |
| [**Reference Types**](https://github.com/WebAssembly/reference-types)                                            | 🟢     | 0.7.0              |
| [**Fixed-width SIMD**](https://github.com/WebAssembly/simd)                                                      | 🟢     | 0.9.0              |
| [**Tail Calls**](https://github.com/WebAssembly/tail-call)                                                       | 🟢     | 0.9.0              |
| [**Extended Constant Expressions**](https://github.com/WebAssembly/extended-const)                               | 🟢     | 0.9.0              |
| [**Multiple Memories**](https://github.com/WebAssembly/multi-memory)                                             | 🟢     | 0.8.0              |
| [**Relaxed SIMD**](https://github.com/WebAssembly/relaxed-simd)                                                  | 🟢     | 0.9.0              |
| [**Custom Annotation Syntax**](https://github.com/WebAssembly/annotations)                                       | 🟢     | 0.8.0              |
| [**Memory64**](https://github.com/WebAssembly/memory64)                                                          | 🟢     | 0.9.0              |
| [**Wide Arithmetic**](https://github.com/WebAssembly/wide-arithmetic)                                            | 🟢     | 0.9.0              |
| [**Custom Page Sizes**](https://github.com/WebAssembly/custom-page-sizes)                                        | 🟢     | 0.9.0              |
| [**Typed Function References**](https://github.com/WebAssembly/function-references)                              | 🟢     | `next`             |
| [**Garbage Collection**](https://github.com/WebAssembly/gc)                                                      | 🟢     | `next`             |
| [**Exception Handling**](https://github.com/WebAssembly/exception-handling)                                      | 🟢     | `next`             |
| [**Compact Import Section**](https://github.com/WebAssembly/compact-import-section)                              | 🟢     | `next`             |
| [**Stack Switching**](https://github.com/WebAssembly/stack-switching)                                            | 🌑     | -                  |
| [**Threads**](https://github.com/WebAssembly/threads)                                                            | 🌑     | -                  |

**Legend**\
🌑 -- not available\
🚧 -- in development/partially supported\
🟢 -- fully supported

## See Also

If you're looking for a WebAssembly runtime with JIT compilation, better performance or other advanced features, check out these other runtimes:

- [wasmi](https://github.com/wasmi-labs/wasmi) - efficient and versatile WebAssembly interpreter for embedded systems
- [wasm3](https://github.com/wasm3/wasm3) - a fast WebAssembly interpreter written in C
- [wazero](https://wazero.io/) - a zero-dependency WebAssembly interpreter written in Go
- [wasmer](https://wasmer.io/) - a fast and secure WebAssembly runtime written in Rust
- [wasmtime](https://wasmtime.dev/) - a fast and secure WebAssembly runtime written in Rust

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in `tinywasm` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
