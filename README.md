<div>
    <div>
        <a href=""><img align="left" src="./tinywasm.png" width="100px"></a>
    </div>
    <h1>TinyWasm</h1>
    A tiny WebAssembly Runtime written in Rust
</div>

<br>

[![docs.rs](https://img.shields.io/docsrs/tinywasm?logo=rust)](https://docs.rs/tinywasm) [![Crates.io](https://img.shields.io/crates/v/tinywasm.svg?logo=rust)](https://crates.io/crates/tinywasm) [![Crates.io](https://img.shields.io/crates/l/tinywasm.svg)](./LICENSE-APACHE)

## Why TinyWasm?

- **Tiny**: TinyWasm is designed to be as small as possible without significantly compromising performance or functionality (< 6000 lines of code).
- **Portable**: TinyWasm runs on any platform that Rust can target, including WebAssembly itself, with minimal external dependencies.
- **Lightweight**: TinyWasm is easy to integrate and has a low call overhead, making it suitable for scripting and embedding.

## Status

As of version `0.3.0`, TinyWasm successfully passes all the WebAssembly 1.0 tests in the [WebAssembly Test Suite](https://github.com/WebAssembly/testsuite). Work on the 2.0 tests is ongoing. This enables TinyWasm to run most WebAssembly programs, including versions of TinyWasm itself compiled to WebAssembly (see [examples/wasm-rust.rs](./examples/wasm-rust.rs)). The results of the testsuites are available [here](https://github.com/explodingcamera/tinywasm/tree/main/crates/tinywasm/tests/generated).

The API is still unstable and may change at any time, so you probably don't want to use it in production _yet_. Note that TinyWasm isn't primarily designed for high performance; its focus lies more on simplicity, size, and portability. More details on its performance aspects can be found in [BENCHMARKS.md](./BENCHMARKS.md).

## Supported Proposals

| Proposal                                                                                                                   | Implementation Status | Version |
| -------------------------------------------------------------------------------------------------------------------------- | --------------------- | ------- |
| [**Mutable Globals**](https://github.com/WebAssembly/mutable-global/blob/master/proposals/mutable-global/Overview.md)      | Fully implemented     | 0.2.0   |
| [**Multi-value**](https://github.com/WebAssembly/spec/blob/master/proposals/multi-value/Overview.md)                       | Fully implemented     | 0.2.0   |
| [**Sign-extension operators**](https://github.com/WebAssembly/spec/blob/master/proposals/sign-extension-ops/Overview.md)   | Fully implemented     | 0.2.0   |
| [**Bulk Memory Operations**](https://github.com/WebAssembly/spec/blob/master/proposals/bulk-memory-operations/Overview.md) | Fully implemented     | 0.4.0   |
| [**Reference Types**](https://github.com/WebAssembly/reference-types/blob/master/proposals/reference-types/Overview.md)    | Partially implemented | N/A     |
| [**Multiple Memories**](https://github.com/WebAssembly/multi-memory/blob/master/proposals/multi-memory/Overview.md)        | Partially implemented | N/A     |
| [**Memory64**](https://github.com/WebAssembly/memory64/blob/master/proposals/memory64/Overview.md)                         | Partially implemented | N/A     |

## Usage

TinyWasm can be used through the `tinywasm-cli` CLI tool or as a library in your Rust project. Documentation can be found [here](https://docs.rs/tinywasm).

### Library

```sh
$ cargo add tinywasm
```

### CLI

The CLI is mainly available for testing purposes, but can also be used to run WebAssembly programs.

```sh
$ cargo install tinywasm-cli
$ tinywasm-cli --help
```

## Feature Flags

- **`std`**\
  Enables the use of `std` and `std::io` for parsing from files and streams. This is enabled by default.
- **`logging`**\
  Enables logging using the `log` crate. This is enabled by default.
- **`parser`**\
  Enables the `tinywasm-parser` crate. This is enabled by default.
- **`archive`**\
  Enables pre-parsing of archives. This is enabled by default.
- **`unsafe`**\
  Uses `unsafe` code to improve performance, particularly in Memory access

With all these features disabled, TinyWasm only depends on `core`, `alloc` and `libm` and can be used in `no_std` environments.
Since `libm` is not as performant as the compiler's math intrinsics, it is recommended to use the `std` feature if possible (at least [for now](https://github.com/rust-lang/rfcs/issues/2505)), especially on wasm32 targets.

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in TinyWasm by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

**Note:** The GitHub repository contains a Submodule (`crates/tinywasm-parser/data`) which is licensed only under the [Apache License, Version 2.0](https://github.com/WebAssembly/spec/blob/main/test/LICENSE). This data is generated from the [WebAssembly Specification](https://github.com/WebAssembly/spec/tree/main/test) and is only used for testing purposes and not included in the final binary.
