use core::panic;
use eyre;
use std::sync;
use std::{ops::ControlFlow, time::Duration};
use tinywasm::{
    CoroState, CoroStateResumeResult, Extern, Imports, Module, ModuleInstance, PotentialCoroCallResult, Store,
    SuspendConditions, SuspendReason,
};
use wat;

#[test]
fn main() -> std::result::Result<(), eyre::Report> {
    println!("\n# testing with callback");
    let mut cb_cond = |store: &mut Store| {
        let callback = make_suspend_in_time_cb(30);
        store.set_suspend_conditions(SuspendConditions::new().with_suspend_callback(Box::new(callback)));
    };
    suspend_with_pure_loop(&mut cb_cond, SuspendReason::SuspendedCallback)?;
    suspend_with_wasm_fn(&mut cb_cond, SuspendReason::SuspendedCallback)?;
    suspend_with_host_fn(&mut cb_cond, SuspendReason::SuspendedCallback)?;

    println!("\n# testing with epoch");
    let mut time_cond = |store: &mut Store| {
        store.set_suspend_conditions(SuspendConditions::new().with_timeout_in(Duration::from_millis(10)))
    };
    suspend_with_pure_loop(&mut time_cond, SuspendReason::SuspendedEpoch)?;
    suspend_with_wasm_fn(&mut time_cond, SuspendReason::SuspendedEpoch)?;
    suspend_with_host_fn(&mut time_cond, SuspendReason::SuspendedEpoch)?;

    println!("\n# testing atomic bool");
    let mut cb_thead = |store: &mut Store| {
        let arc = sync::Arc::<sync::atomic::AtomicBool>::new(sync::atomic::AtomicBool::new(false));
        store.set_suspend_conditions(SuspendConditions::new().with_suspend_flag(arc.clone()));
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            arc.store(true, sync::atomic::Ordering::Release);
        });
        drop(handle);
    };
    suspend_with_pure_loop(&mut cb_thead, SuspendReason::SuspendedFlag)?;
    suspend_with_wasm_fn(&mut cb_thead, SuspendReason::SuspendedFlag)?;
    suspend_with_host_fn(&mut cb_thead, SuspendReason::SuspendedFlag)?;

    Ok(())
}

fn make_suspend_in_time_cb(milis: u64) -> impl FnMut(&Store) -> ControlFlow<(), ()> {
    let mut counter = 0 as u64;
    move |_| -> ControlFlow<(), ()> {
        counter += 1;
        if counter > milis {
            counter = 0;
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }
}

fn try_compare(lhs: &SuspendReason, rhs: &SuspendReason) -> eyre::Result<bool> {
    Ok(match lhs {
        SuspendReason::Yield(_) => eyre::bail!("Can't compare yields"),
        SuspendReason::SuspendedEpoch => matches!(rhs, SuspendReason::SuspendedEpoch),
        SuspendReason::SuspendedCallback => matches!(rhs, SuspendReason::SuspendedCallback),
        SuspendReason::SuspendedFlag => matches!(rhs, SuspendReason::SuspendedFlag),
        _ => eyre::bail!("unimplemented new variant"),
    })
}

// check if you can suspend while looping
fn suspend_with_pure_loop(
    set_cond: &mut impl FnMut(&mut Store) -> (),
    expected_reason: SuspendReason,
) -> eyre::Result<()> {
    println!("## test suspend in loop");

    let wasm: String = {
        let detect_overflow = overflow_detect_snippet("$res");
        format!(
            r#"(module
            (memory $mem 1)
            (export "memory" (memory $mem)) ;; first 8 bytes - counter, next 4 - overflow flag

            (func (export "start_counter")
                (local $res i64)
                (loop $lp
                    (i32.const 0) ;;where to store
                    (i64.load $mem (i32.const 0))
                    (i64.const 1)
                    (i64.add)
                    (local.set $res)
                    (local.get $res)
                    (i64.store $mem)
                    {detect_overflow}
                    (br $lp)
                )
            )
        )"#
        )
        .into()
    };

    let mut tested = {
        let wasm = wat::parse_str(wasm)?;
        let module = Module::parse_bytes(&wasm)?;
        let mut store = Store::default();
        let instance = module.instantiate(&mut store, None)?;
        TestedModule { store, instance: instance, resumable: None }
    };

    let increases = run_loops_look_at_numbers(&mut tested, set_cond, expected_reason, 16)?;
    assert!(increases > 2, "code doesn't enough: either suspend condition is too tight or something is broken");
    Ok(())
}

// check if you can suspend when calling wasm function
fn suspend_with_wasm_fn(
    set_cond: &mut impl FnMut(&mut Store) -> (),
    expected_reason: SuspendReason,
) -> eyre::Result<()> {
    println!("## test suspend wasm fn");

    let wasm: String = {
        let detect_overflow = overflow_detect_snippet("$res");
        format!(
            r#"(module
            (memory $mem 1)
            (export "memory" (memory $mem)) ;; first 8 bytes - counter, next 8 - overflow counter

            (func $wasm_nop
                nop
            )

            (func $wasm_adder (param i64 i64) (result i64)
                (local.get 0)
                (local.get 1)
                (i64.add)
            )

            (func $overflow_detect (param $res i64)
                {detect_overflow}
            )

            (func (export "start_counter")
                (local $res i64)
                (loop $lp
                    (call $wasm_nop)
                    (i32.const 0) ;;where to store
                    (i64.load $mem (i32.const 0))
                    (i64.const 1)
                    (call $wasm_adder)
                    (local.set $res)
                    (call $wasm_nop)
                    (local.get $res)
                    (i64.store $mem)
                    (local.get $res)
                    (call $overflow_detect)
                    (call $wasm_nop)
                    (br $lp)
                )
            )
        )"#
        )
        .into()
    };

    let mut tested = {
        let wasm = wat::parse_str(wasm)?;
        let module = Module::parse_bytes(&wasm)?;
        let mut store = Store::default();
        let instance = module.instantiate(&mut store, None)?;
        TestedModule { store, instance: instance, resumable: None }
    };

    let increases = run_loops_look_at_numbers(&mut tested, set_cond, expected_reason, 16)?;
    assert!(increases > 2, "code doesn't enough: either suspend condition is too tight or something is broken");

    Ok(())
}

// check if you can suspend when calling host function
fn suspend_with_host_fn(
    set_cond: &mut impl FnMut(&mut Store) -> (),
    expected_reason: SuspendReason,
) -> eyre::Result<()> {
    println!("## test suspend host fn");

    let wasm: String = {
        format!(
            r#"(module
            (import "host" "adder" (func $host_adder (param i64 i64)(result i64)))
            (import "host" "nop" (func $host_nop))
            (import "host" "detect" (func $overflow_detect (param $res i64)))
            (memory $mem 1)
            (export "memory" (memory $mem)) ;; first 8 bytes - counter, next 8 - overflow counter

            (func (export "start_counter")
                (local $res i64)
                (loop $lp
                    (call $host_nop)
                    (i32.const 0) ;;where to store
                    (i64.load $mem (i32.const 0))
                    (i64.const 1)
                    (call $host_adder)
                    (local.set $res)
                    (call $host_nop)
                    (local.get $res)
                    (i64.store $mem)
                    (local.get $res)
                    (call $overflow_detect)
                    (call $host_nop)
                    (br $lp)
                )
            )
        )"#
        )
        .into()
    };

    let mut tested = {
        let wasm = wat::parse_str(wasm)?;
        let module = Module::parse_bytes(&wasm)?;
        let mut store = Store::default();
        let mut imports = Imports::new();
        imports.define(
            "host",
            "adder",
            Extern::typed_func(|_, args: (i64, i64)| -> tinywasm::Result<i64> { Ok(args.0 + args.1) }),
        )?;
        imports.define(
            "host",
            "nop",
            Extern::typed_func(|_, ()| -> tinywasm::Result<()> {
                std::thread::sleep(Duration::from_micros(1));
                Ok(())
            }),
        )?;
        imports.define(
            "host",
            "detect",
            Extern::typed_func(|mut ctx, arg: i64| -> tinywasm::Result<()> {
                if arg != 0 {
                    return Ok(());
                }
                let mut mem = ctx.module().exported_memory_mut(ctx.store_mut(), "memory").expect("where's memory");
                let mut buf = [0 as u8; 8];
                buf.copy_from_slice(mem.load(8, 8)?);
                let counter = i64::from_be_bytes(buf);
                mem.store(8, 8, &i64::to_be_bytes(counter + 1))?;
                Ok(())
            }),
        )?;

        let instance = module.instantiate(&mut store, Some(imports))?;
        TestedModule { store, instance: instance, resumable: None }
    };

    let increases = run_loops_look_at_numbers(&mut tested, set_cond, expected_reason, 16)?;
    assert!(increases > 2, "code doesn't enough: either suspend condition is too tight or something is broken");
    Ok(())
}

fn run_loops_look_at_numbers(
    tested: &mut TestedModule,
    set_cond: &mut impl FnMut(&mut Store) -> (),
    expected_reason: SuspendReason,
    times: u32,
) -> eyre::Result<u32> {
    set_cond(&mut tested.store);
    let suspend = tested.start_counter_incrementing_loop("start_counter")?;
    assert!(try_compare(&suspend, &expected_reason).expect("unexpected yield"));

    let mut prev_counter = tested.get_counter();
    let mut times_increased = 0 as u32;

    {
        let (big, small) = prev_counter;
        println!("after start {big} {small}");
    }

    assert!(prev_counter >= (0, 0));

    for _ in 0..times - 1 {
        set_cond(&mut tested.store);
        assert!(try_compare(&tested.continue_counter_incrementing_loop()?, &expected_reason)?);
        let new_counter = tested.get_counter();
        // save for scheduling weirdness, loop should run for a bunch of times in 3ms
        assert!(new_counter >= prev_counter);
        {
            let (big, small) = new_counter;
            println!("after continue {big} {small}");
        }
        if new_counter > prev_counter {
            times_increased += 1;
        }
        prev_counter = new_counter;
    }
    Ok(times_increased)
}

fn overflow_detect_snippet(var: &str) -> String {
    format!(
        r#"(i64.eq (i64.const 0) (local.get {var}))
        (if
            (then
                ;; we wrapped around back to 0 - set flag
                (i32.const 8) ;;where to store
                (i32.const 8) ;;where to load
                (i64.load)
                (i64.const 1)
                (i64.add)
                (i64.store $mem)
            )
            (else
                nop
            )
        )
        "#
    )
    .into()
}

// should have exported memory "memory" and
struct TestedModule {
    store: Store,
    instance: ModuleInstance,
    resumable: Option<tinywasm::SuspendedFunc>,
}

impl TestedModule {
    fn start_counter_incrementing_loop(&mut self, fn_name: &str) -> tinywasm::Result<SuspendReason> {
        let starter = self.instance.exported_func_untyped(&self.store, fn_name)?;
        if let PotentialCoroCallResult::Suspended(res, coro) = starter.call_coro(&mut self.store, &[])? {
            self.resumable = Some(coro);
            return Ok(res);
        } else {
            panic!("that should never return");
        }
    }

    fn continue_counter_incrementing_loop(&mut self) -> tinywasm::Result<SuspendReason> {
        let paused = if let Some(val) = self.resumable.as_mut() {
            val
        } else {
            panic!("nothing to continue");
        };
        let resume_res = (*paused).resume(&mut self.store, None)?;
        match resume_res {
            CoroStateResumeResult::Suspended(res) => Ok(res),
            CoroStateResumeResult::Return(_) => panic!("should never return"),
        }
    }

    // (counter, overflow flag)
    fn get_counter(&self) -> (u64, u64) {
        let counter_now = {
            let mem = self.instance.exported_memory(&self.store, "memory").expect("where's memory");
            let mut buff: [u8; 8] = [0; 8];
            let in_mem = mem.load(0, 8).expect("where's memory");
            buff.clone_from_slice(in_mem);
            u64::from_le_bytes(buff)
        };
        let overflow_times = {
            let mem = self.instance.exported_memory(&self.store, "memory").expect("where's memory");
            let mut buff: [u8; 8] = [0; 8];
            let in_mem = mem.load(8, 8).expect("where's memory");
            buff.clone_from_slice(in_mem);
            u64::from_le_bytes(buff)
        };
        (overflow_times, counter_now)
    }
}
