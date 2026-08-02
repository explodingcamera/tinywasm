use eyre::Result;
use tinywasm::types::{FuncRef, RefType, TableType};
use tinywasm::{HostFunction, Imports, ModuleInstance, Store, Table};

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

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let mut imports = Imports::new();
    let _function_at_zero = HostFunction::from(&mut store, |_, ()| Ok(()));
    let table = Table::new(&mut store, TableType::new(RefType::FUNCREF, 3, None), FuncRef::new(0).into())?;
    imports.define("host", "table", table);

    let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;
    let slot_is_null = instance.func::<i32, i32>(&store, "slot_is_null")?;

    assert_eq!(slot_is_null.call(&mut store, 0)?, 0);
    assert_eq!(slot_is_null.call(&mut store, 1)?, 0);

    Ok(())
}
