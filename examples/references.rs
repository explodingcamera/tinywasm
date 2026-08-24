use anyhow::Result;
use tinywasm::{FuncRef, HostFunction, Imports, ModuleInstance, Store};

const WASM: &str = r#"
(module
  (type $callback (func (param i32) (result i32)))
  (import "host" "double" (func $double (type $callback)))
  (elem declare func $double)
  (func (export "callback") (result funcref)
    ref.func $double)
  (func (export "apply") (param funcref i32) (result i32)
    local.get 1
    local.get 0
    ref.cast (ref $callback)
    call_ref $callback))
"#;

fn main() -> Result<()> {
    let mut store = Store::default();
    let mut imports = Imports::new();
    // Imports resolve reusable host definitions against each module's runtime types.
    // HostFunction::instantiate is available when a standalone Function is needed.
    imports.define("host", "double", HostFunction::from(|_, value: i32| -> tinywasm::Result<i32> { Ok(value * 2) }));

    let wasm = wat::parse_str(WASM)?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    // Option<FuncRef> maps nullable funcref. Bare FuncRef maps a non-null reference.
    let callback = instance.func::<(), Option<FuncRef>>(&store, "callback")?.call(&mut store, ())?;
    let apply = instance.func::<(Option<FuncRef>, i32), i32>(&store, "apply")?;

    assert_eq!(apply.call(&mut store, (callback, 21))?, 42);
    Ok(())
}
