> [!NOTE]
> TinyWasm is currently on pause and not actively developed. _Eventually_ this may change but for now it is archived (as of October 2025).

<div>
    <div>
        <a href=""><img align="left" src="https://raw.githubusercontent.com/explodingcamera/tinywasm/main/tinywasm.png" width="100px"></a>
    </div>
    <h1>TinyWasm</h1>
    A tiny WebAssembly Runtime written in safe Rust
</div>

<br>

[![docs.rs](https://img.shields.io/docsrs/tinywasm?logo=rust)](https://docs.rs/tinywasm) [![Crates.io](https://img.shields.io/crates/v/tinywasm.svg?logo=rust)](https://crates.io/crates/tinywasm) [![Crates.io](https://img.shields.io/crates/l/tinywasm.svg)](./LICENSE-APACHE)

## Why TinyWasm?

- **Tiny**: TinyWasm is designed to be as small as possible without significantly compromising performance or functionality (< 4000 LLOC).
- **Portable**: TinyWasm runs on any platform that Rust can target, including `no_std`, with minimal external dependencies.
- **Safe**: No unsafe code is used in the runtime

## Current Status

TinyWasm passes all WebAssembly MVP tests from the [WebAssembly core testsuite](https://github.com/WebAssembly/testsuite) and is able to run most WebAssembly programs. Additionally, the current 2.0 WebAssembly is mostly supported, with the exception of the SIMD and Memory64 proposals. See the [Supported Proposals](#supported-proposals) section for more information.

## Safety

Safety wise, TinyWasm doesn't use any unsafe code and is designed to be completely memory-safe. Untrusted WebAssembly code should not be able to crash the runtime or access memory outside of its sandbox, however currently there is no protection against infinite loops or excessive memory usage. Unvalidated Wasm and untrusted, precompilled twasm bytecode is safe to run too but can crash the runtime.

## Supported Proposals

**Legend**\
🌑 -- not available\
🚧 -- in development/partially supported\
🟢 -- fully supported

| Proposal                                                                                                                    | Status | TinyWasm Version |
| --------------------------------------------------------------------------------------------------------------------------- | ------ | ---------------- |
| [**Mutable Globals**](https://github.com/WebAssembly/mutable-global/blob/master/proposals/mutable-global/Overview.md)       | 🟢     | 0.2.0            |
| [**Non-trapping float-to-int Conversion**](https://github.com/WebAssembly/nontrapping-float-to-int-conversions)             | 🟢     | 0.2.0            |
| [**Sign-extension operators**](https://github.com/WebAssembly/sign-extension-ops)                                           | 🟢     | 0.2.0            |
| [**Multi-value**](https://github.com/WebAssembly/spec/blob/master/proposals/multi-value/Overview.md)                        | 🟢     | 0.2.0            |
| [**Bulk Memory Operations**](https://github.com/WebAssembly/spec/blob/master/proposals/bulk-memory-operations/Overview.md)  | 🟢     | 0.4.0            |
| [**Reference Types**](https://github.com/WebAssembly/reference-types/blob/master/proposals/reference-types/Overview.md)     | 🟢     | 0.7.0            |
| [**Multiple Memories**](https://github.com/WebAssembly/multi-memory/blob/master/proposals/multi-memory/Overview.md)         | 🟢     | 0.8.0            |
| [**Custom Page Sizes**](https://github.com/WebAssembly/custom-page-sizes/blob/main/proposals/custom-page-sizes/Overview.md) | 🟢     | `next`           |
| [**Tail Call**](https://github.com/WebAssembly/tail-call/blob/main/proposals/tail-call/Overview.md)                         | 🟢     | `next`           |
| [**Memory64**](https://github.com/WebAssembly/memory64/blob/master/proposals/memory64/Overview.md)                          | 🟢     | `next`           |
| [**Fixed-Width SIMD**](https://github.com/webassembly/simd)                                                                 | 🚧     | N/A              |

## Usage

See the [examples](./examples) directory and [documentation](https://docs.rs/tinywasm) for more information on how to use TinyWasm.
For testing purposes, you can also use the `tinywasm-cli` tool:

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

With all these features disabled, TinyWasm only depends on `core`, `alloc`, and `libm` and can be used in `no_std` environments. Since `libm` is not as performant as the compiler's math intrinsics, it is recommended to use the `std` feature if possible (at least [for now](https://github.com/rust-lang/rfcs/issues/2505)), especially on `wasm32` targets.

## Inspiration

Big thanks to the authors of the following projects, which have inspired and influenced TinyWasm:

- [wasmi](https://github.com/wasmi-labs/wasmi) - an efficient and lightweight WebAssembly interpreter that also runs in `no_std` environments
- [wasm3](https://github.com/wasm3/wasm3) - a high-performance WebAssembly interpreter written in C
- [wazero](https://wazero.io/) - a zero-dependency WebAssembly interpreter written in Go
- [wain](https://github.com/rhysd/wain) - a zero-dependency WebAssembly interpreter written in Rust

I encourage you to check these projects out if you're looking for more mature and feature-complete WebAssembly runtimes.

## License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in TinyWasm by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
