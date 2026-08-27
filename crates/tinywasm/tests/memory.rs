use tinywasm::engine::Config;
use tinywasm::types::{MemoryArch, MemoryType, RefType, RefValue, TableType};
use tinywasm::{Engine, Memory, ModuleInstance, ResourceLimiter, Store, Table, Trap};

type TestResult<T = ()> = Result<T, Box<dyn core::error::Error>>;

fn store_with_limiter(limiter: impl ResourceLimiter + 'static) -> Store {
    let engine = Engine::new(Config::new().with_resource_limiter(limiter));
    Store::new(engine)
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
