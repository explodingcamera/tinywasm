# `tinywasm-types`

This crate contains the shared module, instruction, value, and archive types used by [`tinywasm`](https://crates.io/crates/tinywasm) and [`tinywasm-parser`](https://crates.io/crates/tinywasm-parser).

Most users should depend on `tinywasm` directly. This crate is useful when you need to work with parsed modules, serialized `twasm` archives, or shared type definitions without pulling in the runtime.

The opt-in `unstable-component-model` feature exposes an owned `Component` and its declaration types for future linking. It does not change the `.twasm` module archive format.

## API stability

`tinywasm-types` is semver-exempt and may make breaking API changes in any release. Types re-exported by `tinywasm` remain covered by `tinywasm`'s compatibility policy when used through that crate.
