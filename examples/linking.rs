use anyhow::Result;
use tinywasm::{ModuleInstance, Store};

const WASM_ADD: &str = r#"
(module
  (func $add (param $lhs i32) (param $rhs i32) (result i32)
    local.get $lhs
    local.get $rhs
    i32.add)
  (export "add" (func $add)))
"#;

const WASM_IMPORT: &str = r#"
(module
  (import "adder" "add" (func $add (param i32 i32) (result i32)))
  (func $main (result i32)
    i32.const 1
    i32.const 2
    call $add)
  (export "main" (func $main))
)
"#;

fn main() -> Result<()> {
    let wasm_add = wat::parse_str(WASM_ADD)?;
    let wasm_import = wat::parse_str(WASM_IMPORT)?;

    let add_module = tinywasm::parse_bytes(&wasm_add)?;
    let import_module = tinywasm::parse_bytes(&wasm_import)?;

    let mut store = Store::default();

    let add_instance = ModuleInstance::instantiate(&mut store, &add_module, None)?;

    // Imports can link a module namespace and define individual host items together.
    let mut imports = tinywasm::Imports::new();
    imports.link_module("adder", add_instance)?;

    let import_instance = ModuleInstance::instantiate(&mut store, &import_module, Some(&imports))?;

    // Calling `main` crosses the linked module boundary to `add`.
    let main = import_instance.func::<(), i32>(&store, "main")?;
    assert_eq!(main.call(&mut store, ())?, 3);

    Ok(())
}
