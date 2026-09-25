# Benchmarks

```sh
cargo bench-suite
cargo bench-suite execute
cargo coremark
```

To rebuild the fixtures, install the `wasm32-unknown-unknown` target and Binaryen,
then run `bash crates/bench/rebuild-fixtures.sh`.
`fixtures/coremark-minimal.wasm` is from
[wasm3/wasm-coremark](https://github.com/wasm3/wasm-coremark) and its license and terms are in `fixtures/LICENSE-coremark.md`.
