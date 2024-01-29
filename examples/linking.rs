use color_eyre::eyre::Result;
use tinywasm::{Module, Store};

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
    let wasm_add = wat::parse_str(WASM_ADD).expect("failed to parse wat");
    let wasm_import = wat::parse_str(WASM_IMPORT).expect("failed to parse wat");

    let add_module = Module::parse_bytes(&wasm_add)?;
    let import_module = Module::parse_bytes(&wasm_import)?;

    let mut store = Store::default();
    let add_instance = add_module.instantiate(&mut store, None)?;

    let mut imports = tinywasm::Imports::new();
    imports.link_module("adder", add_instance.id())?;
    let import_instance = import_module.instantiate(&mut store, Some(imports))?;

    let main = import_instance.exported_func::<(), i32>(&store, "main")?;
    assert_eq!(main.call(&mut store, ())?, 3);
    Ok(())
}
