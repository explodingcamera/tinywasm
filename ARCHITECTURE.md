# TinyWasm's Architecture

TinyWasm follows the general Runtime Structure described in the [WebAssembly Specification](https://webassembly.github.io/spec/core/exec/runtime.html).

Key runtime layout:

- Values are stored in four fixed-capacity typed stacks: `stack_32` (`i32`/`f32`), `stack_64` (`i64`/`f64`), `stack_128` (`v128`), and `stack_ref` (`funcref`/`externref`).
- Locals are allocated in those value stacks. Each `CallFrame` stores `locals_base`, and local ops index from that base.
- Calls use a separate fixed-capacity `CallStack` of `CallFrame`s.
- Structured control (`block`/`loop`/`if`/`br*`) is lowered during parsing to jump-oriented instructions: `Jump`, `JumpIfZero`, `BranchTable*`, `DropKeep*`, and `Return`.
- The interpreter executes this lowered bytecode in a single iterative loop.

## Precompiled Modules

`TinyWasmModule` can be serialized to `.twasm` (`serialize_twasm`) and loaded later (`from_twasm`).
This allows deployments that execute precompiled modules without enabling the parser in the runtime binary.

See:

- [visit.rs](./crates/parser/src/visit.rs)
- [instructions.rs](./crates/types/src/instructions.rs)
- [value_stack.rs](./crates/tinywasm/src/interpreter/stack/value_stack.rs)
- [call_stack.rs](./crates/tinywasm/src/interpreter/stack/call_stack.rs)
