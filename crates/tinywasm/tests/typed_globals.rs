use eyre::Result;
use tinywasm::types::{GlobalType, Instruction, RefValue, WasmType, WasmValue};
use tinywasm::{Global, Imports, ModuleInstance, Store};

#[test]
fn globals_use_typed_instructions_and_roundtrip_values() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (global (mut i32) (i32.const 1))
          (global (mut f32) (f32.const 2))
          (global (mut i64) (i64.const 3))
          (global (mut f64) (f64.const 4))
          (global (mut v128) (v128.const i32x4 1 2 3 4))
          (global (mut funcref) (ref.null func))

          (func (export "i32") (param i32) (result i32)
            local.get 0 global.set 0 global.get 0)
          (func (export "f32") (param f32) (result f32)
            local.get 0 global.set 1 global.get 1)
          (func (export "i64") (param i64) (result i64)
            local.get 0 global.set 2 global.get 2)
          (func (export "f64") (param f64) (result f64)
            local.get 0 global.set 3 global.get 3)
          (func (export "v128") (param v128) (result v128)
            local.get 0 global.set 4 global.get 4)
          (func (export "ref") (param funcref) (result funcref)
            local.get 0 global.set 5 global.get 5)
          (func (export "add-i32") (param i32) (result i32)
            local.get 0 global.get 0 i32.add)
          (func (export "add-i64") (param i64) (result i64)
            local.get 0 global.get 2 i64.add)
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;
    let instructions = module.funcs.iter().flat_map(|func| func.instructions.iter());
    let (mut get32, mut get64, mut get128, mut fused32, mut fused64) = (0, 0, 0, 0, 0);
    for instruction in instructions {
        match instruction {
            Instruction::GlobalGet32(_) => get32 += 1,
            Instruction::GlobalGet64(_) => get64 += 1,
            Instruction::GlobalGet128(_) => get128 += 1,
            Instruction::BinOpStackGlobal32(..) => fused32 += 1,
            Instruction::BinOpStackGlobal64(..) => fused64 += 1,
            _ => {}
        }
    }
    assert_eq!((get32, get64, get128), (3, 2, 1));
    assert_eq!((fused32, fused64), (1, 1));

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let cases = [
        ("i32", WasmValue::I32(11)),
        ("f32", WasmValue::F32(12.5)),
        ("i64", WasmValue::I64(13)),
        ("f64", WasmValue::F64(14.5)),
        ("v128", WasmValue::V128([15; 16])),
        ("ref", WasmValue::Ref(RefValue::Null)),
    ];
    for (name, value) in cases {
        assert_eq!(instance.func_untyped(&store, name)?.call(&mut store, &[value])?, vec![value]);
    }
    assert_eq!(instance.func::<i32, i32>(&store, "add-i32")?.call(&mut store, 2)?, 13);
    assert_eq!(instance.func::<i64, i64>(&store, "add-i64")?.call(&mut store, 2)?, 15);

    Ok(())
}

#[test]
fn imported_global_keeps_its_typed_store_address() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (import "env" "g" (global (mut i64)))
          (func (export "roundtrip") (param i64) (result i64)
            local.get 0 global.set 0 global.get 0)
        )
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let global = Global::new(&mut store, GlobalType::new(WasmType::I64, true), WasmValue::I64(1))?;
    let mut imports = Imports::default();
    imports.define("env", "g", global);
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;

    assert_eq!(instance.func::<i64, i64>(&store, "roundtrip")?.call(&mut store, 42)?, 42);
    assert_eq!(global.get(&store)?, WasmValue::I64(42));
    Ok(())
}
