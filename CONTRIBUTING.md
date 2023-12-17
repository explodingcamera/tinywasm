# Common Commands

> To improve the development experience, a number of aliases have been added to the `.cargo/config.toml` file. These can be run using `cargo <command>`.

- **`cargo dev`**\
  e.g. `cargo dev -f check ./examples/wasm/call.wat -a i32:0`\
  Run the development version of the tinywasm-cli. This is the main command used for developing new features.

- **`cargo generate-charts`**\
  Generate test result charts

- **`cargo test-mvp`**\
  Run the WebAssembly MVP (1.0) test suite

- **`cargo version-dev`**\
  Bump the version to the next dev version. This should be used after a release so test results are not overwritten. Does not create a new github release.

## Workspace Commands

> These commands require the [cargo-workspaces](https://crates.io/crates/cargo-workspaces) crate to be installed.

- **`cargo workspaces version`**\
  Bump the version of all crates in the workspace and push changes to git. This is used for releasing new versions on github.

- **`cargo workspaces publish --from-git`**\
  Publish all crates in the workspace to crates.io. This should be used a new version has been released on github. After publishing, the version should be bumped to the next dev version.
