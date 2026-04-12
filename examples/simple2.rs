use eyre::Result;
use tinywasm::{Module, Store};

const WASM: &str = r#"
(module
  (func $return (param $lhs i32) (param $rhs i64) (result i32 i64)
    local.get $lhs
    local.get $rhs)
  (export "return" (func $return)))
"#;

fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM).expect("failed to parse wat");
    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = module.instantiate(&mut store, None)?;
    let add = instance.func_typed::<(i32, i64), (i32, i64)>(&store, "return")?;

    assert_eq!(add.call(&mut store, (1, 2))?, (1, 2));
    Ok(())
}
