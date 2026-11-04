# TinyWasm C API

Experimental [WebAssembly C API](https://github.com/WebAssembly/wasm-c-api) support for TinyWasm.
Use `wasm.h` to embed it in C or C++. See [Wasmtime's C API documentation](https://docs.wasmtime.dev/c-api/wasm_8h.html)
for the standard interface and [`examples/add.c`](examples/add.c) for an example.

## Build

```sh
make -C crates/c-api
```

This builds `libtinywasm.so` (or `.dylib` on macOS) and `libtinywasm.a` in
`target/release`. Headers are in `crates/c-api/include`. To install them and a
`pkg-config` file, run `make -C crates/c-api install`. Run `make -C crates/c-api example` to
build and run the C example.

## Symbol prefix

By default, the library exports the names in `wasm.h`. To avoid symbol
collisions with another WebAssembly C API implementation, build with a prefix:

```sh
make -C crates/c-api TINYWASM_C_API_PREFIX=my_
```

Define the same prefix before including `tinywasm.h` in C or C++:

```c
#define TINYWASM_C_API_PREFIX my_
#include "tinywasm.h"
```

When building with Cargo directly, enable `custom-prefix` and set the
`TINYWASM_C_API_PREFIX` environment variable. Include `tinywasm.h` before
`wasm.h` so the symbol aliases take effect.

## Runtime-neutral extension direction

The integration target is the standard `wasm.h` plus a small, separately versioned common
extension header implemented by TinyWasm and WAMR. `wasm_ext.h` is an experimental numeric continuation implementation, not an approved standard.
Enable the C wrapper's `external-host-calls` feature (`CONTINUATIONS=1` with Make).
The Rust API is its backend machinery; see [the shared fixture](tests/continuations/README.md).
See the [WAMR roadmap discussion](https://github.com/wasm-micro-runtime/wasm-micro-runtime/discussions/5164#discussioncomment-19111943).

Keep ordinary module linking, calls, memories, tables, and ownership on the standard API.
Test the same external component/WASI layer against both implementations. A common header
alone does not establish shared semantics, object compatibility, or binary plugin compatibility.
Objects must remain with their originating runtime and store. Symbol prefixes permit static
coexistence; any runtime dispatch table belongs in an optional adapter outside the core.

The prototype provides opaque invocation handles, completed/pending/error outcomes, host-call
suspension, checked completion, and cancellation. Qualify reference rooting, caller memory/context
access, nested canonical ABI operations, error/trap ownership, and finalizer ordering before
freezing declarations. Callback-style
async can use ordinary calls; stackful suspension support must be qualified per execution mode.
Do not expose interpreter frames, instruction pointers, or Rust futures across the C ABI.
Wakers, event loops, resource tables, canonical ABI, and WASI implementations stay external.

Optional functionality must not add fields to ordinary stores/instances/call frames, branches
to synchronous dispatch, global registration roots, or allocation/initialization work in disabled
builds. Put implementations in separately selectable crates or translation units. Avoid forcing
whole-archive retention; shared-library export roots and callback tables need explicit auditing.
LTO cannot remove observable fields or reachable hooks merely because an application never
uses their feature.

Acceptance requires final linked-artifact comparisons with identical compiler, linker, LTO,
whole-program, panic, allocator, and stripping settings: baseline versus extension-disabled;
enabled-but-unused; and enabled-and-used. Check text/rodata/data/bss, retained symbols and link
maps, store/frame layouts, heap/stack high-water marks, and execution cost. Use the same portable
C conformance fixtures for both runtimes, including shared memory/table imports and cancel/error
paths. On a Pico, measure static RAM and heap/stack peaks; RSS applies to hosted deployments.
The current TinyWasm C wrapper requires `std`; Pico C-wrapper support is not qualified by hosted C fixture tests. Final linked-artifact
comparisons and process-memory measurements are recorded separately from Rust seam tests.

## Current API limitations

- Follow the ownership annotations in `wasm.h` and use the matching delete
  functions. Keep a store and its objects on the same thread.
- Load `.wasm` bytes with `wasm_module_new`. Imports are positional, in module
  import order.
- Include `tinywasm.h` for `tinywasm_last_error_message`, which copies the
  calling thread's last error into a vector you delete with `wasm_byte_vec_delete`.
- Module imports and exports must have types representable in `wasm.h`. SIMD
  signatures, shared memory, memory64/table64, GC references, and tags are not supported at
  the module boundary.

`wasm.h` is vendored from WebAssembly/wasm-c-api commit
`9d6b93764ac96cdd9db51081c363e09d2d488b4d`, under
[Apache 2.0](../../LICENSE-APACHE).
