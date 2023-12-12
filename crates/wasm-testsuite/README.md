# `wasm-testsuite`

This crate embeds the latest version of the [WebAssembly Test Suite](https://github.com/WebAssembly/spec/tree/main/test). It is currently mainly used for testing the `tinywasm-parser` crate. Check out the [documentation](https://docs.rs/wasm-testsuite) for more information.

## Usage

```rust
use wasm_testsuite::{MVP_TESTS, get_test_wast};

MVP_TESTS.iter().for_each(|test| {
    let wast_bytes = get_test_wast(test).expect("Failed to get wast bytes");
    let wast = std::str::from_utf8(&wast_bytes).expect("failed to convert wast to utf8");

    // Do something with the wast (e.g. parse it using the `wast` crate)
});
```

## License

This crate is licensed under the [Apache License, Version 2.0](https://github.com/WebAssembly/spec/blob/main/test/LICENSE).