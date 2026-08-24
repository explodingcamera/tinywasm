use tinywasm::types::{RefType, TableType};
use tinywasm::{HostFunction, Imports, ModuleInstance, Store, Table};

#[test]
fn imported_table_uses_provided_init_value() -> Result<(), Box<dyn core::error::Error>> {
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
    let function_at_zero = HostFunction::from(|_, ()| Ok(())).instantiate(&mut store)?;
    let init = function_at_zero.as_func_ref(&store)?.into();
    let table = Table::try_new(&mut store, TableType::new(RefType::FUNCREF, 3, None), init)?;
    imports.define("host", "table", table);

    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    let slot_is_null = instance.func::<i32, i32>(&store, "slot_is_null")?;

    assert_eq!(slot_is_null.call(&mut store, 0)?, 0);
    assert_eq!(slot_is_null.call(&mut store, 1)?, 0);

    Ok(())
}
