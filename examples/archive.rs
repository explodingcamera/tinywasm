use anyhow::Result;
use tinywasm::{Module, ModuleInstance, Store, parser::Parser};

const WASM: &str = r#"
(module
  (func $add (param $lhs i32) (param $rhs i32) (result i32)
    local.get $lhs
    local.get $rhs
    i32.add)
  (export "add" (func $add)))
"#;

fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM)?;
    let module = Parser::default().parse_module_bytes(wasm)?;

    // Serialize the optimized module for storage or distribution.
    let twasm = module.serialize_twasm()?;

    // Archived modules load without parsing Wasm again and should only come from trusted sources.
    let module = Module::try_from_twasm(&twasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let add = instance.func::<(i32, i32), i32>(&store, "add")?;

    assert_eq!(add.call(&mut store, (1, 2))?, 3);

    Ok(())
}
