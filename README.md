<div>
    <div>
        <img align="left" src="./tinywasm.png" width="100px">
    </div>
    <h1>TinyWasm</h1>
    A tiny WebAssembly Runtime written in Rust
</div>

<br>

[![docs.rs](https://img.shields.io/docsrs/okv?logo=rust)](https://docs.rs/okv) [![Crates.io](https://img.shields.io/crates/v/okv.svg?logo=rust)](https://crates.io/crates/okv) [![Crates.io](https://img.shields.io/crates/l/okv.svg)](./LICENSE-APACHE) 

<br/>

> [!WARNING]  
> This project is still in development and is not ready for use.

# 🎯 Goals

* Interpreted Runtime (no JIT)
* Self-hosted (can run itself compiled to WebAssembly)
* No unsafe code
* Works on `no_std` (with `alloc` the feature and nightly compiler)
* Fully support WebAssembly MVP (1.0)
* Low Memory Usage (less than 10kb)
* Fast Startup Time
* Preemptive multitasking support


# 📄 License

Licensed under either of [Apache License, Version 2.0](./LICENSE-APACHE) or [MIT license](./LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in TinyWasm by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions. 