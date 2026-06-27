use eyre::Result;
use tinywasm::{FuncContext, HostFunction, Imports, ModuleInstance, Store};

const WASM: &str = r#"
(module
  (import "host" "call_add_twice" (func $call_add_twice (param i32) (result i32)))

  (func $add_one (export "add_one") (param i32) (result i32)
    local.get 0
    i32.const 1
    i32.add)

  (func (export "run") (param i32) (result i32)
    local.get 0
    call $call_add_twice
    i32.const 10
    i32.add))
"#;

fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM)?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();

    let call_add_twice = HostFunction::from(&mut store, |mut ctx: FuncContext<'_>, value: i32| {
        let add_one = ctx.module().func::<i32, i32>(ctx.store(), "add_one")?;

        // Use ctx.call for reentrant calls from host functions. Function::call
        // starts a root invocation and cannot preserve the active call stacks.
        let value = ctx.call(&add_one, value)?;
        ctx.call(&add_one, value)
    });

    let mut imports = Imports::new();
    imports.define("host", "call_add_twice", call_add_twice);

    let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;
    let run = instance.func::<i32, i32>(&store, "run")?;

    assert_eq!(run.call(&mut store, 40)?, 52);

    Ok(())
}
