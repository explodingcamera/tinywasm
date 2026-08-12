extern crate alloc;

use alloc::sync::Arc;
use core::sync::atomic::{AtomicUsize, Ordering};

#[cfg(feature = "std")]
use std::io::{Read, Seek, SeekFrom, Write};

use tinywasm::engine::Config;
use tinywasm::types::{MemoryArch, MemoryType};
use tinywasm::{Engine, Memory, MemoryBackend, Module, ModuleInstance, PagedMemory, Store};
use tinywasm_parser::{Parser, ParserOptions};

type TestResult<T = ()> = Result<T, Box<dyn core::error::Error>>;

fn initial_memory_size(ty: MemoryType) -> usize {
    usize::try_from(ty.page_count_initial())
        .ok()
        .and_then(|pages| pages.checked_mul(usize::try_from(ty.page_size()).ok()?))
        .expect("test memory size should fit usize")
}

fn instantiate_module_with_counting_backend(module: Module) -> TestResult<usize> {
    let created = Arc::new(AtomicUsize::new(0));
    let factory_calls = created.clone();
    let backend = MemoryBackend::custom(move |ty| {
        factory_calls.fetch_add(1, Ordering::Relaxed);
        PagedMemory::try_new(initial_memory_size(ty), 16)
    });
    let engine = Engine::new(Config::new().with_memory_backend(backend));
    let mut store = Store::new(engine);

    let _ = ModuleInstance::instantiate(&mut store, &module, None)?;

    Ok(created.load(Ordering::Relaxed))
}

fn instantiate_with_counting_backend(wat: &str) -> TestResult<usize> {
    let wasm = wat::parse_str(wat)?;
    let module = tinywasm::parse_bytes(&wasm)?;
    instantiate_module_with_counting_backend(module)
}

fn instantiate_exported_memory_with_counting_backend(
    wat: &str,
) -> TestResult<(Store, tinywasm::ModuleInstance, Arc<AtomicUsize>)> {
    let wasm = wat::parse_str(wat)?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let created = Arc::new(AtomicUsize::new(0));
    let factory_calls = created.clone();
    let backend = MemoryBackend::custom(move |ty| {
        factory_calls.fetch_add(1, Ordering::Relaxed);
        PagedMemory::try_new(initial_memory_size(ty), 16)
    });
    let engine = Engine::new(Config::new().with_memory_backend(backend));
    let mut store = Store::new(engine);
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    Ok((store, instance, created))
}

#[test]
fn paged_backend_works_for_module_memories() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory (export "memory") 1)
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;
    let config = Config::new().with_memory_backend(MemoryBackend::paged(8));
    let mut store = Store::new(Engine::new(config));
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let memory = instance.memory("memory")?;

    memory.copy_from_slice(&mut store, 6, &[1, 2, 3, 4, 5, 6, 7, 8])?;
    assert_eq!(memory.read_vec(&store, 6, 8)?, &[1, 2, 3, 4, 5, 6, 7, 8]);

    Ok(())
}

#[test]
fn custom_backend_factory_is_used_for_host_memories() -> TestResult {
    let created = Arc::new(AtomicUsize::new(0));
    let seen_page_size = Arc::new(AtomicUsize::new(0));
    let factory_calls = created.clone();
    let page_size_seen = seen_page_size.clone();

    let backend = MemoryBackend::custom(move |ty| {
        factory_calls.fetch_add(1, Ordering::Relaxed);
        page_size_seen.store(ty.page_size() as usize, Ordering::Relaxed);
        PagedMemory::try_new(initial_memory_size(ty), 16)
    });

    let engine = Engine::new(Config::new().with_memory_backend(backend));
    let mut store = Store::new(engine);

    let memory = Memory::new(&mut store, MemoryType::new(MemoryArch::I32, 1, Some(2), Some(32)))?;
    assert_eq!(memory.ty(&store)?.page_size(), 32);
    memory.copy_from_slice(&mut store, 12, &[9, 8, 7, 6, 5])?;

    assert_eq!(memory.read_vec(&store, 12, 5)?, &[9, 8, 7, 6, 5]);
    assert_eq!(created.load(Ordering::Relaxed), 1);
    assert_eq!(seen_page_size.load(Ordering::Relaxed), 32);

    Ok(())
}

#[test]
fn local_memory_without_observable_use_is_not_allocated() -> TestResult {
    let created = instantiate_with_counting_backend(
        r#"
        (module
          (memory 1)
          (func (export "run"))
        )
        "#,
    )?;

    assert_eq!(created, 0);
    Ok(())
}

#[test]
fn exported_local_memory_is_not_eagerly_allocated() -> TestResult {
    let created = instantiate_with_counting_backend(
        r#"
        (module
          (memory (export "memory") 1)
        )
        "#,
    )?;

    assert_eq!(created, 0);
    Ok(())
}

#[test]
fn exported_local_memory_reads_zeroes_without_materializing() -> TestResult {
    let (mut store, instance, created) = instantiate_exported_memory_with_counting_backend(
        r#"
        (module
          (memory (export "memory") 1)
        )
        "#,
    )?;

    let memory = instance.memory("memory")?;
    assert_eq!(created.load(Ordering::Relaxed), 0);
    assert_eq!(memory.len(&store)?, 65536);
    assert_eq!(memory.read_vec(&store, 65534, 2)?, &[0, 0]);
    assert_eq!(created.load(Ordering::Relaxed), 0);

    memory.copy_from_slice(&mut store, 65534, &[1, 2])?;
    assert_eq!(created.load(Ordering::Relaxed), 1);
    assert_eq!(memory.read_vec(&store, 65534, 2)?, &[1, 2]);
    Ok(())
}

#[test]
fn active_data_segment_on_local_memory_is_allocated() -> TestResult {
    let created = instantiate_with_counting_backend(
        r#"
        (module
          (memory 1)
          (data (i32.const 0) "hi")
        )
        "#,
    )?;

    assert_eq!(created, 1);
    Ok(())
}

#[test]
fn local_memory_instruction_is_allocated() -> TestResult {
    let created = instantiate_with_counting_backend(
        r#"
        (module
          (memory 1)
          (func (export "run") (drop (memory.size)))
        )
        "#,
    )?;

    assert_eq!(created, 1);
    Ok(())
}

#[test]
fn disabled_local_memory_allocation_optimization_keeps_old_behavior() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory 1)
          (func (export "run"))
        )
        "#,
    )?;
    let parser = Parser::new(ParserOptions::default().with_local_memory_allocation_optimization(false));
    let module = parser.parse_module_bytes(&wasm)?;

    let created = instantiate_module_with_counting_backend(module)?;

    assert_eq!(created, 1);
    Ok(())
}

#[test]
fn read_returns_short_count_at_end_of_memory() -> TestResult {
    let mut store = Store::default();
    let memory = Memory::new(&mut store, MemoryType::new(MemoryArch::I32, 1, Some(1), Some(4)))?;
    memory.copy_from_slice(&mut store, 0, &[1, 2, 3, 4])?;

    let mut dst = [9; 8];
    assert_eq!(memory.read(&store, 2, &mut dst)?, 2);
    assert_eq!(&dst[..2], &[3, 4]);
    assert_eq!(&dst[2..], &[9; 6]);

    Ok(())
}

#[test]
fn paged_read_and_write_stop_at_chunk_boundaries() -> TestResult {
    let engine = Engine::new(Config::new().with_memory_backend(MemoryBackend::paged(4)));
    let mut store = Store::new(engine);
    let memory = Memory::new(&mut store, MemoryType::new(MemoryArch::I32, 1, Some(1), Some(16)))?;

    memory.copy_from_slice(&mut store, 0, &[1, 2, 3, 4, 5, 6, 7, 8])?;

    let mut read_buf = [9; 6];
    assert_eq!(memory.read(&store, 2, &mut read_buf)?, 2);
    assert_eq!(&read_buf[..2], &[3, 4]);
    assert_eq!(&read_buf[2..], &[9; 4]);

    let mut exact_buf = [0; 6];
    memory.read_exact(&store, 2, &mut exact_buf)?;
    assert_eq!(exact_buf, [3, 4, 5, 6, 7, 8]);

    assert_eq!(memory.write(&mut store, 6, &[10, 11, 12, 13])?, 2);
    assert_eq!(memory.read_vec(&store, 6, 4)?, &[10, 11, 0, 0]);

    memory.copy_from_slice(&mut store, 6, &[20, 21, 22, 23])?;
    assert_eq!(memory.read_vec(&store, 6, 4)?, &[20, 21, 22, 23]);

    Ok(())
}

#[test]
fn paged_fixed_width_accesses_cross_chunk_boundaries() -> TestResult {
    use tinywasm::LinearMemory;

    let mut memory = PagedMemory::try_new(32, 4).map_err(tinywasm::Error::from)?;
    memory.write_32(2, &0x1234_5678u32.to_le_bytes()).map_err(tinywasm::Error::from)?;
    memory.write_64(6, &0x0123_4567_89ab_cdefu64.to_le_bytes()).map_err(tinywasm::Error::from)?;
    memory.write_128(14, &u128::MAX.to_le_bytes()).map_err(tinywasm::Error::from)?;

    assert_eq!(u32::from_le_bytes(memory.read_32(2).map_err(tinywasm::Error::from)?), 0x1234_5678);
    assert_eq!(u64::from_le_bytes(memory.read_64(6).map_err(tinywasm::Error::from)?), 0x0123_4567_89ab_cdef);
    assert_eq!(u128::from_le_bytes(memory.read_128(14).map_err(tinywasm::Error::from)?), u128::MAX);
    Ok(())
}

#[test]
fn lazy_custom_backend_creation_trap_is_propagated() -> TestResult {
    let wasm = wat::parse_str(r#"(module (memory (export "memory") 1))"#)?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let backend = MemoryBackend::custom(|_| Err::<PagedMemory, _>(tinywasm::Trap::Other("backend unavailable")));
    let mut store = Store::new(Engine::new(Config::new().with_memory_backend(backend)));
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let memory = instance.memory("memory")?;

    assert_eq!(
        memory.copy_from_slice(&mut store, 0, &[1]).unwrap_err(),
        tinywasm::Error::from(tinywasm::Trap::Other("backend unavailable"))
    );
    Ok(())
}

#[test]
fn memory64_default_limit_is_not_memory32_limit() {
    let ty = MemoryType::new(MemoryArch::I64, 65_537, None, None);
    assert!(ty.page_count_max() > 65_536);
}

#[cfg(feature = "std")]
#[test]
fn memory_cursor_supports_read_write_and_seek() -> TestResult {
    let mut store = Store::default();
    let memory = Memory::new(&mut store, MemoryType::new(MemoryArch::I32, 1, Some(1), Some(8)))?;

    let mut cursor = memory.cursor(&mut store)?;
    cursor.seek(SeekFrom::Start(2))?;
    cursor.write_all(b"abc")?;
    cursor.seek(SeekFrom::Start(0))?;

    let mut buf = [0; 5];
    cursor.read_exact(&mut buf)?;
    assert_eq!(buf, [0, 0, b'a', b'b', b'c']);

    cursor.seek(SeekFrom::End(-1))?;
    cursor.write_all(b"z")?;

    assert_eq!(memory.read_vec(&store, 0, 8)?, &[0, 0, b'a', b'b', b'c', 0, 0, b'z']);
    Ok(())
}
