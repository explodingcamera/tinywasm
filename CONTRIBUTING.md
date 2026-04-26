# Contributing

Thank you for considering a contribution. For small fixes, feel free to open a pull request directly. For larger changes, please open an issue first so we can discuss the approach. Please target the `next` branch and [allow maintainers to edit your PR branch](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/allowing-changes-to-a-pull-request-branch-created-from-a-fork).

## Code of Conduct

This project follows the [Contributor Covenant 3.0 Code of Conduct](https://www.contributor-covenant.org/version/3/0/code_of_conduct/).

## Development

This project mostly uses a pretty standard Rust setup. Some common tasks:

```bash
# Run a specific benchmark (run without arguments to see available benchmarks)
$ cargo bench --bench {bench_name}

# Run all tests
$ cargo test

# Run only the WebAssembly MVP (1.0) test suite
$ cargo test-wasm-1

# Run only the full WebAssembly 2.0 test suite
$ cargo test-wasm-2

# Run only the full WebAssembly 3.0 test suite
$ cargo test-wasm-3

# Run a single WAST test file
$ cargo test-wast ./wasm-testsuite/data/wasm-v1/{file}.wast

# Run custom wasm tests from crates/tinywasm/tests/wasm-custom
$ cargo test-wasm-custom

# Run a specific example (run without arguments to see available examples)
#   The wasm test files required to run the `wasm-rust` examples are not
#   included in the main repository.
#   To build these, you will need to install binaryen and wabt
#   and run `./examples/rust/build.sh`.
$ cargo run --example {example_name}
```

### Profiling

Use [samply](https://github.com/mstange/samply/) for profiling.

Example usage:

```bash
cargo install --locked samply
samply record -- cargo run --release --example wasm-rust -- tinywasm
```

## Commits

This project uses [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) for commit messages. For pull requests, the commit messages will be squashed so you don't need to worry about this too much. However, it is still recommended to follow this convention for consistency.

## Branches

- `main`: The main branch. This branch is used for the latest stable release.
- `next`: The next branch. Development happens here.
