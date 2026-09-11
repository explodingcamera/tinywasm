use tinywasm::parser::Parser;
use tinywasm::types::{CmpOp, Instruction};
use tinywasm::{ExecProgress, ModuleInstance, Store};

fn parse(wat: &str) -> Result<tinywasm::types::Module, Box<dyn core::error::Error>> {
    let wasm = wat::parse_str(wat)?;
    Parser::default().parse_module_bytes(&wasm).map_err(Into::into)
}

#[test]
fn uses_reference_accumulator_for_direct_return() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (func (export "id") (param externref) (result externref)
            local.get 0))
        "#,
    )?;

    assert_eq!(module.funcs[0].instructions.as_ref(), [Instruction::AccRefLocalGet(0), Instruction::ReturnAccRef]);
    Ok(())
}

#[test]
fn terminal_paths_retire_live_accumulators() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (func (export "return-i32") (result i32)
            ref.null extern
            i64.const 0
            i64.clz
            i32.const 0
            i32.eqz
            return)
          (func (result externref)
            i32.const 0
            i32.eqz
            i64.const 0
            i64.clz
            ref.null extern
            return)
          (func
            ref.null extern
            i32.const 0
            i32.eqz
            i64.const 0
            i64.clz
            unreachable))
        "#,
    )?;

    let return_i32 = module.funcs[0].instructions.as_ref();
    assert!(return_i32.windows(2).any(|pair| pair == [Instruction::ClearAccRef, Instruction::ReturnAcc32]));
    assert!(module.funcs[1].instructions.contains(&Instruction::ReturnAccRef));
    assert!(module.funcs[2].instructions.contains(&Instruction::Unreachable));

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<(), i32>(&store, "return-i32")?.call(&mut store, ())?, 1);
    Ok(())
}

#[test]
fn reference_accumulator_handles_null_local_and_global_values() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (global $value (mut externref) (ref.null extern))
          (func (export "check") (result i32) (local externref)
            ref.null extern
            local.tee 0
            global.set $value
            global.get $value
            local.set 0
            local.get 0
            ref.is_null))
        "#,
    )?;

    let instructions = module.funcs[0].instructions.as_ref();
    assert!(instructions.contains(&Instruction::AccRefNull));
    assert!(instructions.contains(&Instruction::AccRefLocalTee(0)));
    assert!(instructions.contains(&Instruction::AccRefGlobalSet(0)));
    assert!(instructions.contains(&Instruction::AccRefGlobalGet(0)));
    assert!(instructions.contains(&Instruction::AccRefLocalSet(0)));
    assert!(instructions.contains(&Instruction::AccRefLocalGet(0)));
    assert!(instructions.contains(&Instruction::AccRefIsNull));

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<(), i32>(&store, "check")?.call(&mut store, ())?, 1);
    Ok(())
}

#[test]
fn resumable_execution_preserves_a_live_reference_accumulator() -> Result<(), Box<dyn core::error::Error>> {
    let mut body = String::from("(module (func (export \"check\") (result i32) ref.null extern i64.const 0 ");
    for _ in 0..200 {
        body.push_str("i64.const 1 i64.add ");
    }
    body.push_str("drop ref.is_null))");
    let module = parse(&body)?;

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let function = instance.func::<(), i32>(&store, "check")?;
    let mut execution = function.call_resumable(&mut store, ())?;
    let mut suspended = false;
    let result = loop {
        match execution.resume_with_fuel(128)? {
            ExecProgress::Completed(value) => break value,
            ExecProgress::Suspended => suspended = true,
        }
    };
    assert!(suspended);
    assert_eq!(result, 1);
    Ok(())
}

#[test]
fn local_destinations_consume_or_retain_the_register() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (func (export "set") (param i32) (result i32) (local i32)
            local.get 0
            i32.const 1
            i32.add
            local.tee 1
            local.set 0
            local.get 1))
        "#,
    )?;

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<i32, i32>(&store, "set")?.call(&mut store, 4)?, 5);
    Ok(())
}

#[test]
fn selects_self_calls_during_streaming_lowering() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (func $recurse (export "recurse") (param i32)
            local.get 0
            if
              local.get 0
              i32.const 1
              i32.sub
              call $recurse
            end))
        "#,
    )?;

    assert!(module.funcs[0].instructions.contains(&Instruction::CallSelf));
    Ok(())
}

#[test]
fn local_writes_preserve_earlier_deferred_reads() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (func (export "tee") (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.const 1
            i32.add
            local.tee 0
            i32.add)
          (func (export "set") (param i32 i32) (result i32)
            local.get 0
            local.get 1
            i32.const 1
            i32.add
            local.set 0))
        "#,
    )?;

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<(i32, i32), i32>(&store, "tee")?.call(&mut store, (10, 2))?, 13);
    assert_eq!(instance.func::<(i32, i32), i32>(&store, "set")?.call(&mut store, (10, 2))?, 10);
    Ok(())
}

#[test]
fn resumable_execution_preserves_a_live_register() -> Result<(), Box<dyn core::error::Error>> {
    let mut body = String::from("(module (func (export \"sum\") (result i32) i32.const 0 ");
    for _ in 0..200 {
        body.push_str("i32.const 1 i32.add ");
    }
    body.push_str("))");
    let module = parse(&body)?;

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let function = instance.func::<(), i32>(&store, "sum")?;
    let mut execution = function.call_resumable(&mut store, ())?;
    let mut suspended = false;
    let result = loop {
        match execution.resume_with_fuel(128)? {
            ExecProgress::Completed(value) => break value,
            ExecProgress::Suspended => suspended = true,
        }
    };
    assert!(suspended);
    assert_eq!(result, 200);
    Ok(())
}

#[test]
fn register_lowering_preserves_control_flow() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (func (export "choose") (param i32) (result i32)
            local.get 0
            if (result i32)
              i32.const 3
            else
              i32.const 4
            end)
          (func (export "less") (param i32 i32) (result i32)
            local.get 0 local.get 1 i32.lt_s
            if (result i32)
              i32.const 1
            else
              i32.const 0
            end))
        "#,
    )?;
    assert!(module.funcs[1].instructions.iter().any(|instruction| {
        matches!(instruction, Instruction::JumpCmpLocalLocal32(packed) if packed.op == CmpOp::GeS)
    }));
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<i32, i32>(&store, "choose")?.call(&mut store, 1)?, 3);
    assert_eq!(instance.func::<i32, i32>(&store, "choose")?.call(&mut store, 0)?, 4);
    assert_eq!(instance.func::<(i32, i32), i32>(&store, "less")?.call(&mut store, (2, 3))?, 1);
    assert_eq!(instance.func::<(i32, i32), i32>(&store, "less")?.call(&mut store, (3, 2))?, 0);
    Ok(())
}

#[test]
fn accumulator_branch_does_not_consume_materialized_branch_values() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (func (export "choose") (param i32 i32) (result i32)
            block (result i32)
              local.get 0
              local.get 1
              i32.const 8
              i32.eq
              br_if 0
              drop
              i32.const 0
            end))
        "#,
    )?;

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let choose = instance.func::<(i32, i32), i32>(&store, "choose")?;
    assert_eq!(choose.call(&mut store, (7, 8))?, 7);
    assert_eq!(choose.call(&mut store, (7, 9))?, 0);
    Ok(())
}

#[test]
fn accumulator_local_update_compare_branches_are_correct() -> Result<(), Box<dyn core::error::Error>> {
    let module = parse(
        r#"
        (module
          (func (export "direct") (param $limit i32) (result i32) (local $i i32)
            loop $loop
              local.get $i
              i32.const 1
              i32.add
              local.tee $i
              local.get $limit
              i32.lt_u
              br_if $loop
            end
            local.get $i)
          (func (export "inverted") (param $limit i32) (result i32) (local $i i32)
            loop $loop
              local.get $i
              i32.const 1
              i32.add
              local.tee $i
              local.get $limit
              i32.lt_u
              if
                br $loop
              end
            end
            local.get $i))
        "#,
    )?;

    assert!(module.funcs[0].instructions.iter().any(
        |instruction| matches!(instruction, Instruction::IncLocalJumpCmpLocal32(packed) if packed.op == CmpOp::LtU)
    ));
    assert!(module.funcs[1].instructions.iter().any(
        |instruction| matches!(instruction, Instruction::IncLocalJumpCmpLocal32(packed) if packed.op == CmpOp::GeU)
    ));

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<i32, i32>(&store, "direct")?.call(&mut store, 5)?, 5);
    assert_eq!(instance.func::<i32, i32>(&store, "inverted")?.call(&mut store, 5)?, 5);
    Ok(())
}

#[test]
fn register_lowering_matches_fibonacci_application() -> Result<(), Box<dyn core::error::Error>> {
    let wasm = include_bytes!("../../../examples/rust/out/fibonacci.wasm");
    let module = Parser::default().parse_module_bytes(wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<i32, i32>(&store, "fibonacci")?.call(&mut store, 20)?, 6765);
    assert_eq!(instance.func::<i32, i32>(&store, "fibonacci_recursive")?.call(&mut store, 20)?, 6765);
    Ok(())
}
