use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tinywasm::engine::Config;
use tinywasm::types::{MemoryArch, MemoryType, RefType, RefValue, TableType};
use tinywasm::{Engine, Imports, Memory, ModuleInstance, ResourceLimiter, Store, Table, Trap};

type TestResult<T = ()> = Result<T, Box<dyn core::error::Error>>;

fn store_with_limiter(limiter: impl ResourceLimiter + 'static) -> Store {
    let engine = Engine::new(Config::new().with_resource_limiter(limiter));
    Store::new(engine)
}

#[derive(Clone)]
struct SharedQuota {
    used: Arc<AtomicUsize>,
    limit: usize,
}

impl SharedQuota {
    fn new(limit: usize) -> Self {
        Self { used: Arc::new(AtomicUsize::new(0)), limit }
    }

    fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }
}

impl ResourceLimiter for SharedQuota {
    fn memory_growing(&self, current: usize, desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        let delta = desired - current;
        self.used
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |used| {
                (delta <= self.limit.saturating_sub(used)).then_some(used + delta)
            })
            .map(|_| true)
            .or_else(|_| Ok(false))
    }

    fn memory_grow_failed(&self, current: usize, desired: usize) {
        self.used.fetch_sub(desired - current, Ordering::SeqCst);
    }

    fn memory_dropped(&self, charged_bytes: usize) {
        self.used.fetch_sub(charged_bytes, Ordering::SeqCst);
    }
}

#[test]
fn shared_quota_releases_store_memories_for_worker_restart() -> TestResult {
    const PAGE: usize = 65_536;
    let quota = SharedQuota::new(2 * PAGE);
    let engine = Engine::new(Config::new().with_resource_limiter(quota.clone()));
    let ty = MemoryType::new(MemoryArch::I32, 1, None, None);

    let mut main = Store::new(engine.clone());
    let _main_memory = Memory::try_new(&mut main, ty)?;
    let mut worker = Store::new(engine.clone());
    let _worker_memory = Memory::try_new(&mut worker, ty)?;
    assert_eq!(quota.used(), 2 * PAGE);

    let mut denied = Store::new(engine.clone());
    assert!(matches!(Memory::try_new(&mut denied, ty), Err(tinywasm::Error::Trap(Trap::OutOfMemory))));
    assert_eq!(quota.used(), 2 * PAGE);

    drop(worker);
    assert_eq!(quota.used(), PAGE);
    let _replacement_worker = Memory::try_new(&mut denied, ty)?;
    assert_eq!(quota.used(), 2 * PAGE);
    drop(denied);
    drop(main);
    assert_eq!(quota.used(), 0);
    Ok(())
}

#[test]
fn shared_quota_counts_logical_bytes_for_custom_pages() -> TestResult {
    let quota = SharedQuota::new(8);
    let mut store = store_with_limiter(quota.clone());
    let ty = MemoryType::new(MemoryArch::I64, 1, Some(3), Some(4));
    let memory = Memory::try_new(&mut store, ty)?;
    assert_eq!(quota.used(), 4);
    assert_eq!(memory.grow(&mut store, 1)?, Some(1));
    assert_eq!(quota.used(), 8);
    assert_eq!(memory.grow(&mut store, 1)?, None);
    assert_eq!(quota.used(), 8);
    drop(store);
    assert_eq!(quota.used(), 0);
    Ok(())
}

#[test]
fn imported_memory_is_only_charged_once() -> TestResult {
    const PAGE: usize = 65_536;
    let module = tinywasm::parse_bytes(&wat::parse_str("(module (import \"host\" \"memory\" (memory 1)))")?)?;
    let quota = SharedQuota::new(PAGE);
    let mut store = store_with_limiter(quota.clone());
    let memory = Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, None, None))?;
    let mut imports = Imports::new();
    imports.define("host", "memory", memory);
    let _first = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    let _second = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    assert_eq!(quota.used(), PAGE);
    drop(store);
    assert_eq!(quota.used(), 0);
    Ok(())
}

#[test]
fn failed_second_memory_does_not_strand_first_reservation() -> TestResult {
    const PAGE: usize = 65_536;
    let module = tinywasm::parse_bytes(&wat::parse_str(
        "(module (memory (export \"first\") 1) (memory (export \"second\") 1))",
    )?)?;
    assert_eq!(module.memory_types.len(), 2);
    let quota = SharedQuota::new(PAGE);
    let mut store = store_with_limiter(quota.clone());
    let result = ModuleInstance::instantiate(&mut store, &module, None);
    assert!(matches!(&result, Err(tinywasm::Error::Trap(Trap::OutOfMemory))), "{result:?}");
    assert_eq!(quota.used(), 0);
    let _memory = Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, None, None))?;
    assert_eq!(quota.used(), PAGE);
    drop(store);
    assert_eq!(quota.used(), 0);
    Ok(())
}

#[cfg(feature = "send")]
#[test]
fn concurrent_stores_cannot_both_reserve_the_last_page() {
    const PAGE: usize = 65_536;
    let quota = SharedQuota::new(PAGE);
    let engine = Engine::new(Config::new().with_resource_limiter(quota.clone()));
    let start = Arc::new(std::sync::Barrier::new(3));
    let release = Arc::new(std::sync::Barrier::new(3));
    let (sender, receiver) = std::sync::mpsc::channel();
    let workers: Vec<_> = (0..2)
        .map(|_| {
            let engine = engine.clone();
            let start = start.clone();
            let release = release.clone();
            let sender = sender.clone();
            std::thread::spawn(move || {
                let mut store = Store::new(engine);
                start.wait();
                let accepted = Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, None, None)).is_ok();
                sender.send(accepted).unwrap();
                release.wait();
            })
        })
        .collect();
    start.wait();
    let accepted = usize::from(receiver.recv().unwrap()) + usize::from(receiver.recv().unwrap());
    assert_eq!(quota.used(), PAGE);
    assert_eq!(accepted, 1);
    release.wait();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(quota.used(), 0);
}

#[test]
fn memory_read_write_roundtrip() -> TestResult {
    let mut store = Store::default();
    let memory = Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, None, None))?;

    memory.copy_from_slice(&mut store, 0, &[1, 2, 3, 4, 5])?;
    assert_eq!(memory.read_vec(&store, 0, 5)?, &[1, 2, 3, 4, 5]);
    memory.fill(&mut store, 2, 2, 0)?;
    assert_eq!(memory.read_vec(&store, 0, 5)?, &[1, 2, 0, 0, 5]);
    Ok(())
}

#[test]
fn read_returns_short_count_at_end_of_memory() -> TestResult {
    let mut store = Store::default();
    let memory = Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, Some(1), Some(4)))?;
    memory.copy_from_slice(&mut store, 0, &[1, 2, 3, 4])?;

    let mut dst = [9; 8];
    assert_eq!(memory.read(&store, 2, &mut dst)?, 2);
    assert_eq!(&dst[..2], &[3, 4]);
    assert_eq!(&dst[2..], &[9; 6]);

    Ok(())
}

#[test]
fn memory64_default_limit_is_not_memory32_limit() {
    let ty = MemoryType::new(MemoryArch::I64, 65_537, None, None);
    assert!(ty.page_count_max() > 65_536);
}

struct DenyAll;

impl ResourceLimiter for DenyAll {
    fn memory_growing(&self, _current: usize, _desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        Ok(false)
    }

    fn table_growing(&self, _current: usize, _desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        Ok(false)
    }
}

struct DenyGrowth;

impl ResourceLimiter for DenyGrowth {
    fn memory_growing(&self, current: usize, _desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        Ok(current == 0)
    }

    fn table_growing(&self, current: usize, _desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        Ok(current == 0)
    }
}

struct TrapGrowth;

impl ResourceLimiter for TrapGrowth {
    fn memory_growing(&self, current: usize, _desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        if current == 0 { Ok(true) } else { Err(Trap::Unreachable) }
    }

    fn table_growing(&self, current: usize, _desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        if current == 0 { Ok(true) } else { Err(Trap::Unreachable) }
    }
}

#[test]
fn resource_limiter_can_reject_growth() -> TestResult {
    let mut store = store_with_limiter(DenyGrowth);
    let memory = Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, None, None))?;

    assert_eq!(memory.grow(&mut store, 1)?, None);
    assert_eq!(memory.page_count(&store)?, 1);
    Ok(())
}

#[test]
fn resource_limiter_rejects_guest_memory_grow() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory 1)
          (func (export "grow") (result i32)
            i32.const 1
            memory.grow))
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = store_with_limiter(DenyGrowth);
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

    let grow = instance.func::<(), i32>(&store, "grow")?;
    assert_eq!(grow.call(&mut store, ())?, -1);
    Ok(())
}

#[test]
fn resource_limiter_can_trap_guest_memory_grow() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory 1)
          (func (export "grow") (result i32)
            i32.const 1
            memory.grow))
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = store_with_limiter(TrapGrowth);
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

    let grow = instance.func::<(), i32>(&store, "grow")?;
    assert!(matches!(grow.call(&mut store, ()), Err(tinywasm::Error::Trap(Trap::Unreachable))));
    Ok(())
}

#[test]
fn resource_limiter_rejects_host_memory_initial_size() {
    let mut store = store_with_limiter(DenyAll);
    let result = Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, None, None));

    assert!(matches!(result, Err(tinywasm::Error::Trap(Trap::OutOfMemory))));
}

#[test]
fn resource_limiter_rejects_module_memory_initial_size() -> TestResult {
    let wasm = wat::parse_str("(module (memory (export \"memory\") 1))")?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = store_with_limiter(DenyAll);
    let result = ModuleInstance::instantiate(&mut store, &module, None);

    assert!(matches!(result, Err(tinywasm::Error::Trap(Trap::OutOfMemory))));
    Ok(())
}

#[test]
fn resource_limiter_rejects_table_initial_size() {
    let mut store = store_with_limiter(DenyAll);
    let result = Table::try_new(&mut store, TableType::new(RefType::FUNCREF, 1, None), RefValue::Null.into());

    assert!(matches!(result, Err(tinywasm::Error::Trap(Trap::OutOfMemory))));
}

#[test]
fn table_rejects_initial_size_above_maximum() {
    let mut store = Store::default();
    let result = Table::try_new(&mut store, TableType::new(RefType::FUNCREF, 2, Some(1)), RefValue::Null.into());

    assert!(matches!(result, Err(tinywasm::Error::Trap(Trap::OutOfMemory))));
}

#[test]
fn resource_limiter_can_reject_table_growth() -> TestResult {
    let mut store = store_with_limiter(DenyGrowth);
    let table = Table::try_new(&mut store, TableType::new(RefType::FUNCREF, 1, None), RefValue::Null.into())?;

    assert_eq!(table.grow(&mut store, 1, RefValue::Null.into())?, None);
    assert_eq!(table.size(&store)?, 1);
    Ok(())
}

#[test]
fn resource_limiter_can_trap_table_growth() -> TestResult {
    let mut store = store_with_limiter(TrapGrowth);
    let table = Table::try_new(&mut store, TableType::new(RefType::FUNCREF, 1, None), RefValue::Null.into())?;

    assert!(matches!(table.grow(&mut store, 1, RefValue::Null.into()), Err(tinywasm::Error::Trap(Trap::Unreachable))));
    Ok(())
}

#[test]
fn resource_limiter_allows_guest_memory_grow_by_default() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory 1)
          (func (export "grow") (result i32)
            i32.const 1
            memory.grow))
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

    let grow = instance.func::<(), i32>(&store, "grow")?;
    assert_eq!(grow.call(&mut store, ())?, 1);
    Ok(())
}
