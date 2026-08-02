> [!WARNING]
> This is the `next` branch and contains unreleased changes. See [`v0.10.0`](https://github.com/explodingcamera/tinywasm/tree/v0.10.0) for the latest released version.

# <b>`tinywasm`</b>

[![docs.rs](https://img.shields.io/docsrs/tinywasm?logo=rust&style=flat-square)](https://docs.rs/tinywasm) [![Crates.io](https://img.shields.io/crates/v/tinywasm.svg?logo=rust&style=flat-square)](https://crates.io/crates/tinywasm) [![Crates.io](https://img.shields.io/crates/l/tinywasm.svg?style=flat-square)](./LICENSE-APACHE)

## Why `tinywasm`?

- **Tiny**: Small by design, without significantly compromising performance or functionality.
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

Use `Engine` and `engine::Config` when you need non-default runtime settings such as fuel accounting, stack sizing, memory backend selection, or trap-on-OOM behavior.

[^libm]: [rust-lang/rust#137578](https://github.com/rust-lang/rust/issues/137578) — tracking issue for floating-point math support in `no_std`.

## Current Status

`tinywasm` passes the WebAssembly MVP and WebAssembly 2.0 core testsuites and supports the [Lime1](https://github.com/WebAssembly/tool-conventions/blob/main/Lime.md#lime1) interoperability target. WebAssembly 3.0 support is still in progress, and some newer proposal suites are tracked in-repo as experimental coverage rather than release guarantees; see [Supported Proposals](#supported-proposals) for details.

TinyWasm also has its own internal bytecode format, `twasm`. WebAssembly modules can be compiled to `twasm`, which stores TinyWasm's optimized instruction representation for faster loading and reuse.

## Safety

TinyWasm only uses safe Rust by default. The optional `simd-x86` feature enables x86-specific SIMD intrinsics and uses `unsafe` internally. WebAssembly input is validated by default through the `validate` feature. Disabling validation should not let Wasm access host memory or escape the sandbox, but malformed input may panic or otherwise crash the process, so only disable it for trusted input.

The internal `twasm` bytecode format is not currently validated as an untrusted input format. Malformed `twasm` may panic, but should not compromise memory safety or allow sandbox escape. Only run trusted `twasm` bytecode, or generate it through TinyWasm from Wasm input.

## Supported Proposals

| Proposal                                                                                                                                | Status | `tinywasm` Version |
| --------------------------------------------------------------------------------------------------------------------------------------- | ------ | ------------------ |
| [**Multi-value**](https://github.com/WebAssembly/spec/blob/master/proposals/multi-value/Overview.md)                                    | 🟢     | 0.2.0              |
| [**Mutable Globals**](https://github.com/WebAssembly/mutable-global/blob/master/proposals/mutable-global/Overview.md)                   | 🟢     | 0.2.0              |
| [**Non-trapping float-to-int Conversion**](https://github.com/WebAssembly/nontrapping-float-to-int-conversions)                         | 🟢     | 0.2.0              |
| [**Sign-extension operators**](https://github.com/WebAssembly/sign-extension-ops)                                                       | 🟢     | 0.2.0              |
| [**Bulk Memory Operations**](https://github.com/WebAssembly/spec/blob/master/proposals/bulk-memory-operations/Overview.md)              | 🟢     | 0.4.0              |
| [**Reference Types**](https://github.com/WebAssembly/reference-types/blob/master/proposals/reference-types/Overview.md)                 | 🟢     | 0.7.0              |
| [**Multi-memory**](https://github.com/WebAssembly/multi-memory/blob/master/proposals/multi-memory/Overview.md)                          | 🟢     | 0.8.0              |
| [**Custom Page Sizes**](https://github.com/WebAssembly/custom-page-sizes/blob/main/proposals/custom-page-sizes/Overview.md)             | 🟢     | 0.9.0              |
| [**Extended Const**](https://github.com/WebAssembly/extended-const/blob/main/proposals/extended-const/Overview.md)                      | 🟢     | 0.9.0              |
| [**Fixed-Width SIMD**](https://github.com/WebAssembly/simd/blob/main/proposals/simd/Overview.md)                                        | 🟢     | 0.9.0              |
| [**Memory64**](https://github.com/WebAssembly/memory64/blob/master/proposals/memory64/Overview.md)                                      | 🟢     | 0.9.0              |
| [**Tail Call**](https://github.com/WebAssembly/tail-call/blob/main/proposals/tail-call/Overview.md)                                     | 🟢     | 0.9.0              |
| [**Relaxed SIMD**](https://github.com/WebAssembly/relaxed-simd/blob/main/proposals/relaxed-simd/Overview.md)                            | 🟢     | 0.9.0              |
| [**Wide Arithmetic**](https://github.com/WebAssembly/wide-arithmetic/blob/main/proposals/wide-arithmetic/Overview.md)                   | 🟢     | 0.9.0              |
| [**Typed Function References**](https://github.com/WebAssembly/function-references/blob/main/proposals/function-references/Overview.md) | 🚧     | `next`             |
| [**Exception Handling**](https://github.com/WebAssembly/exception-handling/blob/main/proposals/exception-handling/Exceptions.md)        | 🌑     | -                  |
| [**Garbage Collection**](https://github.com/WebAssembly/gc/blob/main/proposals/gc/Overview.md)                                          | 🌑     | -                  |
| [**Stack Switching**](https://github.com/WebAssembly/stack-switching/blob/main/proposals/stack-switching/Explainer.md)                  | 🌑     | -                  |
| [**Threads**](https://github.com/WebAssembly/threads/blob/main-legacy/proposals/threads/Overview.md)                                    | 🌑     | -                  |

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
