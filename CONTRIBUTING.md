# Scripts and Commands

> To improve the development experience, a number of custom commands and aliases have been added to the `.cargo/config.toml` file. These can be run using `cargo <command>`.

- **`cargo test-wasm-1`**\
  Run the WebAssembly MVP (1.0) test suite. Be sure to cloned this repo with `--recursive` or initialize the submodules with `git submodule update --init --recursive`

- **`cargo test-wasm-2`**\
  Run the full WebAssembly test suite (2.0)

- **`cargo test-wast <path>`**\
  Run a single WAST test file. e.g. `cargo test-wast ./examples/wast/i32.wast`. Useful for debugging failing test-cases.
