# TinyWasm Architecture

TinyWasm follows the general runtime model described in the [WebAssembly specification](https://webassembly.github.io/spec/core/exec/runtime.html). It is a stack-based interpreter with a compact internal bytecode, width-specific value stacks, and a contiguous `Vec`-backed linear memory.

## Execution Pipeline

TinyWasm does not execute WebAssembly instructions directly. Parsing lowers them into an internal bytecode designed to make execution simpler and cheaper:

- structured control flow (`block`, `loop`, `if`, and `br*`) becomes jump-oriented instructions such as `Jump`, `JumpIfZero`, `BranchTable*`, `DropKeep*`, and `Return`
- operand widths are encoded in instruction variants, and branch stack reshaping is explicit
- instructions retain compact module-local indexes, which each instance maps to Store-wide runtime addresses
- when enabled, the optimizer applies local rewrites, including superinstruction fusion, specialized calls and returns, and redundant-instruction removal
- modules can be serialized as `.twasm` archives containing this lowered representation
- execution uses a single iterative dispatch loop over the resulting instruction stream

## Value Stacks

WebAssembly combines an operand stack with function-scoped locals. TinyWasm stores both in the same width-specific physical stacks:

- `stack_32` for `i32`, `f32`, and reference values, including GC and exception references
- `stack_64` for `i64` and `f64`
- `stack_128` for `v128`

The interpreter does not maintain a runtime type stack or tag individual stack slots. Lowered instructions encode the physical lane they operate on, while WebAssembly validation guarantees type correctness. Splitting values by width lets each value use its natural storage size, reducing stack memory and the data moved by common operations.

Locals are stored directly in these stacks. Each `CallFrame` records a base for every lane, and lowered local instructions index from those bases. The value stacks and call stack can use either a fixed capacity or dynamic initial and maximum sizes. Dynamic stacks keep the initial allocation small, grow when needed, and retain a hard limit.

## Interpreter Optimization

Instruction dispatch is one of the interpreter's main costs. TinyWasm reduces it through superinstructions and by shaping the large Rust dispatch match based on benchmarks and assembly inspection. Most simple arithmetic remains directly in the interpreter loop. Small, frequently used stack, value, and global operations use `#[inline]` or `#[inline(always)]` where measurements show a benefit, while unlikely error paths use `core::hint::cold_path()`.

Superinstructions also reduce value-stack traffic. They can read locals, globals, and constants directly, perform an operation, and write `set` or `tee` destinations without materializing intermediate operand-stack values. Examples include:

- fused binary operations such as `BinOpLocalLocal*`, `BinOpLocalConst*`, and `BinOpStackGlobal*`
- fused conditional branches such as `JumpCmpLocalConst*`, `JumpCmpLocalLocal*`, and `JumpCmpStackConst*`

The default runtime remains safe Rust throughout rather than relying on unchecked operations.

## SIMD

SIMD instructions have a portable safe-Rust implementation built from fixed-size arrays and lane operations, relying on the compiler to auto-vectorize where possible. Generated code is inspected with `cargo asm`, and benchmarks determine where architecture-specific alternatives are worthwhile. WebAssembly targets use native SIMD intrinsics where available, while the optional `simd-x86` feature provides selected x86 implementations for operations where the generic code produces worse results.

## Linear Memory

Linear memory is a contiguous `Vec<u8>` allocation owned by a `MemoryInstance`. The interpreter accesses it through the internal `MemoryStorage` type, a small concrete boundary that keeps the `Vec` representation out of the executor so an mmap-backed storage can be substituted later without touching load and store paths.

Fixed-width loads and stores use a single const-generic `read_fixed::<N>` / `write_fixed::<N>` pair rather than per-width vtable methods. Scalar operations reduce to an effective-address computation, a bounds check, a slice access, and a `from_le_bytes` / `to_le_bytes` conversion, with out-of-bounds construction kept on cold paths. Bulk operations such as `fill` and `copy_within` map directly to native slice methods.

Memory growth keeps the Wasm page count and limits on `MemoryInstance`. Before memory or table backing storage is allocated or resized, the configured `ResourceLimiter` is consulted so a host can bound guest resource consumption. The limiter is shared across the stores created from one `Engine` and lives behind an `Arc`.

For conventional operating systems, a future mmap-backed storage could reserve virtual address space and use guard pages to move more bounds enforcement to the operating system, reducing explicit checks in linear-memory hot paths. This is the same broad approach described in [Wasmtime's linear-memory architecture](https://docs.wasmtime.dev/contributing-architecture.html#linear-memory), where virtual-memory reservations and guard regions eliminate or deduplicate explicit bounds checks.

## Future Experiments

Future work may explore additional dispatch and code-generation strategies, including Rust's experimental `loop_match` state-machine work, a tail-call-based interpreter once Rust's explicit tail-call support matures, more aggressive superinstruction fusion, top-of-stack register allocation, or optional JIT compilation.

## Important Modules

- [visit.rs](./crates/parser/src/visit.rs) - function-body operator lowering
- [optimize.rs](./crates/parser/src/optimize.rs) - peephole optimizer and superinstruction fusion
- [parallel.rs](./crates/parser/src/parallel.rs) - parallel function parsing
- [instructions.rs](./crates/types/src/instructions.rs) - internal instruction set
- [value_stack.rs](./crates/tinywasm/src/interpreter/stack/value_stack.rs) - width-specific stacks
- [call_stack.rs](./crates/tinywasm/src/interpreter/stack/call_stack.rs) - call frame stack
- [memory/mod.rs](./crates/tinywasm/src/store/memory/mod.rs) - linear memory storage
