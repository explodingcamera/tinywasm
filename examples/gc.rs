use anyhow::Result;
use tinywasm::{ModuleInstance, Store, StructRef, WasmValue};

const WASM: &str = r#"
(module
  (type $cube (struct (field (mut i32)) (field (mut i32))))
  (func $make-cube (result (ref $cube))
    i32.const 3
    i32.const 4
    struct.new $cube)
  (func (export "cube") (result (ref struct))
    call $make-cube))
"#;

fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM)?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let cube = instance.func::<(), StructRef>(&store, "cube")?.call(&mut store, ())?;

    // GC handles are Store-bound and expose typed field access through that Store.
    assert_eq!(cube.fields(&mut store)?, [WasmValue::I32(3), WasmValue::I32(4)]);
    cube.set_field(&mut store, 0, WasmValue::I32(5))?;

    // Cloned handles keep their guest objects live across explicit collection.
    let retained = cube.clone();
    drop(cube);
    store.gc()?;

    assert_eq!(retained.field(&mut store, 0)?, WasmValue::I32(5)); // still alive
    Ok(())
}
