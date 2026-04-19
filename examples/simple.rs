use eyre::Result;
use tinywasm::{ModuleInstance, Store};

const WASM: &str = r#"
(module
  (func $add (param $lhs i32) (param $rhs i32) (result i32)
    local.get $lhs
    local.get $rhs
    i32.add)
  (export "add" (func $add)))
"#;

fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM).expect("failed to parse wat");
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let add = instance.func::<(i32, i32), i32>(&store, "add")?;

    assert_eq!(add.call(&mut store, (1, 2))?, 3);
    Ok(())
}
