use tinywasm::{HostFunction, Imports, ModuleInstance, Store};

#[test]
fn multi_arg_host_imports_preserve_source_order() {
    let wasm = wat::parse_str(
        r#"
        (module
          (import "env" "pair" (func $pair (param i32 i32) (result i32)))
          (func (export "call_pair") (param i32 i32) (result i32)
            local.get 0
            local.get 1
            call $pair))
        "#,
    )
    .unwrap();

    let module = tinywasm::parse_bytes(&wasm).unwrap();
    let mut store = Store::default();

    let pair = HostFunction::from(&mut store, |_ctx, (left, right): (i32, i32)| -> tinywasm::Result<i32> {
        Ok(left * 1000 + right)
    });

    let mut imports = Imports::new();
    imports.define("env", "pair", pair);

    let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports)).unwrap();
    let func = instance.func::<(i32, i32), i32>(&store, "call_pair").unwrap();

    assert_eq!(func.call(&mut store, (12, 34)).unwrap(), 12034);
}
