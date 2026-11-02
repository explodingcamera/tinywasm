use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tinywasm::{
    Error, FuncContext, HostExecProgress, HostFunction, Imports, ModuleInstance, Result, Store, Trap, WasmValue,
};

fn instance(store: &mut Store, source: &str, calls: Arc<AtomicUsize>) -> Result<ModuleInstance> {
    let mut imports = Imports::new();
    imports.define(
        "env",
        "wait",
        HostFunction::from(move |_: FuncContext<'_>, value: i32| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(value + 1)
        }),
    );
    let module = tinywasm::parse_bytes(&wat::parse_str(source).unwrap())?;
    ModuleInstance::instantiate(store, &module, Some(&imports))
}

const SOURCE: &str = r#"(module
  (type $i2i (func (param i32) (result i32)))
  (import "env" "wait" (func $wait (type $i2i)))
  (memory (export "memory") 1)
  (table 1 funcref)
  (elem (i32.const 0) func $wait)
  (func (export "run") (result i32) i32.const 20 call $wait i32.const 2 i32.mul)
  (func (export "tail") (result i32) i32.const 20 return_call $wait)
  (func (export "indirect") (result i32) i32.const 20 i32.const 0 call_indirect (type $i2i))
  (func (export "tail-indirect") (result i32) i32.const 20 i32.const 0 return_call_indirect (type $i2i))
  (func (export "ref") (result i32) i32.const 20 ref.func $wait call_ref $i2i)
  (func (export "tail-ref") (result i32) i32.const 20 ref.func $wait return_call_ref $i2i)
  (export "host" (func $wait)))"#;

#[test]
fn host_calls_wait_without_replay_and_validate_results() -> Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut store = Store::default();
    let instance = instance(&mut store, SOURCE, calls.clone())?;
    for (name, expected) in [
        ("run", 84),
        ("tail", 42),
        ("host", 42),
        ("indirect", 42),
        ("tail-indirect", 42),
        ("ref", 42),
        ("tail-ref", 42),
    ] {
        let function = instance.func_untyped(&store, name)?;
        let params = if name == "host" { vec![WasmValue::I32(20)] } else { vec![] };
        let mut results = [WasmValue::I32(-1)];
        let mut execution = function.call_resumable_with_host_calls(&mut store, &params, &mut results)?;
        while execution.resume_with_fuel(100)? == HostExecProgress::Suspended {}
        for _ in 0..3 {
            assert_eq!(execution.resume_with_fuel(0)?, HostExecProgress::HostCall);
            let call = execution.host_call().unwrap();
            assert_eq!(call.params(), &[WasmValue::I32(20)]);
        }
        assert!(matches!(execution.host_call().unwrap().complete(&[]), Err(Error::InvalidHostFnReturn { .. })));
        assert!(matches!(
            execution.host_call().unwrap().complete(&[WasmValue::I64(42)]),
            Err(Error::InvalidHostFnReturn { .. })
        ));
        execution.host_call().unwrap().complete(&[WasmValue::I32(42)])?;
        while execution.resume_with_fuel(1)? == HostExecProgress::Suspended {}
        assert!(execution.resume_with_fuel(100).is_err());
        drop(execution);
        assert_eq!(results, [WasmValue::I32(expected)]);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn synchronous_callbacks_can_be_forwarded_once() -> Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut store = Store::default();
    let instance = instance(&mut store, SOURCE, calls.clone())?;
    for (name, expected) in [
        ("run", 42),
        ("tail", 21),
        ("host", 21),
        ("indirect", 21),
        ("tail-indirect", 21),
        ("ref", 21),
        ("tail-ref", 21),
    ] {
        let function = instance.func_untyped(&store, name)?;
        let params = if name == "host" { vec![WasmValue::I32(20)] } else { vec![] };
        let mut results = [WasmValue::I32(-1)];
        let mut execution = function.call_resumable_with_host_calls(&mut store, &params, &mut results)?;
        loop {
            match execution.resume_with_fuel(1)? {
                HostExecProgress::HostCall => execution.host_call().unwrap().invoke()?,
                HostExecProgress::Suspended => {}
                HostExecProgress::Completed => break,
            }
        }
        drop(execution);
        assert_eq!(results, [WasmValue::I32(expected)]);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 7);
    Ok(())
}

#[test]
fn external_adapter_uses_callers_memory_and_canonical_abi_exports() -> Result<()> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut store = Store::default();
    let leaf = instance(
        &mut store,
        r#"(module
      (import "env" "wait" (func $wait (param i32) (result i32)))
      (memory (export "memory") 1)
      (global $post (mut i32) (i32.const 0))
      (func (export "realloc") (param i32 i32 i32 i32) (result i32) i32.const 64)
      (func (export "post-return") (param i32) i32.const 1 global.set $post)
      (func (export "run") (result i32)
        i32.const 20 call $wait i32.load global.get $post i32.add))"#,
        calls,
    )?;
    let mut imports = Imports::new();
    imports.link_module("leaf", leaf)?;
    let module = tinywasm::parse_bytes(
        &wat::parse_str(
            r#"(module
      (import "leaf" "run" (func $run (result i32)))
      (memory (export "memory") 1)
      (func (export "run") (result i32) call $run))"#,
        )
        .unwrap(),
    )?;
    let root = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    let function = root.func_untyped(&store, "run")?;
    let mut results = [WasmValue::I32(0)];
    let mut execution = function.call_resumable_with_host_calls(&mut store, &[], &mut results)?;
    assert_eq!(execution.resume_with_fuel(100)?, HostExecProgress::HostCall);
    let mut call = execution.host_call().unwrap();
    let pointer = call.with_context(|mut context| {
        let realloc = context.module().func::<(i32, i32, i32, i32), i32>(context.store(), "realloc")?;
        let pointer = context.call(&realloc, (0, 0, 4, 4))?;
        context.memory("memory")?.copy_from_slice(context.store_mut(), pointer as usize, &41_i32.to_le_bytes())?;
        let post = context.module().func::<i32, ()>(context.store(), "post-return")?;
        context.call(&post, pointer)?;
        context.store_mut().gc()?;
        Ok(pointer)
    })?;
    call.complete(&[WasmValue::I32(pointer)])?;
    assert_eq!(execution.resume_with_fuel(100)?, HostExecProgress::Completed);
    drop(execution);
    assert_eq!(results, [WasmValue::I32(42)]);
    assert_eq!(root.memory("memory")?.read_vec(&store, 64, 4)?, [0; 4]);
    Ok(())
}

#[test]
fn dropping_a_pending_call_releases_argument_roots() -> Result<()> {
    struct Usage(Arc<AtomicUsize>);
    impl tinywasm::ResourceLimiter for Usage {
        fn gc_growing(&self, current: usize, _: usize, _: Option<usize>) -> std::result::Result<bool, Trap> {
            self.0.store(current, Ordering::SeqCst);
            Ok(true)
        }
    }
    let current = Arc::new(AtomicUsize::new(usize::MAX));
    let config = tinywasm::engine::Config::new()
        .with_gc_collection_threshold(usize::MAX)
        .with_resource_limiter(Usage(current.clone()));
    let mut store = Store::new(tinywasm::Engine::new(config));
    let mut imports = Imports::new();
    imports.define(
        "env",
        "wait",
        HostFunction::from(|_: FuncContext<'_>, value: Option<tinywasm::ExternRef>| Ok(value)),
    );
    let module = tinywasm::parse_bytes(
        &wat::parse_str(
            r#"(module
      (type $s (struct (field i32)))
      (import "env" "wait" (func $wait (param externref) (result externref)))
      (func (export "make") (result externref) i32.const 7 struct.new $s extern.convert_any)
      (func (export "read") (param externref) (result i32)
        local.get 0 any.convert_extern ref.cast (ref $s) struct.get $s 0)
      (func (export "run") (param externref) (result externref) local.get 0 return_call $wait))"#,
        )
        .unwrap(),
    )?;
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    let make = instance.func::<(), Option<tinywasm::ExternRef>>(&store, "make")?;
    let reference = make.call(&mut store, ())?.unwrap();
    let function = instance.func_untyped(&store, "run")?;
    let params = [WasmValue::from(reference)];
    let mut results = [WasmValue::I32(0)];
    let mut execution = function.call_resumable_with_host_calls(&mut store, &params, &mut results)?;
    drop(params);
    assert_eq!(execution.resume_with_fuel(100)?, HostExecProgress::HostCall);
    let mut call = execution.host_call().unwrap();
    let param = call.params()[0].clone();
    call.with_context(move |mut context| {
        context.store_mut().gc()?;
        let read = context.module().func_untyped(context.store(), "read")?;
        let mut result = [WasmValue::I32(0)];
        context.call_untyped(&read, &[param], &mut result)?;
        assert_eq!(result, [WasmValue::I32(7)]);
        Ok(())
    })?;
    let mut foreign_store = Store::default();
    let foreign = tinywasm::ExternRef::try_new(&mut foreign_store, 0)?;
    assert!(matches!(
        execution.host_call().unwrap().complete(&[WasmValue::from(foreign)]),
        Err(Error::Trap(Trap::InvalidStore))
    ));
    drop(execution);
    store.gc()?;
    drop(make.call(&mut store, ())?);
    assert_eq!(current.load(Ordering::SeqCst), 0, "pending arguments must not leak GC objects");
    assert_eq!(
        instance
            .func::<Option<tinywasm::ExternRef>, Option<tinywasm::ExternRef>>(&store, "run")?
            .call(&mut store, None)?,
        None
    );
    Ok(())
}

#[test]
fn callback_panics_and_guest_traps_are_terminal() -> Result<()> {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let mut store = Store::default();
    let mut imports = Imports::new();
    imports.define(
        "env",
        "wait",
        HostFunction::from(|_: FuncContext<'_>, (): ()| -> Result<()> { panic!("host panic") }),
    );
    let module = tinywasm::parse_bytes(
        &wat::parse_str(
            r#"(module
      (import "env" "wait" (func $wait))
      (func (export "run") call $wait)
      (func (export "trap") unreachable)
      (func (export "ok") (result i32) i32.const 42))"#,
        )
        .unwrap(),
    )?;
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    let function = instance.func_untyped(&store, "run")?;
    let mut execution = function.call_resumable_with_host_calls(&mut store, &[], &mut [])?;
    assert_eq!(execution.resume_with_fuel(100)?, HostExecProgress::HostCall);
    assert!(catch_unwind(AssertUnwindSafe(|| execution.host_call().unwrap().invoke())).is_err());
    assert!(execution.resume_with_fuel(100).is_err());
    drop(execution);
    let mut execution = function.call_resumable_with_host_calls(&mut store, &[], &mut [])?;
    assert_eq!(execution.resume_with_fuel(100)?, HostExecProgress::HostCall);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            execution.host_call().unwrap().with_context(|_| -> Result<()> { panic!("context panic") })
        }))
        .is_err()
    );
    assert!(execution.resume_with_fuel(100).is_err());
    drop(execution);
    let trap = instance.func_untyped(&store, "trap")?;
    let mut execution = trap.call_resumable_with_host_calls(&mut store, &[], &mut [])?;
    assert!(matches!(execution.resume_with_fuel(100), Err(Error::Trap(Trap::Unreachable))));
    assert!(execution.resume_with_fuel(100).is_err());
    drop(execution);
    assert_eq!(instance.func::<(), i32>(&store, "ok")?.call(&mut store, ())?, 42);
    Ok(())
}

#[test]
fn future_adapter_waits_for_a_wakeup_without_repolling_wasm() -> Result<()> {
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::task::{Context, Poll, Wake, Waker};

    #[derive(Default)]
    struct Mailbox {
        value: Option<i32>,
        waker: Option<Waker>,
    }
    struct Deferred {
        mailbox: Arc<Mutex<Mailbox>>,
        cancelled: Arc<AtomicUsize>,
        done: bool,
    }
    impl Drop for Deferred {
        fn drop(&mut self) {
            if !self.done {
                self.cancelled.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
    impl Future for Deferred {
        type Output = i32;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<i32> {
            let this = self.get_mut();
            let mut mailbox = this.mailbox.lock().unwrap();
            if let Some(value) = mailbox.value.take() {
                this.done = true;
                return Poll::Ready(value);
            }
            mailbox.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
    // All future and scheduler state lives in the external adapter. No runtime-private APIs.
    struct Adapter<'a> {
        execution: tinywasm::HostExecution<'a>,
        mailbox: Arc<Mutex<Mailbox>>,
        future: Option<Deferred>,
        cancelled: Arc<AtomicUsize>,
    }
    impl Future for Adapter<'_> {
        type Output = Result<()>;
        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.get_mut();
            loop {
                if let Some(future) = &mut this.future {
                    let value = std::task::ready!(Pin::new(future).poll(cx));
                    this.execution.host_call().unwrap().complete(&[WasmValue::I32(value)])?;
                    this.future = None;
                }
                match this.execution.resume_with_fuel(128)? {
                    HostExecProgress::Completed => return Poll::Ready(Ok(())),
                    HostExecProgress::Suspended => {
                        cx.waker().wake_by_ref();
                        return Poll::Pending;
                    }
                    HostExecProgress::HostCall => {
                        assert_eq!(this.execution.host_call().unwrap().params(), &[WasmValue::I32(20)]);
                        this.future = Some(Deferred {
                            mailbox: this.mailbox.clone(),
                            cancelled: this.cancelled.clone(),
                            done: false,
                        });
                    }
                }
            }
        }
    }
    struct WakeCount(AtomicUsize);
    impl Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let mut store = Store::default();
    let instance = instance(&mut store, SOURCE, calls.clone())?;
    let function = instance.func_untyped(&store, "run")?;
    let mut results = [WasmValue::I32(0)];
    let mailbox = Arc::new(Mutex::new(Mailbox::default()));
    let cancelled = Arc::new(AtomicUsize::new(0));
    let mut adapter = Adapter {
        cancelled: cancelled.clone(),
        execution: function.call_resumable_with_host_calls(&mut store, &[], &mut results)?,
        mailbox: mailbox.clone(),
        future: None,
    };
    let wakes = Arc::new(WakeCount(AtomicUsize::new(0)));
    let waker = Waker::from(wakes.clone());
    let mut cx = Context::from_waker(&waker);
    assert!(Pin::new(&mut adapter).poll(&mut cx).is_pending());
    assert!(Pin::new(&mut adapter).poll(&mut cx).is_pending());
    assert_eq!(wakes.0.load(Ordering::SeqCst), 0, "host wait must not self-wake");
    let wake = {
        let mut mailbox = mailbox.lock().unwrap();
        mailbox.value = Some(42);
        mailbox.waker.take().unwrap()
    };
    wake.wake();
    assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
    assert!(matches!(Pin::new(&mut adapter).poll(&mut cx), Poll::Ready(Ok(()))));
    drop(adapter);
    assert_eq!(results, [WasmValue::I32(84)]);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(cancelled.load(Ordering::SeqCst), 0);
    let mut cancelled_results = [WasmValue::I32(-1)];
    let mut adapter = Adapter {
        execution: function.call_resumable_with_host_calls(&mut store, &[], &mut cancelled_results)?,
        mailbox: Arc::new(Mutex::new(Mailbox::default())),
        future: None,
        cancelled: cancelled.clone(),
    };
    assert!(Pin::new(&mut adapter).poll(&mut cx).is_pending());
    drop(adapter);
    assert_eq!(cancelled.load(Ordering::SeqCst), 1);
    assert_eq!(cancelled_results, [WasmValue::I32(-1)]);
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 42);

    Ok(())
}
