# `tinywasm-c-api`

This crate will expose the C API surface for TinyWasm.

It is currently a placeholder crate that exists to reserve the package name and workspace layout.

## Artifacts

For now, this crate only provides boilerplate packaging for future C consumers:

- a vendored upstream-compatible public header at `include/wasm.h`
- a `Makefile` that builds the Rust crate and stages C-facing artifacts in `dist/`

Build the staged artifacts with the host Rust target:

```sh
make -C crates/c-api
```

Artifacts are staged under `dist/<target>/`.

Override the target triple when needed:

```sh
make -C crates/c-api RUST_TARGET=aarch64-apple-darwin
```
