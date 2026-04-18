use eyre::Result;
use tinywasm::types::{FuncRef, WasmValue};
use tinywasm::{ExternItem, Module, Store};

#[test]
#[cfg(feature = "guest_debug")]
fn private_items_are_accessible_by_index() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (func (result i32)
            i32.const 7)
          (memory 1)
          (global (mut i32) (i32.const 11))
          (table 2 funcref)
          (elem (i32.const 0) func 0)
        )
        "#,
    )?;

    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = module.instantiate(&mut store, None)?;

    let func = instance.func_by_index(&store, 0)?;
    assert_eq!(func.call(&mut store, &[])?, vec![WasmValue::I32(7)]);

    instance.memory_by_index(0)?.copy_from_slice(&mut store, 0, &[1, 2, 3, 4])?;
    assert_eq!(instance.memory_by_index(0)?.read_vec(&store, 0, 4)?, &[1, 2, 3, 4]);

    assert_eq!(instance.table_by_index(0)?.size(&store)?, 2);
    assert_eq!(instance.table_by_index(0)?.get(&store, 0)?, WasmValue::RefFunc(FuncRef::new(Some(0))));
    assert!(matches!(instance.table_by_index(0)?.get(&store, 1)?, WasmValue::RefFunc(func_ref) if func_ref.is_null()));

    assert_eq!(instance.global_by_index(0)?.get(&store)?, WasmValue::I32(11));
    instance.global_by_index(0)?.set(&mut store, WasmValue::I32(23))?;
    assert_eq!(instance.global_by_index(0)?.get(&store)?, WasmValue::I32(23));

    Ok(())
}

#[test]
fn exported_tables_and_globals_have_handle_and_helper_apis() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (global (export "g") (mut i32) (i32.const 3))
          (table (export "t") 1 funcref)
        )
        "#,
    )?;

    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = module.instantiate(&mut store, None)?;

    assert_eq!(instance.global_get(&store, "g")?, WasmValue::I32(3));
    assert_eq!(instance.global("g")?.get(&store)?, WasmValue::I32(3));
    instance.global_set(&mut store, "g", WasmValue::I32(9))?;
    assert_eq!(instance.global("g")?.get(&store)?, WasmValue::I32(9));

    let table = instance.table("t")?;
    assert_eq!(table.size(&store)?, 1);
    assert!(matches!(table.get(&store, 0)?, WasmValue::RefFunc(func_ref) if func_ref.is_null()));

    let old_size = instance.table("t")?.grow(&mut store, 1, WasmValue::RefFunc(FuncRef::null()))?;
    assert_eq!(old_size, 1);
    assert_eq!(instance.table("t")?.size(&store)?, 2);

    Ok(())
}

#[test]
fn extern_item_lookup_returns_expected_kinds() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (func (export "f") (result i32) i32.const 1)
          (memory (export "m") 1)
          (table (export "t") 1 funcref)
          (global (export "g") (mut i32) (i32.const 5))
        )
        "#,
    )?;

    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = module.instantiate(&mut store, None)?;

    assert!(matches!(instance.extern_item("f")?, ExternItem::Func(_)));
    assert!(matches!(instance.extern_item("m")?, ExternItem::Memory(_)));
    assert!(matches!(instance.extern_item("t")?, ExternItem::Table(_)));
    assert!(matches!(instance.extern_item("g")?, ExternItem::Global(_)));

    Ok(())
}
