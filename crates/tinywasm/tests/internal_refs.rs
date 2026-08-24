#[cfg(feature = "guest-debug")]
use tinywasm::types::RefValue;
use tinywasm::types::WasmValue;
use tinywasm::{ExternItem, ModuleInstance, Store};

#[test]
#[cfg(feature = "guest-debug")]
fn private_items_are_accessible_by_index() -> Result<(), Box<dyn core::error::Error>> {
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
    let mut results = [WasmValue::I32(0)];
    func.call(&mut store, &[], &mut results)?;
    assert_eq!(results, [WasmValue::I32(7)]);

    instance.memory_by_index(0)?.copy_from_slice(&mut store, 0, &[1, 2, 3, 4])?;
    assert_eq!(instance.memory_by_index(0)?.read_vec(&store, 0, 4)?, &[1, 2, 3, 4]);

    assert_eq!(instance.table_by_index(0)?.size(&store)?, 2);
    let function = instance.func_by_index(&store, 0)?.as_func_ref(&store)?;
    assert_eq!(instance.table_by_index(0)?.get(&mut store, 0)?, WasmValue::Ref(RefValue::Func(function)));
    assert_eq!(instance.table_by_index(0)?.get(&mut store, 1)?, WasmValue::Ref(tinywasm::types::RefValue::Null));

    assert_eq!(instance.global_by_index(0)?.get(&mut store)?, WasmValue::I32(11));
    instance.global_by_index(0)?.set(&mut store, WasmValue::I32(23))?;
    assert_eq!(instance.global_by_index(0)?.get(&mut store)?, WasmValue::I32(23));

    Ok(())
}

#[test]
fn exported_tables_and_globals_have_handle_and_helper_apis() -> Result<(), Box<dyn core::error::Error>> {
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

    assert_eq!(instance.global_get(&mut store, "g")?, WasmValue::I32(3));
    assert_eq!(instance.global("g")?.get(&mut store)?, WasmValue::I32(3));
    instance.global_set(&mut store, "g", WasmValue::I32(9))?;
    assert_eq!(instance.global("g")?.get(&mut store)?, WasmValue::I32(9));

    let table = instance.table("t")?;
    assert_eq!(table.size(&store)?, 1);
    assert_eq!(table.get(&mut store, 0)?, WasmValue::Ref(tinywasm::types::RefValue::Null));

    let old_size = instance.table("t")?.grow(&mut store, 1, tinywasm::types::RefValue::Null.into())?;
    assert_eq!(old_size, Some(1));
    assert_eq!(instance.table("t")?.size(&store)?, 2);

    Ok(())
}

#[test]
fn extern_item_lookup_returns_expected_kinds() -> Result<(), Box<dyn core::error::Error>> {
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
fn extern_item_and_exports_use_actual_function_type() -> Result<(), Box<dyn core::error::Error>> {
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
        tinywasm::HostFunction::from(|_ctx: tinywasm::FuncContext<'_>, _arg: i64| Ok(())),
    );
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;

    let ExternItem::Func(func) = instance.extern_item("f")? else { panic!("expected function export") };
    func.call(&mut store, &[], &mut [])?;

    let (_, ExternItem::Func(func)) = instance.exports().find(|(name, _)| *name == "f").expect("export f not found")
    else {
        panic!("expected function export")
    };
    func.call(&mut store, &[], &mut [])?;

    Ok(())
}

#[test]
fn export_func_type_index_mismatch_fixture_would_break_old_lookup() -> Result<(), Box<dyn core::error::Error>> {
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
    let old_lookup_ty = module.types.get(export.index).and_then(|ty| ty.as_func()).expect("old lookup type missing");
    let func_ty = module
        .types
        .get(module.func_type_idxs[export.index as usize])
        .expect("export function type index should exist")
        .as_func()
        .expect("export function should reference a function type");

    assert_eq!(old_lookup_ty.params(), &[tinywasm::types::WasmType::I64]);
    assert_eq!(func_ty.params(), &[]);
    assert_ne!(old_lookup_ty.params(), func_ty.params());

    let mut store = Store::default();
    let mut imports = tinywasm::Imports::new();
    imports.define(
        "spectest",
        "print_i64",
        tinywasm::HostFunction::from(|_ctx: tinywasm::FuncContext<'_>, _arg: i64| Ok(())),
    );
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;

    let ExternItem::Func(func) = instance.extern_item("f")? else { panic!("expected function export") };
    func.call(&mut store, &[], &mut [])?;

    Ok(())
}

#[test]
fn start_resolves_module_func_index_to_store_addr() -> Result<(), Box<dyn core::error::Error>> {
    let wasm = wat::parse_str(
        r#"
        (module
          (global (export "g") (mut i32) (i32.const 0))
          (func $start
            i32.const 1
            global.set 0)
          (start $start)
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let _unused =
        tinywasm::HostFunction::from(|_ctx: tinywasm::FuncContext<'_>, (): ()| Ok(())).instantiate(&mut store)?;
    let instance = ModuleInstance::instantiate_no_start(&mut store, &module, None)?;

    instance.start(&mut store)?;
    assert_eq!(instance.global_get(&mut store, "g")?, WasmValue::I32(1));

    Ok(())
}
