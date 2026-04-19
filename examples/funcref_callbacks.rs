use eyre::Result;
use tinywasm::{FuncContext, HostFunction, Imports, ModuleInstance, Store, types::FuncRef};

const LHS: i32 = 5;
const RHS: i32 = 3;

fn main() -> Result<()> {
    run_passed_funcref_example()?;
    run_returned_funcref_example()?;
    Ok(())
}

fn run_passed_funcref_example() -> Result<()> {
    // Host receives funcref and calls it via an exported proxy.
    const WASM: &str = r#"
    (module
        (import "host" "call_this" (func $host_callback_caller (param funcref)))
        (import "host" "mul" (func $host_mul (param $x i32) (param $y i32) (result i32)))
      
        (func $tell_host_to_call (export "tell_host_to_call")
            (call $host_callback_caller (ref.func $add))
            (call $host_callback_caller (ref.func $sub))
            (call $host_callback_caller (ref.func $host_mul))
        )
      
        (type $binop (func (param i32 i32) (result i32)))
        
        (table 3 funcref)
        (elem (i32.const 0) $add $sub $host_mul)
        (func $add (param $x i32) (param $y i32) (result i32)
            local.get $x
            local.get $y
            i32.add
        )
        (func $sub (param $x i32) (param $y i32) (result i32)
            local.get $x
            local.get $y
            i32.sub
        )

        (table $callback_register 1 funcref)
        (func (export "call_binop_by_ref") (param funcref i32 i32) (result i32)
            (table.set $callback_register (i32.const 0) (local.get 0))
            (call_indirect $callback_register (type $binop) (local.get 1)(local.get 2)(i32.const 0))
        )
    )
    "#;

    let wasm = wat::parse_str(WASM).expect("failed to parse wat");
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();

    let mul = HostFunction::from(&mut store, |_, (lhs, rhs): (i32, i32)| -> tinywasm::Result<i32> { Ok(lhs * rhs) });
    let call_this =
        HostFunction::from(&mut store, |mut ctx: FuncContext<'_>, func_ref: FuncRef| -> tinywasm::Result<()> {
            // Host cannot call a funcref directly, so it routes through Wasm.
            let call_by_ref = ctx.module().func::<(FuncRef, i32, i32), i32>(ctx.store(), "call_binop_by_ref")?;
            let _result = call_by_ref.call(ctx.store_mut(), (func_ref, LHS, RHS))?;
            Ok(())
        });

    let mut imports = Imports::new();
    imports.define("host", "call_this", call_this).define("host", "mul", mul);

    let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;
    let caller = instance.func::<(), ()>(&store, "tell_host_to_call")?;

    caller.call(&mut store, ())?;

    Ok(())
}

fn run_returned_funcref_example() -> Result<()> {
    // Wasm returns funcref values, host executes them through the same proxy.
    const WASM: &str = r#"
    (module
        (import "host" "mul" (func $host_mul (param $x i32) (param $y i32) (result i32)))
        (type $binop (func (param i32 i32) (result i32)))
        (table 3 funcref)
        (elem (i32.const 0) $add $sub $host_mul)
        (func $add (param $x i32) (param $y i32) (result i32)
            local.get $x
            local.get $y
            i32.add
        )
        (func $sub (param $x i32) (param $y i32) (result i32)
            local.get $x
            local.get $y
            i32.sub
        )
        (func $ref_to_funcs (export "what_should_host_call")  (result funcref funcref funcref)
            (ref.func $add)
            (ref.func $sub)
            (ref.func $host_mul)
        )

        (table $callback_register 1 funcref)
        (func $call (export "call_binop_by_ref") (param funcref i32 i32) (result i32)
            (table.set $callback_register (i32.const 0) (local.get 0))
            (call_indirect $callback_register (type $binop) (local.get 1)(local.get 2)(i32.const 0))
        )
    )
    "#;

    let wasm = wat::parse_str(WASM).expect("failed to parse wat");
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let mut imports = Imports::new();

    let mul = HostFunction::from(&mut store, |_, (lhs, rhs): (i32, i32)| -> tinywasm::Result<i32> { Ok(lhs * rhs) });
    imports.define("host", "mul", mul);

    let instance = ModuleInstance::instantiate(&mut store, &module, Some(imports))?;
    let (add_ref, sub_ref, mul_ref) = {
        let get_funcrefs = instance.func::<(), (FuncRef, FuncRef, FuncRef)>(&store, "what_should_host_call")?;
        get_funcrefs.call(&mut store, ())?
    };

    let call_by_ref = instance.func::<(FuncRef, i32, i32), i32>(&store, "call_binop_by_ref")?;
    for func_ref in [add_ref, sub_ref, mul_ref] {
        let _result = call_by_ref.call(&mut store, (func_ref, LHS, RHS))?;
    }
    Ok(())
}
