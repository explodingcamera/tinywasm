# <b>`tinywasm`</b>

[![docs.rs](https://img.shields.io/docsrs/tinywasm?logo=rust&style=flat-square)](https://docs.rs/tinywasm) [![Crates.io](https://img.shields.io/crates/v/tinywasm.svg?logo=rust&style=flat-square)](https://crates.io/crates/tinywasm) [![Crates.io](https://img.shields.io/crates/l/tinywasm.svg?style=flat-square)](./LICENSE-APACHE)

## Why `tinywasm`?

- **Tiny**: TinyWasm is designed to be as small as possible without significantly compromising performance or functionality
- **Portable**: Runs anywhere Rust can target, supports `no_std`, and keeps external dependencies to a minimum.
- **Safe**: Written entirely safe Rust and designed to prevent untrusted code from crashing the runtime

## Current Status

`tinywasm` passes 100% of WebAssembly MVP and WebAssembly 2.0 tests from the [WebAssembly core testsuite](https://github.com/WebAssembly/testsuite) and is able to run most WebAssembly programs. Additionally, support for WebAssembly 3.0 is on the way. See the [Supported Proposals](#supported-proposals) section for more information.

## Usage

See the [examples](./examples) directory and [documentation](https://docs.rs/tinywasm) for more information on how to use `tinywasm`.
For testing purposes, you can also use the `tinywasm-cli` tool:

```sh
$ cargo install tinywasm-cli
$ tinywasm-cli --help
```

## Feature Flags

- **`std`**\
  Enables the use of `std` and `std::io` for parsing from files and streams. This is enabled by default.
- **`log`**\
  Enables logging using the `log` crate. This is enabled by default.
- **`parser`**\
  Enables the `tinywasm-parser` crate. This is enabled by default.
- **`archive`**\
  Enables pre-parsing of archives. This is enabled by default.

With all these features disabled, `tinywasm` only depends on `core`, `alloc`, and `libm` and can be used in `no_std` environments. Since `libm` is not as performant as the compiler's math intrinsics, it is recommended to use the `std` feature if possible (at least [for now](https://github.com/rust-lang/rfcs/issues/2505)), especially on `wasm32` targets.

## Safety

Untrusted WebAssembly code should not be able to crash the runtime or access memory outside of its sandbox. Unvalidated Wasm and untrusted, precompiled twasm bytecode is safe to run as well, but can lead to panics if the bytecode is malformed. In general, it is recommended to validate Wasm bytecode before running it, and to only run trusted twasm bytecode.

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
| [**Annotations**](https://github.com/WebAssembly/annotations/blob/main/proposals/annotations/Overview.md)                               | 🟢     | `next`             |
| [**Custom Page Sizes**](https://github.com/WebAssembly/custom-page-sizes/blob/main/proposals/custom-page-sizes/Overview.md)             | 🟢     | `next`             |
| [**Extended Const**](https://github.com/WebAssembly/extended-const/blob/main/proposals/extended-const/Overview.md)                      | 🟢     | `next`             |
| [**Fixed-Width SIMD**](https://github.com/WebAssembly/simd/blob/main/proposals/simd/Overview.md)                                        | 🟢     | `next`             |
| [**Memory64**](https://github.com/WebAssembly/memory64/blob/master/proposals/memory64/Overview.md)                                      | 🟢     | `next`             |
| [**Tail Call**](https://github.com/WebAssembly/tail-call/blob/main/proposals/tail-call/Overview.md)                                     | 🟢     | `next`             |
| [**Relaxed SIMD**](https://github.com/WebAssembly/relaxed-simd/blob/main/proposals/relaxed-simd/Overview.md)                            | 🟢     | `next`             |
| [**Wide Arithmetic**](https://github.com/WebAssembly/wide-arithmetic/blob/main/proposals/wide-arithmetic/Overview.md)                   | 🟢     | `next`             |
| [**Branch Hinting**](https://github.com/WebAssembly/branch-hinting/blob/master/proposals/branch-hinting/Overview.md)                    | 🌑     | -                  |
| [**Custom Descriptors**](https://github.com/WebAssembly/custom-descriptors/blob/main/proposals/custom-descriptors/Overview.md)          | 🌑     | -                  |
| [**Exception Handling**](https://github.com/WebAssembly/exception-handling/blob/main/proposals/exception-handling/Exceptions.md)        | 🌑     | -                  |
| [**Typed Function References**](https://github.com/WebAssembly/function-references/blob/main/proposals/function-references/Overview.md) | 🌑     | -                  |
| [**Garbage Collection**](https://github.com/WebAssembly/gc/blob/main/proposals/gc/Overview.md)                                          | 🌑     | -                  |
| [**Stack Switching**](https://github.com/WebAssembly/stack-switching/blob/main/proposals/stack-switching/Explainer.md)                  | 🌑     | -                  |
| [**Threads**](https://github.com/WebAssembly/threads/blob/main-legacy/proposals/threads/Overview.md)                                    | 🌑     | -                  |

**Legend**\
🌑 -- not available\
🚧 -- in development/partially supported\
🟢 -- fully supported

## See Also

I encourage you to check these projects out if you're looking for more mature and feature-complete WebAssembly runtimes:

- [wasmi](https://github.com/wasmi-labs/wasmi) - efficient and versatile WebAssembly interpreter for embedded systems
- [wasm3](https://github.com/wasm3/wasm3) - a fast WebAssembly interpreter written in C
- [wazero](https://wazero.io/) - a zero-dependency WebAssembly interpreter written in Go
- [wain](https://github.com/rhysd/wain) - a zero-dependency WebAssembly interpreter written in Rust

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in `tinywasm` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
