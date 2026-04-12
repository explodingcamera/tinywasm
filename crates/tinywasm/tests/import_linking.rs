use eyre::Result;
use tinywasm::{Error, Imports, Module, Store};

const WASM_ADD: &str = r#"
    (module
      (func $add (param i32 i32) (result i32)
        local.get 0
        local.get 1
        i32.add)
      (export "add" (func $add)))
"#;

const WASM_IMPORT: &str = r#"
    (module
      (import "adder" "add" (func $add (param i32 i32) (result i32)))
      (func (export "main") (result i32)
        i32.const 1
        i32.const 2
        call $add))
"#;

fn parse_modules() -> Result<(Module, Module)> {
    let add = Module::parse_bytes(&wat::parse_str(WASM_ADD)?)?;
    let import = Module::parse_bytes(&wat::parse_str(WASM_IMPORT)?)?;
    Ok((add, import))
}

#[test]
fn link_module_links_same_store_instance() -> Result<()> {
    let (add_module, import_module) = parse_modules()?;
    let mut store = Store::default();

    let add_instance = add_module.instantiate(&mut store, None)?;
    let mut imports = Imports::new();
    imports.link_module("adder", add_instance)?;

    let instance = import_module.instantiate(&mut store, Some(imports))?;
    let main = instance.func::<(), i32>(&store, "main")?;
    assert_eq!(main.call(&mut store, ())?, 3);
    Ok(())
}

#[test]
fn link_module_rejects_cross_store_instance() -> Result<()> {
    let (add_module, import_module) = parse_modules()?;

    let mut source_store = Store::default();
    let add_instance = add_module.instantiate(&mut source_store, None)?;

    let mut target_store = Store::default();
    let mut imports = Imports::new();
    imports.link_module("adder", add_instance)?;

    let err = import_module.instantiate(&mut target_store, Some(imports)).unwrap_err();
    assert!(matches!(err, Error::InvalidStore));
    Ok(())
}
