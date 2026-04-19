use eyre::Result;
use tinywasm::{ModuleInstance, Store};

const MODULE_WAT: &str = r#"
    (module
      (func (export "add") (param i32 i32) (result i32)
        local.get 0
        local.get 1
        i32.add)
      (memory (export "memory") 1)
    )
"#;

#[test]
fn func_handle_rejects_wrong_store() -> Result<()> {
    let wasm = wat::parse_str(MODULE_WAT)?;
    let module = tinywasm::parse_bytes(&wasm)?;

    let mut owner_store = Store::default();
    let instance = ModuleInstance::instantiate(&mut owner_store, &module, None)?;
    let func = instance.func_untyped(&owner_store, "add")?;

    let mut other_store = Store::default();
    let err = func.call(&mut other_store, &[1.into(), 2.into()]).unwrap_err();
    assert!(err.to_string().contains("invalid store"));

    Ok(())
}

#[test]
fn memory_access_rejects_wrong_store() -> Result<()> {
    let wasm = wat::parse_str(MODULE_WAT)?;
    let module = tinywasm::parse_bytes(&wasm)?;

    let mut owner_store = Store::default();
    let instance = ModuleInstance::instantiate(&mut owner_store, &module, None)?;

    let memory = instance.memory("memory")?;
    let other_store = Store::default();
    let err = memory.len(&other_store).unwrap_err();
    assert!(err.to_string().contains("invalid store"));

    Ok(())
}
