use eyre::{self, bail};
use tinywasm::{
    types::{FuncType, ValType, WasmValue},
    CoroState, CoroStateResumeResult, Extern, FuncContext, HostCoroState, Imports, Module, PotentialCoroCallResult,
    Store, SuspendReason,
};
use wat;

fn main() -> eyre::Result<()> {
    untyped()?;
    typed()?;
    Ok(())
}

const WASM: &str = r#"(module
    (import "host" "hello" (func $host_hello (param i32)))
    (import "host" "wait" (func $host_suspend (param i32)(result i32)))
    
    (func (export "call_hello") (result f32)
        (call $host_hello (i32.const -3))
        (call $host_suspend (i32.const 10))
        (call $host_hello)
        (f32.const 6.28)
    )
)
"#;

#[derive(Debug)]
struct MyUserData {
    magic: u16,
}

#[derive(Debug)]
struct MySuspendedState {
    base: i32,
}
impl<'a> CoroState<Vec<WasmValue>, FuncContext<'a>> for MySuspendedState {
    fn resume(
        &mut self,
        _: FuncContext<'a>,
        arg: tinywasm::types::ResumeArgument,
    ) -> tinywasm::Result<tinywasm::CoroStateResumeResult<Vec<WasmValue>>> {
        let val = arg.expect("you din't send").downcast::<i32>().expect("you sent wrong");
        return Ok(CoroStateResumeResult::Return(vec![WasmValue::I32(*val + self.base)]));
    }
}

fn untyped() -> eyre::Result<()> {
    let wasm = wat::parse_str(WASM).expect("failed to parse wat");
    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();

    let mut imports = Imports::new();
    imports.define(
        "host",
        "hello",
        Extern::func(&FuncType { params: Box::new([ValType::I32]), results: Box::new([]) }, |_: FuncContext<'_>, x| {
            x.first().map(|x| println!("{:?}", x));
            Ok(vec![])
        }),
    )?;
    let my_coro_starter = |_ctx: FuncContext<'_>,
                           vals: &[WasmValue]|
     -> tinywasm::Result<PotentialCoroCallResult<Vec<WasmValue>, Box<dyn HostCoroState>>> {
        let base = if let WasmValue::I32(v) = vals.first().expect("wrong args") { v } else { panic!("wrong arg") };
        let coro = Box::new(MySuspendedState { base: *base });
        return Ok(PotentialCoroCallResult::Suspended(
            SuspendReason::make_yield::<MyUserData>(MyUserData { magic: 42 }),
            coro,
        ));
    };
    imports.define(
        "host",
        "wait",
        Extern::func_coro(
            &FuncType { params: Box::new([ValType::I32]), results: Box::new([ValType::I32]) },
            my_coro_starter,
        ),
    )?;

    let instance = module.instantiate(&mut store, Some(imports))?;

    let greeter = instance.exported_func_untyped(&store, "call_hello")?;
    let call_res = greeter.call_coro(&mut store, &[])?;
    let mut resumable = match call_res {
        tinywasm::PotentialCoroCallResult::Return(..) => bail!("it's not supposed to return yet"),
        tinywasm::PotentialCoroCallResult::Suspended(SuspendReason::Yield(Some(val)), resumable) => {
            match val.downcast::<MyUserData>() {
                Ok(val) => assert_eq!(val.magic, 42),
                Err(_) => bail!("invalid yielded val"),
            }
            resumable
        }
        tinywasm::PotentialCoroCallResult::Suspended(..) => bail!("wrong suspend"),
    };

    let final_res = resumable.resume(&mut store, Some(Box::<i32>::new(7)))?;
    if let CoroStateResumeResult::Return(vals) = final_res {
        println!("{:?}", vals.first().unwrap());
    } else {
        panic!("should have finished");
    }

    Ok(())
}

fn typed() -> eyre::Result<()> {
    let wasm = wat::parse_str(WASM).expect("failed to parse wat");
    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();

    let mut imports = Imports::new();
    imports.define(
        "host",
        "hello",
        Extern::typed_func(|_: FuncContext<'_>, x: i32| {
            println!("{x}");
            Ok(())
        }),
    )?;
    let my_coro_starter =
        |_ctx: FuncContext<'_>, base: i32| -> tinywasm::Result<PotentialCoroCallResult<i32, Box<dyn HostCoroState>>> {
            let coro = Box::new(MySuspendedState { base: base });
            return Ok(PotentialCoroCallResult::Suspended(
                SuspendReason::make_yield::<MyUserData>(MyUserData { magic: 42 }),
                coro,
            ));
        };
    imports.define("host", "wait", Extern::typed_func_coro(my_coro_starter))?;

    let instance = module.instantiate(&mut store, Some(imports))?;

    let greeter = instance.exported_func::<(), f32>(&store, "call_hello")?;
    let call_res = greeter.call_coro(&mut store, ())?;
    let mut resumable = match call_res {
        tinywasm::PotentialCoroCallResult::Return(..) => bail!("it's not supposed to return yet"),
        tinywasm::PotentialCoroCallResult::Suspended(SuspendReason::Yield(Some(val)), resumable) => {
            match val.downcast::<MyUserData>() {
                Ok(val) => assert_eq!(val.magic, 42),
                Err(_) => bail!("invalid yielded val"),
            }
            resumable
        }
        tinywasm::PotentialCoroCallResult::Suspended(..) => bail!("wrong suspend"),
    };

    let final_res = resumable.resume(&mut store, Some(Box::<i32>::new(7)))?;

    if let CoroStateResumeResult::Return(res) = final_res {
        println!("{res}");
    } else {
        panic!("should have returned");
    }

    Ok(())
}
