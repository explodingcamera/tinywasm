> [!WARNING]
> This is the `next` branch and contains unreleased changes. See [`v0.10.0`](https://github.com/explodingcamera/tinywasm/tree/v0.10.0) for the latest released version.

# <b>`tinywasm`</b>

[![docs.rs](https://img.shields.io/docsrs/tinywasm?logo=rust&style=flat-square)](https://docs.rs/tinywasm) [![Crates.io](https://img.shields.io/crates/v/tinywasm.svg?logo=rust&style=flat-square)](https://crates.io/crates/tinywasm) [![Crates.io](https://img.shields.io/crates/l/tinywasm.svg?style=flat-square)](./LICENSE-APACHE)

## Why `tinywasm`?

- **Tiny**: Small by design, while still passing the full WebAssembly 3.0 core testsuite.
- **Portable**: Runs anywhere Rust can target, supports `no_std`, has minimal dependencies, and can itself compile to WebAssembly.
- **Safe**: Written in safe Rust, with optional `unsafe` limited to the `simd-x86` feature. Its sandbox is designed to prevent untrusted Wasm from accessing host memory or escaping the runtime.

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

## Precompiled Modules

TinyWasm modules can be compiled to the internal `twasm` bytecode format, which stores the optimized instruction representation for faster loading and reuse.

## Cargo Features

- **`std`:** Enables `std` and parsing from files and streams. Enabled by default.
- **`log`:** Enables integration with the `log` crate. Enabled by default.
- **`parser`:** Enables `tinywasm-parser` and top-level parse helpers. Enabled by default.
- **`validate`:** Enables WebAssembly validation while parsing. Enabled by default and configurable through `ParserOptions`.
- **`archive`:** Enables serialization and deserialization of the internal `twasm` format. Enabled by default.
- **`canonicalize-nans`:** Canonicalizes NaN values. Enabled by default.
- **`debug`:** Derives `Debug` for runtime types. Enabled by default.
- **`parallel-parser`:** Parallelizes function parsing when `std` is enabled. Enabled by default.
- **`guest-debug`:** Exposes module-internal by-index inspection APIs (`*_by_index`).
- **`simd-x86`:** Enables x86-specific SIMD intrinsics and uses `unsafe` internally.

With default features disabled, `tinywasm` depends only on `core`, `alloc`, and `libm`[^libm], making it usable in `no_std + alloc` environments.

Use `Engine` and `engine::Config` when you need non-default runtime settings such as fuel accounting, stack sizing, or the GC collection threshold. A `ResourceLimiter` attached to the engine's config bounds guest memory and table allocation and growth and can trap rejected requests.

[^libm]: [rust-lang/rust#137578](https://github.com/rust-lang/rust/issues/137578) — tracking issue for floating-point math support in `no_std`.

## Safety

TinyWasm only uses safe Rust by default. The optional `simd-x86` feature enables x86-specific SIMD intrinsics and uses `unsafe` internally. WebAssembly input is validated by default through the `validate` feature. Disabling validation should not let Wasm access host memory or escape the sandbox, but malformed input may panic or otherwise crash the process, so only disable it for trusted input.

The internal `twasm` bytecode format is not currently validated as an untrusted input format. Malformed `twasm` may panic, but should not compromise memory safety or allow sandbox escape. Only run trusted `twasm` bytecode, or generate it through TinyWasm from Wasm input.

## Supported Proposals

TinyWasm targets non-JavaScript core proposals through [phase 3](https://github.com/WebAssembly/proposals). JavaScript integrations and optional embedding or tooling APIs are not included here.

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
| [**Stack Switching**](https://github.com/WebAssembly/stack-switching)                                            | 🌑     | -                  |
| [**Compact Import Section**](https://github.com/WebAssembly/compact-import-section)                              | 🌑     | -                  |
| [**Threads**](https://github.com/WebAssembly/threads)                                                            | 🌑     | -                  |

**Legend**\
🌑 -- not available\
🚧 -- in development/partially supported\
🟢 -- fully supported

## See Also

If you need a more mature, production-tested, or performance-focused WebAssembly runtime today, consider one of these projects:

- [wasmi](https://github.com/wasmi-labs/wasmi) - efficient and versatile WebAssembly interpreter for embedded systems
- [wasm3](https://github.com/wasm3/wasm3) - a fast WebAssembly interpreter written in C
- [wazero](https://wazero.io/) - a zero-dependency WebAssembly interpreter written in Go

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in `tinywasm` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
