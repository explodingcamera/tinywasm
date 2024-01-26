# Examples

## WasmRust

These are examples using WebAssembly generated from Rust code.
To run these, you first need to build the Rust code into WebAssembly, since the wasm files are not included in the repository to keep it small.
This requires the `wasm32-unknown-unknown` target and `wasm-opt` to be installed (available via Binaryen).

```bash
$ ./examples/rust/build.sh
```

Then you can run the examples:

```bash
$ cargo run --example wasm-rust <example>
```

Where `<example>` is one of the following:

- `hello`: A simple example that prints a number to the console.
- `tinywasm`: Runs `hello` using TinyWasm - inside of TinyWasm itself!
