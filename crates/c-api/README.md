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

## Notes

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
`9d6b93764ac96cdd9db51081c363e09d2d488b4d` under
[`include/LICENSE-wasm-c-api`](include/LICENSE-wasm-c-api).
