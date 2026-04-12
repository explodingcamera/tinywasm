use eyre::Result;
use tinywasm::Module;

#[test]
fn func_typed_rejects_wrong_param_or_result_types() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (func (export "add") (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.add)
        )
        "#,
    )?;

    let module = Module::parse_bytes(&wasm)?;
    let mut store = tinywasm::Store::default();
    let instance = module.instantiate(&mut store, None)?;

    assert!(instance.func_typed::<(i32, i32), i32>(&store, "add").is_ok());
    assert!(instance.func_typed::<i32, i32>(&store, "add").is_err());
    assert!(instance.func_typed::<(i32, i32), ()>(&store, "add").is_err());

    Ok(())
}

#[test]
fn func_typed_rejects_partial_multi_value_results() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (func (export "pair") (result i32 i32)
            i32.const 1
            i32.const 2)
        )
        "#,
    )?;

    let module = Module::parse_bytes(&wasm)?;
    let mut store = tinywasm::Store::default();
    let instance = module.instantiate(&mut store, None)?;

    assert!(instance.func_typed::<(), (i32, i32)>(&store, "pair").is_ok());
    assert!(instance.func_typed::<(), i32>(&store, "pair").is_err());

    Ok(())
}
