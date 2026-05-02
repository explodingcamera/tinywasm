use eyre::Result;
use tinywasm::types::{FuncRef, WasmValue};
use tinywasm::{ExternItem, ModuleInstance, Store};

#[test]
#[cfg(feature = "guest-debug")]
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

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

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

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

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

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

    assert!(matches!(instance.extern_item("f")?, ExternItem::Func(_)));
    assert!(matches!(instance.extern_item("m")?, ExternItem::Memory(_)));
    assert!(matches!(instance.extern_item("t")?, ExternItem::Table(_)));
    assert!(matches!(instance.extern_item("g")?, ExternItem::Global(_)));

    Ok(())
}

#[test]
fn extern_item_and_exports_use_actual_function_type() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (type $local_ty (func))
          (type $import_ty (func (param i64)))
          (import "host" "imported" (func (type $import_ty)))
          (func (export "f") (type $local_ty)
            nop)
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let mut imports = tinywasm::Imports::new();
    imports.define(
        "host",
        "imported",
        tinywasm::HostFunction::from(&mut store, |_ctx: tinywasm::FuncContext<'_>, _arg: i64| Ok(())),
    );
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;

    let ExternItem::Func(func) = instance.extern_item("f")? else { panic!("expected function export") };
    assert_eq!(func.call(&mut store, &[])?, vec![]);

    let (_, ExternItem::Func(func)) = instance.exports().find(|(name, _)| *name == "f").expect("export f not found")
    else {
        panic!("expected function export")
    };
    assert_eq!(func.call(&mut store, &[])?, vec![]);

    Ok(())
}

#[test]
fn export_func_type_index_mismatch_fixture_would_break_old_lookup() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (type $local_ty (func))
          (type $import_ty (func (param i64)))
          (import "spectest" "print_i64" (func (type $import_ty)))
          (func (export "f") (type $local_ty)
            nop)
        )
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;

    let export = module.exports.iter().find(|export| export.name.as_ref() == "f").expect("export f not found");
    let old_lookup_ty = module.func_types.get(export.index as usize).expect("old lookup type missing");

    assert_eq!(old_lookup_ty.params(), &[tinywasm::types::WasmType::I64]);
    assert_eq!(module.funcs[0].ty.params(), &[]);
    assert_ne!(old_lookup_ty.params(), module.funcs[0].ty.params());

    let mut store = Store::default();
    let mut imports = tinywasm::Imports::new();
    imports.define(
        "spectest",
        "print_i64",
        tinywasm::HostFunction::from(&mut store, |_ctx: tinywasm::FuncContext<'_>, _arg: i64| Ok(())),
    );
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;

    let ExternItem::Func(func) = instance.extern_item("f")? else { panic!("expected function export") };
    assert_eq!(func.call(&mut store, &[])?, vec![]);

    Ok(())
}

#[test]
fn start_prefers_exported_start_without_re_resolving_store_addr() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (global (export "g") (mut i32) (i32.const 0))
          (func (export "_start")
            i32.const 1
            global.set 0)
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let _unused = tinywasm::HostFunction::from(&mut store, |_ctx: tinywasm::FuncContext<'_>, (): ()| Ok(()));
    let instance = ModuleInstance::instantiate_no_start(&mut store, &module, None)?;

    instance.start(&mut store)?;
    assert_eq!(instance.global_get(&store, "g")?, WasmValue::I32(1));

    Ok(())
}
