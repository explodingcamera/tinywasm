use color_eyre::eyre::Result;
use tinywasm::{parser::Parser, types::TinyWasmModule, Module, Store};

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
    let module = Parser::default().parse_module_bytes(wasm)?;
    let twasm = module.serialize_twasm();

    // now, you could e.g. write twasm to a file called `add.twasm`
    // and load it later in a different program

    let module: Module = TinyWasmModule::from_twasm(&twasm)?.into();
    let mut store = Store::default();
    let instance = module.instantiate(&mut store, None)?;
    let add = instance.exported_func::<(i32, i32), i32>(&store, "add")?;

    assert_eq!(add.call(&mut store, (1, 2))?, 3);

    Ok(())
}
