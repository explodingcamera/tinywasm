use eyre::Result;
use tinywasm::types::WasmType;
use tinywasm_types::{ExportType, ImportType};

#[test]
fn module_descriptors_resolve_imported_and_local_export_types() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (type $t0 (func (param i32) (result i32)))
          (import "host" "ifunc" (func $ifunc (type $t0)))
          (import "host" "iglobal" (global $iglobal (mut i32)))

          (func $lfunc (param i64) (result i64)
            local.get 0)
          (global $lglobal i64 (i64.const 9))

          (export "ifunc_export" (func $ifunc))
          (export "iglobal_export" (global $iglobal))
          (export "lfunc_export" (func $lfunc))
          (export "lglobal_export" (global $lglobal))
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;

    let imports: Vec<_> = module.imports().collect();
    assert_eq!(imports.len(), 2);

    let ifunc_import = imports.iter().find(|import| import.name == "ifunc").expect("ifunc import not found");
    match ifunc_import.ty {
        ImportType::Func(ty) => {
            assert_eq!(ty.params(), &[WasmType::I32]);
            assert_eq!(ty.results(), &[WasmType::I32]);
        }
        _ => panic!("ifunc import should be a function type"),
    }

    let iglobal_import = imports.iter().find(|import| import.name == "iglobal").expect("iglobal import not found");
    match iglobal_import.ty {
        ImportType::Global(ty) => {
            assert!(ty.mutable);
            assert_eq!(ty.ty, WasmType::I32);
        }
        _ => panic!("iglobal import should be a global type"),
    }

    let exports: Vec<_> = module.exports().collect();
    assert_eq!(exports.len(), 4);

    let ifunc_export = exports.iter().find(|export| export.name == "ifunc_export").expect("ifunc export not found");
    match ifunc_export.ty {
        ExportType::Func(ty) => {
            assert_eq!(ty.params(), &[WasmType::I32]);
            assert_eq!(ty.results(), &[WasmType::I32]);
        }
        _ => panic!("ifunc export should resolve to imported function type"),
    }

    let iglobal_export =
        exports.iter().find(|export| export.name == "iglobal_export").expect("iglobal export not found");
    match iglobal_export.ty {
        ExportType::Global(ty) => {
            assert!(ty.mutable);
            assert_eq!(ty.ty, WasmType::I32);
        }
        _ => panic!("iglobal export should resolve to imported global type"),
    }

    let lfunc_export = exports.iter().find(|export| export.name == "lfunc_export").expect("lfunc export not found");
    match lfunc_export.ty {
        ExportType::Func(ty) => {
            assert_eq!(ty.params(), &[WasmType::I64]);
            assert_eq!(ty.results(), &[WasmType::I64]);
        }
        _ => panic!("lfunc export should resolve to local function type"),
    }

    let lglobal_export =
        exports.iter().find(|export| export.name == "lglobal_export").expect("lglobal export not found");
    match lglobal_export.ty {
        ExportType::Global(ty) => {
            assert!(!ty.mutable);
            assert_eq!(ty.ty, WasmType::I64);
        }
        _ => panic!("lglobal export should resolve to local global type"),
    }

    Ok(())
}

#[test]
fn module_descriptors_resolve_imported_and_local_table_and_memory_exports() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (import "host" "itable" (table 2 4 funcref))
          (import "host" "imemory" (memory 1 3))
          (table $ltable 5 7 funcref)
          (memory $lmemory 2 6)
          (export "itable_export" (table 0))
          (export "imemory_export" (memory 0))
          (export "ltable_export" (table 1))
          (export "lmemory_export" (memory 1))
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;
    let exports: Vec<_> = module.exports().collect();

    let itable_export = exports.iter().find(|export| export.name == "itable_export").expect("itable export not found");
    match itable_export.ty {
        ExportType::Table(ty) => {
            assert_eq!(ty.element_type, WasmType::RefFunc);
            assert_eq!(ty.size_initial, 2);
            assert_eq!(ty.size_max, Some(4));
        }
        _ => panic!("itable export should resolve to imported table type"),
    }

    let imemory_export =
        exports.iter().find(|export| export.name == "imemory_export").expect("imemory export not found");
    match imemory_export.ty {
        ExportType::Memory(ty) => {
            assert_eq!(ty.page_count_initial(), 1);
            assert_eq!(ty.page_count_max(), 3);
        }
        _ => panic!("imemory export should resolve to imported memory type"),
    }

    let ltable_export = exports.iter().find(|export| export.name == "ltable_export").expect("ltable export not found");
    match ltable_export.ty {
        ExportType::Table(ty) => {
            assert_eq!(ty.element_type, WasmType::RefFunc);
            assert_eq!(ty.size_initial, 5);
            assert_eq!(ty.size_max, Some(7));
        }
        _ => panic!("ltable export should resolve to local table type"),
    }

    let lmemory_export =
        exports.iter().find(|export| export.name == "lmemory_export").expect("lmemory export not found");
    match lmemory_export.ty {
        ExportType::Memory(ty) => {
            assert_eq!(ty.page_count_initial(), 2);
            assert_eq!(ty.page_count_max(), 6);
        }
        _ => panic!("lmemory export should resolve to local memory type"),
    }

    Ok(())
}
