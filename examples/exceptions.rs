use anyhow::{Result, bail};
use tinywasm::{Error, ModuleInstance, Store, WasmValue};

const WASM: &str = r#"
(module
  (tag $failure (export "failure") (param i32))
  (func (export "fail") (param i32)
    local.get 0
    throw $failure))
"#;

fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM)?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let fail = instance.func::<i32, ()>(&store, "fail")?;

    // Uncaught guest exceptions cross the host boundary as owned ExnRef handles.
    let Err(Error::Exception(exception)) = fail.call(&mut store, 42) else {
        bail!("expected a guest exception");
    };

    // The handle exposes the exception tag and typed payload values.
    assert_eq!(exception.tag(&store)?, instance.tag("failure")?);
    assert_eq!(exception.fields(&mut store)?, [WasmValue::I32(42)]);
    Ok(())
}
