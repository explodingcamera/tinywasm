use anyhow::Result;
use tinywasm::{ModuleInstance, Store};
use tinywasm_wasi::p1::{WasiCtx, imports};

const WASM: &str = r#"
(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 16) "Hello from WASI Preview 1!\n")

  (func (export "_start")
    i32.const 0
    i32.const 16
    i32.store
    i32.const 4
    i32.const 27
    i32.store

    i32.const 1
    i32.const 0
    i32.const 1
    i32.const 8
    call $fd_write
    drop))
"#;

// Requires TinyWasm's `parser`, `std`, and `state` features.
// Run with `cargo run --example wasi_p1` from the repository root.
fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM)?;
    let module = tinywasm::parse_bytes(&wasm)?;

    // The host explicitly chooses what the guest can observe.
    let wasi = WasiCtx::new().with_args(["wasi_p1.wasm", "example"])?.with_env([("MODE", "example")])?.inherit_stdio();
    let mut store = Store::default().with_state(wasi);

    // Preview 1 imports are reusable because their state comes from the Store.
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports()))?;
    instance.func::<(), ()>(&store, "_start")?.call(&mut store, ())?;
    Ok(())
}
