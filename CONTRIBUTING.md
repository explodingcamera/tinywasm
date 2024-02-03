# Scripts and Commands

> To improve the development experience, a number of custom commands and aliases have been added to the `.cargo/config.toml` file. These can be run using `cargo <command>`.

- **`cargo dev [args]`**\
  e.g. `cargo dev -f check ./examples/wasm/call.wat -a i32:0`\
  Run the development version of the tinywasm-cli. This is the main command used for developing new features.\
  See [tinywasm-cli](./crates/cli) for more information.

- **`cargo generate-charts`**\
  Generate test result charts from the previous test runs. This is used to generate the charts in the [README](./README.md).

- **`cargo test-mvp`**\
  Run the WebAssembly MVP (1.0) test suite. Be sure to cloned this repo with `--recursive` or initialize the submodules with `git submodule update --init --recursive`

- **`cargo test-2`**\
  Run the full WebAssembly test suite (2.0)

- **`cargo benchmark <benchmark>`**\
  Run a single benchmark. e.g. `cargo benchmark argon2id`

- **`cargo test-wast <path>`**\
  Run a single WAST test file. e.g. `cargo test-wast ./examples/wast/i32.wast`

- **`cargo version-dev`**\
  Bump the version to the next dev version. This should be used after a release so test results are not overwritten. Does not create a new github release.

## Workspace Commands

> These commands require the [cargo-workspaces](https://crates.io/crates/cargo-workspaces) crate to be installed.

- **`cargo workspaces version`**\
  Bump the version of all crates in the workspace and push changes to git. This is used for releasing new versions on github.

- **`cargo workspaces publish --publish-as-is`**\
  Publish all crates in the workspace to crates.io. This should be used a new version has been released on github. After publishing, the version should be bumped to the next dev version.
