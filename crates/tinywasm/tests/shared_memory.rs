#![cfg(feature = "std")]

use tinywasm::engine::Config;
use tinywasm::types::{MemoryArch, MemoryType, ModuleInner};
use tinywasm::{Engine, Imports, MemoryShared, ModuleInstance, ResourceLimiter, Store, Trap};

type TestResult = Result<(), Box<dyn core::error::Error>>;

#[test]
fn shared_memory_is_visible_across_stores() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (import "host" "memory" (memory 1 2 shared))
          (export "memory" (memory 0))
          (func (export "add") (result i32)
            i32.const 0
            i32.const 1
            i32.atomic.rmw.add)
          (func (export "size") (result i32) memory.size)
          (func (export "grow") (result i32) i32.const 1 memory.grow))
    "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let memory = MemoryShared::try_new(MemoryType::new(MemoryArch::I32, 1, Some(2), None))?;
    let mut imports = Imports::new();
    imports.define("host", "memory", memory.clone());

    let mut first_store = Store::default();
    let first = ModuleInstance::instantiate(&mut first_store, &module, Some(&imports))?;
    let mut second_store = Store::default();
    let second = ModuleInstance::instantiate(&mut second_store, &module, Some(&imports))?;

    memory.lock().data_mut()[..4].copy_from_slice(&41u32.to_le_bytes());
    assert_eq!(first.func::<(), i32>(&first_store, "add")?.call(&mut first_store, ())?, 41);
    assert_eq!(second.func::<(), i32>(&second_store, "add")?.call(&mut second_store, ())?, 42);
    assert_eq!(memory.read_vec(0, 4)?, 43u32.to_le_bytes());
    assert_eq!(first.memory_shared("memory")?.lock().data()[..4], 43u32.to_le_bytes());

    assert_eq!(first.func::<(), i32>(&first_store, "grow")?.call(&mut first_store, ())?, 1);
    assert_eq!(second.func::<(), i32>(&second_store, "size")?.call(&mut second_store, ())?, 2);
    assert_eq!(memory.lock().page_count(), 2);
    Ok(())
}

#[test]
fn concurrent_growth_publishes_page_count() -> TestResult {
    let memory = MemoryShared::try_new(MemoryType::new(MemoryArch::I32, 1, Some(3), None))?;
    let barrier = std::sync::Barrier::new(3);
    let mut sizes = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let memory = memory.clone();
                let barrier = &barrier;
                scope.spawn(move || {
                    barrier.wait();
                    memory.grow(1).unwrap().unwrap()
                })
            })
            .collect();
        barrier.wait();
        workers.into_iter().map(|worker| worker.join().unwrap()).collect::<Vec<_>>()
    });
    sizes.sort();
    assert_eq!(sizes, [1, 2]);
    assert_eq!(memory.page_count(), 3);
    assert_eq!(memory.len(), 3 * 65536);
    Ok(())
}

#[test]
fn shared_import_type_must_match() -> TestResult {
    let wasm = wat::parse_str("(module (import \"host\" \"memory\" (memory 1 2 shared)))")?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let ordinary = tinywasm::Memory::try_new(&mut store, MemoryType::new(MemoryArch::I32, 1, Some(2), None))?;
    let mut imports = Imports::new();
    imports.define("host", "memory", ordinary);
    assert!(ModuleInstance::instantiate(&mut store, &module, Some(&imports)).is_err());
    Ok(())
}

#[test]
fn shared_memory_requires_declared_maximum_even_when_unused() {
    let ty = MemoryType::new(MemoryArch::I32, 1, None, None).with_shared(true);
    let module = tinywasm::Module::from(ModuleInner {
        memory_types: vec![ty].into_boxed_slice(),
        skip_local_memory_allocation: true,
        ..ModuleInner::default()
    });
    assert!(ModuleInstance::instantiate(&mut Store::default(), &module, None).is_err());
}

#[test]
fn atomic_rmw_is_indivisible_across_stores() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (import "host" "memory" (memory 1 1 shared))
          (func (export "add")
            i32.const 0
            i32.const 1
            i32.atomic.rmw.add
            drop))
    "#,
    )?;
    let memory = MemoryShared::try_new(MemoryType::new(MemoryArch::I32, 1, Some(1), None))?;
    std::thread::scope(|scope| -> TestResult {
        let mut workers = Vec::new();
        for _ in 0..4 {
            let memory = memory.clone();
            let wasm = &wasm;
            workers.push(scope.spawn(move || -> tinywasm::Result<()> {
                let module = tinywasm::parse_bytes(wasm)?;
                let mut imports = Imports::new();
                imports.define("host", "memory", memory);
                let mut store = Store::default();
                let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
                let add = instance.func::<(), ()>(&store, "add")?;
                for _ in 0..1000 {
                    add.call(&mut store, ())?;
                }
                Ok(())
            }));
        }
        for worker in workers {
            worker.join().unwrap()?;
        }
        Ok(())
    })?;
    assert_eq!(memory.read_vec(0, 4)?, 4000u32.to_le_bytes());
    Ok(())
}

#[test]
fn wait_notify_across_stores() -> TestResult {
    let wasm = wat::parse_str(
        r#"(module
            (import "host" "memory" (memory 1 1 shared))
            (func (export "wait32") (param i32 i32 i64) (result i32)
                local.get 0 local.get 1 local.get 2 memory.atomic.wait32)
            (func (export "wait64") (param i32 i64 i64) (result i32)
                local.get 0 local.get 1 local.get 2 memory.atomic.wait64)
            (func (export "notify") (param i32 i32) (result i32)
                local.get 0 local.get 1 memory.atomic.notify))"#,
    )?;
    let memory = MemoryShared::try_new(MemoryType::new(MemoryArch::I32, 1, Some(1), None))?;
    std::thread::scope(|scope| -> TestResult {
        let mut workers = Vec::new();
        for (address, width) in [(0, 4), (0, 4), (8, 8)] {
            let worker_memory = memory.clone();
            let wasm = &wasm;
            workers.push(scope.spawn(move || -> tinywasm::Result<i32> {
                let module = tinywasm::parse_bytes(wasm)?;
                let mut imports = Imports::new();
                imports.define("host", "memory", worker_memory);
                let mut store = Store::default();
                let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
                if width == 4 {
                    instance
                        .func::<(i32, i32, i64), i32>(&store, "wait32")?
                        .call(&mut store, (address, 0, 10_000_000_000))
                } else {
                    instance
                        .func::<(i32, i64, i64), i32>(&store, "wait64")?
                        .call(&mut store, (address, 0, 10_000_000_000))
                }
            }));
        }
        let module = tinywasm::parse_bytes(&wasm)?;
        let mut imports = Imports::new();
        imports.define("host", "memory", memory.clone());
        let mut store = Store::default();
        let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
        let notify = instance.func::<(i32, i32), i32>(&store, "notify")?;
        for address in [8, 0, 0] {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            loop {
                if notify.call(&mut store, (address, 1))? == 1 {
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "waiter did not register");
                std::thread::yield_now();
            }
        }
        for worker in workers {
            assert_eq!(worker.join().unwrap()?, 0);
        }
        assert_eq!(notify.call(&mut store, (0, 1))?, 0);
        assert_eq!(notify.call(&mut store, (8, 1))?, 0);
        Ok(())
    })
}

#[test]
fn wait_checks_values_timeouts_and_addresses() -> TestResult {
    let module = tinywasm::parse_bytes(&wat::parse_str(
        r#"(module
            (memory 1 1 shared)
            (func (export "wait") (param i32 i64 i64) (result i32)
                local.get 0 local.get 1 local.get 2 memory.atomic.wait64)
            (func (export "notify") (param i32 i32) (result i32)
                local.get 0 local.get 1 memory.atomic.notify))"#,
    )?)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let wait = instance.func::<(i32, i64, i64), i32>(&store, "wait")?;
    assert_eq!(wait.call(&mut store, (0, 1, -1))?, 1);
    assert_eq!(wait.call(&mut store, (0, 0, 0))?, 2);
    assert_eq!(wait.call(&mut store, (0, 0, 1_000_000))?, 2);
    assert!(wait.call(&mut store, (1, 0, 0)).is_err());
    assert!(wait.call(&mut store, (65536, 0, 0)).is_err());
    let notify = instance.func::<(i32, i32), i32>(&store, "notify")?;
    assert_eq!(notify.call(&mut store, (0, 1))?, 0);
    assert!(notify.call(&mut store, (65536, 1)).is_err());
    Ok(())
}

#[test]
fn defined_shared_memory_supports_data_and_bulk_access() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory (export "memory") 1 2 shared)
          (data (i32.const 0) "abc")
          (func (export "copy")
            i32.const 4
            i32.const 0
            i32.const 3
            memory.copy)
          (func (export "read") (result i32)
            i32.const 4
            i32.load8_u))
    "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert!(instance.memory("memory").is_err());
    let memory = instance.memory_shared("memory")?;
    assert!(matches!(
        instance.exports().find(|(name, _)| *name == "memory"),
        Some((_, tinywasm::ExternItem::MemoryShared(_)))
    ));
    assert_eq!(memory.read_vec(0, 3)?, b"abc");
    instance.func::<(), ()>(&store, "copy")?.call(&mut store, ())?;
    assert_eq!(memory.read_vec(4, 3)?, b"abc");
    memory.lock().data_mut()[4] = b'z';
    assert_eq!(instance.func::<(), i32>(&store, "read")?.call(&mut store, ())?, i32::from(b'z'));

    let consumer = tinywasm::parse_bytes(&wat::parse_str(
        r#"
        (module
          (import "producer" "memory" (memory 1 2 shared))
          (func (export "read") (result i32)
            i32.const 4
            i32.load8_u))
    "#,
    )?)?;
    let mut imports = Imports::new();
    imports.link_module("producer", instance)?;
    let linked = ModuleInstance::instantiate(&mut store, &consumer, Some(&imports))?;
    assert_eq!(linked.func::<(), i32>(&store, "read")?.call(&mut store, ())?, i32::from(b'z'));
    Ok(())
}

#[cfg(feature = "archive")]
#[test]
fn shared_memory_flag_survives_serialization() -> TestResult {
    let wasm = wat::parse_str("(module (memory 1 2 shared))")?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let roundtrip = tinywasm::Module::try_from_twasm(&module.serialize_twasm()?)?;
    assert!(roundtrip.memory_types[0].shared());
    Ok(())
}

#[test]
fn copy_between_shared_memories_checks_ranges() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory (export "source") 1 1 shared)
          (memory (export "destination") 1 1 shared)
          (func (export "copy") (param i32 i32 i32)
            local.get 0
            local.get 1
            local.get 2
            memory.copy 1 0))
    "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let source = instance.memory_shared("source")?;
    let destination = instance.memory_shared("destination")?;
    let bytes = vec![0xa5; 8192];
    source.copy_from_slice(0, &bytes)?;
    let copy = instance.func::<(i32, i32, i32), ()>(&store, "copy")?;
    copy.call(&mut store, (8, 0, bytes.len() as i32))?;
    assert_eq!(destination.read_vec(8, bytes.len())?, bytes);
    assert!(copy.call(&mut store, (65535, 0, 2)).is_err());
    assert_eq!(destination.read_vec(8, bytes.len())?, bytes);
    Ok(())
}

#[test]
fn copy_between_ordinary_and_shared_memory() -> TestResult {
    let module = tinywasm::parse_bytes(&wat::parse_str(
        r#"
        (module
          (memory (export "ordinary") 1)
          (memory (export "shared") 1 1 shared)
          (func (export "to_shared")
            i32.const 4
            i32.const 0
            i32.const 3
            memory.copy 1 0)
          (func (export "to_ordinary")
            i32.const 8
            i32.const 4
            i32.const 3
            memory.copy 0 1))
    "#,
    )?)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let ordinary = instance.memory("ordinary")?;
    let shared = instance.memory_shared("shared")?;

    ordinary.copy_from_slice(&mut store, 0, b"abc")?;
    instance.func::<(), ()>(&store, "to_shared")?.call(&mut store, ())?;
    assert_eq!(shared.read_vec(4, 3)?, b"abc");

    shared.copy_from_slice(4, b"xyz")?;
    instance.func::<(), ()>(&store, "to_ordinary")?.call(&mut store, ())?;
    assert_eq!(ordinary.read_vec(&store, 8, 3)?, b"xyz");
    Ok(())
}

#[test]
fn copy_between_aliases_of_one_shared_memory() -> TestResult {
    let module = tinywasm::parse_bytes(&wat::parse_str(
        r#"
        (module
          (import "host" "source" (memory 1 1 shared))
          (import "host" "destination" (memory 1 1 shared))
          (func (export "copy")
            i32.const 1
            i32.const 0
            i32.const 3
            memory.copy 1 0))
    "#,
    )?)?;
    let memory = MemoryShared::try_new(MemoryType::new(MemoryArch::I32, 1, Some(1), None))?;
    memory.copy_from_slice(0, b"abc")?;
    let mut imports = Imports::new();
    imports.define("host", "source", memory.clone()).define("host", "destination", memory.clone());
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    instance.func::<(), ()>(&store, "copy")?.call(&mut store, ())?;
    assert_eq!(memory.read_vec(0, 4)?, b"aabc");
    Ok(())
}

struct DenyGrowth;

impl ResourceLimiter for DenyGrowth {
    fn memory_growing(&self, current: usize, _desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        Ok(current == 0)
    }
}

#[test]
fn shared_memory_growth_respects_store_limiter() -> TestResult {
    let module = tinywasm::parse_bytes(&wat::parse_str(
        r#"
        (module
          (memory (export "memory") 1 2 shared)
          (func (export "grow") (result i32) i32.const 1 memory.grow))
    "#,
    )?)?;
    let mut store = Store::new(Engine::new(Config::new().with_resource_limiter(DenyGrowth)));
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<(), i32>(&store, "grow")?.call(&mut store, ())?, -1);
    assert_eq!(instance.memory_shared("memory")?.lock().page_count(), 1);
    Ok(())
}
