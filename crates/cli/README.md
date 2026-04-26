# `tinywasm-cli`

The `tinywasm-cli` package installs the `tinywasm` binary for `tinywasm`. See [`tinywasm`](https://crates.io/crates/tinywasm) for the embedding API.
It is recommended to use the library directly instead of the CLI.

The crate also exposes reusable helpers such as `tinywasm_cli::wast_runner::WastRunner` so workspace tests can drive the same WAST execution logic directly.

## Usage

```bash
$ cargo install tinywasm-cli --version 0.9.0-alpha.0 --bin tinywasm
$ tinywasm --help
$ tinywasm ./module.wasm
$ tinywasm run --invoke add ./module.wasm 1 2
$ tinywasm compile ./module.wat -o ./module.twasm
$ tinywasm dump ./module.twasm
$ tinywasm inspect ./module.wasm
$ tinywasm wast ./spec-tests/address.wast
```

Notes:

- `run`, `dump`, and `inspect` accept `.wasm`, `.wat`, and `.twasm` inputs.
- Use `-` as the input path to read a module from stdin.
- Without `--invoke`, `tinywasm` expects the module to have a start function or `_start` export.
- `compile` writes TinyWasm's `twasm` archive format.
- Function invocation arguments are parsed from the export signature, so `tinywasm run --invoke add ./module.wasm 1 2` works without repeating Wasm types on the command line.
- `inspect` uses ANSI colors automatically when writing to a terminal; set `NO_COLOR=1` to disable them.
- Stack flags support both fixed sizes like `--value-stack-size 4096` and dynamic sizes like `--value-stack-dynamic 1024:8192`.
- `wast` is a separate command for WebAssembly spec scripts and accepts files or folders containing `.wast` files.
