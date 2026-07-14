use eyre::Result;
use tinywasm::{ModuleInstance, Store};

#[test]
fn exported_wasi_start_is_not_run_during_instantiation() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (table 1 funcref)
          (func $_start (export "_start")
            unreachable)
          (elem (i32.const 0) func $_start))
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();

    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

    assert!(instance.start_func(&store)?.is_none());
    assert!(instance.func::<(), ()>(&store, "_start")?.call(&mut store, ()).is_err());
    Ok(())
}
