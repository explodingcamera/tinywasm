# TinyWasm's Architecture

TinyWasm follows the general Runtime Structure described in the [WebAssembly Specification](https://webassembly.github.io/spec/core/exec/runtime.html).
Some key differences are:

- **Type Storage**: Types are inferred from usage context rather than stored explicitly, with all values held as `u64`.
- **Stack Design**: Implements a specific stack for values, labels, and frames to simplify the implementation and enable optimizations.
- **Bytecode Format**: Adopts a custom bytecode format to reduce memory usage and improve performance by allowing direct execution without the need for decoding.
- **Global State Access**: Allows cross-module access to the `Store`'s global state, optimizing imports and exports access. Access requires a module instance reference, maintaining implicit ownership through a reference count.
- **Non-thread-safe Store**: Designed for efficiency in single-threaded applications.
- **JIT Compilation Support**: Prepares for JIT compiler integration with function instances designed to accommodate `WasmFunction`, `HostFunction`, or future `JitFunction`.
- **`no_std` Environment Support**: Offers compatibility with `no_std` environments by allowing disabling of `std` feature
- **Call Frame Execution**: Executes call frames in a single loop rather than recursively, using a single stack for all frames, facilitating easier pause, resume, and step-through.

## Bytecode Format

To improve performance and reduce code size, instructions are encoded as enum variants instead of opcodes.
This allows preprocessing the bytecode into a more memory aligned format, which can be loaded directly into memory and executed without decoding later. This can skip the decoding step entirely on resource-constrained devices where memory is limited. See this [blog post](https://wasmer.io/posts/improving-with-zero-copy-deserialization) by Wasmer
for more details which inspired this design.

Some instructions are split into multiple variants to reduce the size of the enum (e.g. `br_table` and `br_label`).
Additionally, label instructions contain offsets relative to the current instruction to make branching faster and easier to implement.
Also, `End` instructions are split into `End` and `EndBlock`. Others are also combined, especially in cases where the stack can be skipped.

See [instructions.rs](./crates/types/src/instructions.rs) for the full list of instructions.

This is a area that can still be improved. While being able to load pre-processes bytecode directly into memory is nice, in-place decoding could achieve similar speeds, see [A fast in-place interpreter for WebAssembly](https://arxiv.org/abs/2205.01183).
