# TinyWasm's Architecture

TinyWasm follows the general Runtime Structure described in the [WebAssembly Specification](https://webassembly.github.io/spec/core/exec/runtime.html).
Some key differences are:

- Values are stored without their type, (as `u64`), and the type is inferred from the instruction that uses them. This is possible because the instructions are validated before execution and the type of each value can be inferred from the instruction.
- TinyWasm has a explicit stack for values, labels and frames. This is mostly for simplicity in the implementation, but also allows for some optimizations.
- Floats always use a canonical NaN representation, the spec allows for multiple NaN representations.
- TinyWasm uses a custom bytecode format (see [Bytecode Format](#bytecode-format) for more details)
- Global state in the `Store` can be addressed from module instances other than the owning module. This is to allow more efficient access to imports and exports. Ownership is still enforced implicitly by requiring a reference to the instance to access it which can not be changed using the WebAssembly instructions.
- The `Store` is not thread-safe. This is to allow for more efficient access to the `Store` and its contents. When later adding support for threads, a `Mutex` can be used to make it thread-safe but the overhead of requiring a lock for every access is not necessary for single-threaded applications.
- TinyWasm is architectured to allow for a JIT compiler to be added later. Functions are stored as FunctionInstances which can contain either a `WasmFunction` or a `HostFunction`. A third variant `JitFunction` could be added later to store a pointer to the compiled function. This would allow for the JIT to be used transparently without changing the rest of the runtime.
- TinyWasm is designed to be used in `no_std` environments. The `std` feature is enabled by default, but can be disabled to remove the dependency on `std` and `std::io`. This is done by disabling the `std` and `parser` features. The `logging` feature can also be disabled to remove the dependency on `log`. This is not recommended, since `libm` is not as performant as the compiler's math intrinsics, especially on wasm32 targets, but can be useful for resource-constrained devices or other environments where `std` is not available such as OS kernels.
- Call Frames are executed in a loop instead of recursively. This allows the use of a single stack for all frames and makes it easier to pause execution and resume it later, or to step through the code one instruction at a time.

## Bytecode Format

To improve performance and reduce code size, instructions are encoded as enum variants instead of opcodes.
This allows preprocessing the bytecode into a more compact format, which can be loaded directly into memory and executed without decoding later. This can skip the decoding step entirely on resource-constrained devices where memory is limited.

Some instructions are split into multiple variants to reduce the size of the enum (e.g. `br_table` and `br_label`).
Additionally, label instructions contain offsets relative to the current instruction to make branching faster and easier to implement.
Also, `End` instructions are split into `End` and `EndBlock`.

See [instructions.rs](./crates/types/src/instructions.rs) for the full list of instructions.

This is a area that can still be improved. While being able to load pre-processes bytecode directly into memory is nice, in-place decoding could achieve similar speeds, see [A fast in-place interpreter for WebAssembly](https://arxiv.org/abs/2205.01183).
