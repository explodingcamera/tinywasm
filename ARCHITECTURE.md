# TinyWasm Architecture

TinyWasm follows the general runtime model described in the [WebAssembly specification](https://webassembly.github.io/spec/core/exec/runtime.html), but lowers validated WebAssembly into a compact internal instruction format before execution.

## Runtime Layout

- Values are stored in untyped stacks:
  - `stack_32` for `i32`, `f32`, `funcref`, and `externref`
  - `stack_64` for `i64` and `f64`
  - `stack_128` for `v128`
- Locals are stored directly in the value stacks. Each `CallFrame` stores a `locals_base`, and local instructions index from that base.
- Structured control flow (`block`, `loop`, `if`, `br*`) is lowered during parsing to jump-oriented internal instructions such as `Jump`, `JumpIfZero`, `BranchTable*`, `DropKeep*`, and `Return`.
- Execution is a single iterative interpreter loop over the lowered instruction stream.

## Internal Bytecode

TinyWasm does not interpret WebAssembly instructions directly. During parsing and validation, WebAssembly is translated into TinyWasm's internal bytecode format.

This internal representation is designed to make execution simpler and cheaper:

- structured control flow is resolved ahead of time
- stack effects are made explicit
- common instruction sequences can be fused into superinstructions
- modules can optionally be serialized as `.twasm` for reuse

## Optimizer

During parsing, a peephole optimizer (`optimize.rs`) fuses common instruction sequences into superinstructions. These reduce interpreter dispatch overhead by combining multiple logical operations into one internal instruction.

Examples include:

- **Fused binops**: `BinOpLocalLocal*`, `BinOpLocalConst*`, `BinOpStackGlobal*`  
  Combine local/global access, a binary operation, and sometimes a store/tee.
- **Fused jumps**: `JumpCmpLocalConst*`, `JumpCmpLocalLocal*`, `JumpCmpStackConst*`  
  Combine comparison and conditional branch logic.

## Memory Backends

Linear memory is implemented through the `LinearMemory` trait. The backend is selected with `engine::Config::with_memory_backend()`.

Available backends:

- `VecMemory` - contiguous `Vec<u8>` backing; the default backend.
- `PagedMemory` - chunk-based allocation, useful when growing memory without reallocating one large buffer.
- `LazyLinearMemory` - wraps another backend and allocates memory on first access.
- Custom backends through `MemoryBackend::custom()`.

## Future Experiments

TinyWasm's interpreter is intentionally simple today: validated WebAssembly is lowered to internal instructions, optimized with peephole fusion, and executed by an iterative dispatch loop.

Future work may explore additional dispatch and code-generation strategies, including Rust's experimental `loop_match` state-machine work, explicit tail calls, more aggressive superinstruction fusion, top-of-stack register allocation, or even optional JIT compilation.

## Code Map

- [visit.rs](./crates/parser/src/visit.rs) - WebAssembly binary visitor
- [optimize.rs](./crates/parser/src/optimize.rs) - peephole optimizer and superinstruction fusion
- [parallel.rs](./crates/parser/src/parallel.rs) - multithreaded function parsing
- [instructions.rs](./crates/types/src/instructions.rs) - internal instruction set
- [value_stack.rs](./crates/tinywasm/src/interpreter/stack/value_stack.rs) - typed value stacks
- [call_stack.rs](./crates/tinywasm/src/interpreter/stack/call_stack.rs) - call frame stack
- [memory/mod.rs](./crates/tinywasm/src/store/memory/mod.rs) - memory backend trait and implementations
