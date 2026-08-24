# Contributing

Thank you for contributing to TinyWasm. For small fixes, you can open a pull request directly. For larger changes, including new features and public API changes, open an issue first to discuss the approach.

Pull requests should target the `next` branch. If you submit a pull request from a fork, [allow maintainers to edit the branch](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/allowing-changes-to-a-pull-request-branch-created-from-a-fork).

All contributors must follow the [Code of Conduct](./CODE_OF_CONDUCT.md).

## AI-assisted contributions

You may use AI tools to help develop a contribution, but:

- Write issue and pull request descriptions yourself. Do not submit AI-generated issue or pull request text.
- Understand and review all code that you submit. You are responsible for its behavior and quality.
- Keep the change focused on the issue. Do not include unrelated generated changes or refactors.

## Development

Common commands:

```bash
cargo fmt --all -- --check
cargo clippy --workspace
cargo test --workspace
cargo run --example basic
cargo bench --bench tinywasm
```

WebAssembly test commands:

```bash
cargo test-wasm-1
cargo test-wasm-2
cargo test-wasm-3
cargo test-wasm-custom
cargo test-wast crates/tinywasm/tests/wasm-custom/table-basics.wast
```

Set `RUST_LOG=debug` when running `cargo test-wast` to see executor debug logs.

The `rust` example requires the `wasm32-unknown-unknown` target, the `rust-src` component, [Binaryen](https://github.com/WebAssembly/binaryen), and [WABT](https://github.com/WebAssembly/wabt):

```bash
./examples/rust/build.sh
cargo run --example rust -- hello
```

You can use [samply](https://github.com/mstange/samply/) for profiling:

```bash
cargo install --locked samply
samply record -- cargo run --profile samply --example rust -- tinywasm
```

Keep changes focused and external dependencies to a minimum. Update public documentation, the README, and the unreleased changelog when applicable.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) when practical. Pull requests are squash-merged, so the pull request title is more important than individual commit messages.

## Licensing

Unless you explicitly state otherwise, contributions are licensed under the repository's [MIT](./LICENSE-MIT) and [Apache-2.0](./LICENSE-APACHE) licenses.
