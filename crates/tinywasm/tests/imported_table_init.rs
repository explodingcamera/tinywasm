use eyre::Result;
use tinywasm::types::{FuncRef, TableType, WasmType, WasmValue};
use tinywasm::{Imports, Module, Store, Table};

#[test]
fn imported_table_uses_provided_init_value() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (import "host" "table" (table 3 funcref))
          (func (export "slot_is_null") (param i32) (result i32)
            local.get 0
            table.get 0
            ref.is_null)
        )
        "#,
    )?;

    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let mut imports = Imports::new();
    let table =
        Table::new(&mut store, TableType::new(WasmType::RefFunc, 3, None), WasmValue::RefFunc(FuncRef::new(Some(0))))?;
    imports.define("host", "table", table);

    let instance = module.instantiate(&mut store, Some(imports))?;
    let slot_is_null = instance.func::<i32, i32>(&store, "slot_is_null")?;

    assert_eq!(slot_is_null.call(&mut store, 0)?, 0);
    assert_eq!(slot_is_null.call(&mut store, 1)?, 0);

    Ok(())
}
