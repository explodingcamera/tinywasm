use eyre::Result;
use tinywasm::types::ValType;
use tinywasm::{ExportType, ImportType, Module};

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

    let module = Module::parse_bytes(&wasm)?;

    let imports: Vec<_> = module.imports().collect();
    assert_eq!(imports.len(), 2);

    let ifunc_import = imports.iter().find(|import| import.name == "ifunc").expect("ifunc import not found");
    match ifunc_import.ty {
        ImportType::Func(ty) => {
            assert_eq!(ty.params.as_ref(), &[ValType::I32]);
            assert_eq!(ty.results.as_ref(), &[ValType::I32]);
        }
        _ => panic!("ifunc import should be a function type"),
    }

    let iglobal_import = imports.iter().find(|import| import.name == "iglobal").expect("iglobal import not found");
    match iglobal_import.ty {
        ImportType::Global(ty) => {
            assert!(ty.mutable);
            assert_eq!(ty.ty, ValType::I32);
        }
        _ => panic!("iglobal import should be a global type"),
    }

    let exports: Vec<_> = module.exports().collect();
    assert_eq!(exports.len(), 4);

    let ifunc_export = exports.iter().find(|export| export.name == "ifunc_export").expect("ifunc export not found");
    match ifunc_export.ty {
        ExportType::Func(ty) => {
            assert_eq!(ty.params.as_ref(), &[ValType::I32]);
            assert_eq!(ty.results.as_ref(), &[ValType::I32]);
        }
        _ => panic!("ifunc export should resolve to imported function type"),
    }

    let iglobal_export =
        exports.iter().find(|export| export.name == "iglobal_export").expect("iglobal export not found");
    match iglobal_export.ty {
        ExportType::Global(ty) => {
            assert!(ty.mutable);
            assert_eq!(ty.ty, ValType::I32);
        }
        _ => panic!("iglobal export should resolve to imported global type"),
    }

    let lfunc_export = exports.iter().find(|export| export.name == "lfunc_export").expect("lfunc export not found");
    match lfunc_export.ty {
        ExportType::Func(ty) => {
            assert_eq!(ty.params.as_ref(), &[ValType::I64]);
            assert_eq!(ty.results.as_ref(), &[ValType::I64]);
        }
        _ => panic!("lfunc export should resolve to local function type"),
    }

    let lglobal_export =
        exports.iter().find(|export| export.name == "lglobal_export").expect("lglobal export not found");
    match lglobal_export.ty {
        ExportType::Global(ty) => {
            assert!(!ty.mutable);
            assert_eq!(ty.ty, ValType::I64);
        }
        _ => panic!("lglobal export should resolve to local global type"),
    }

    Ok(())
}
