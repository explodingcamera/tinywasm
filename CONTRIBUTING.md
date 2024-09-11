# Contributing

Thank you for considering contributing to this project! This document outlines the process for contributing to this project. For small changes or bug fixes, feel free to open a pull request directly. For larger changes, please open an issue first to discuss the proposed changes. Also, please ensure that you open up your pull request against the `next` branch and [allow maintainers of the project to edit your code](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/allowing-changes-to-a-pull-request-branch-created-from-a-fork).

## 1. Clone the Repository

Ensure you clone this repository with the `--recursive` flag to include the submodules:

```bash
git clone --recursive https://github.com/explodingcamera/tinywasm.git
```

If you have already cloned the repository, you can initialize the submodules with:

```bash
git submodule update --init --recursive
```

This is required to run the WebAssembly test suite.

## 2. Set up the Development Environment

This project mostly uses a pretty standard Rust setup. Some common tasks:

```bash
# Run a specific benchmark (run without arguments to see available benchmarks)
$ cargo bench --bench {bench_name}

# Run all tests
$ cargo test

# Run only the WebAssembly MVP (1.0) test suite
$ cargo test-wasm-1

# Run only the full WebAssembly test suite (2.0)
$ cargo test-wasm-2

# Run a specific test (run without arguments to see available tests)
$ cargo test --test {test_name}

# Run a single WAST test file
$ cargo test-wast {path}

# Run a specific example (run without arguments to see available examples)
#   The wasm test files required to run the `wasm-rust` examples are not
#   included in the main repository.
#   To build these, you will need to install binaryen and wabt
#   and run `./examples/rust/build.sh`.
$ cargo run --example {example_name}
```

### Profiling

Either [samply](https://github.com/mstange/samply/) or [cargo-flamegraph](https://github.com/flamegraph-rs/flamegraph) are recommended for profiling.

Example usage:

```bash
cargo install --locked samply
cargo samply --example wasm-rust -- selfhosted
```

# Commits

This project uses [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) for commit messages. For pull requests, the commit messages will be squashed so you don't need to worry about this too much. However, it is still recommended to follow this convention for consistency.

# Branches

- `main`: The main branch. This branch is used for the latest stable release.
- `next`: The next branch. Development happens here.
