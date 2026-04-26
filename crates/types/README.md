# `tinywasm-types`

This crate contains the shared module, instruction, value, and archive types used by [`tinywasm`](https://crates.io/crates/tinywasm) and [`tinywasm-parser`](https://crates.io/crates/tinywasm-parser).

Most users should depend on `tinywasm` directly. This crate is useful when you need to work with parsed modules, serialized `twasm` archives, or shared type definitions without pulling in the runtime.
