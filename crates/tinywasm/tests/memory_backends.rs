use core::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(feature = "std")]
use std::io::{Read, Seek, SeekFrom, Write};

use eyre::Result;
use tinywasm::engine::Config;
use tinywasm::types::{MemoryArch, MemoryType};
use tinywasm::{Engine, Memory, MemoryBackend, Module, PagedMemory, Store};
use tinywasm_parser::{Parser, ParserOptions};

fn instantiate_module_with_counting_backend(module: Module) -> Result<usize> {
    let created = Arc::new(AtomicUsize::new(0));
    let factory_calls = created.clone();
    let backend = MemoryBackend::custom(move |ty| {
        factory_calls.fetch_add(1, Ordering::Relaxed);
        Ok(PagedMemory::new(ty.initial_size() as usize, 16))
    });
    let engine = Engine::new(Config::new().with_memory_backend(backend));
    let mut store = Store::new(engine);

    let _ = module.instantiate(&mut store, None)?;

    Ok(created.load(Ordering::Relaxed))
}

fn instantiate_with_counting_backend(wat: &str) -> Result<usize> {
    let wasm = wat::parse_str(wat)?;
    let module = Module::parse_bytes(&wasm)?;
    instantiate_module_with_counting_backend(module)
}

fn instantiate_exported_memory_with_counting_backend(
    wat: &str,
) -> Result<(Store, tinywasm::ModuleInstance, Arc<AtomicUsize>)> {
    let wasm = wat::parse_str(wat)?;
    let module = Module::parse_bytes(&wasm)?;
    let created = Arc::new(AtomicUsize::new(0));
    let factory_calls = created.clone();
    let backend = MemoryBackend::custom(move |ty| {
        factory_calls.fetch_add(1, Ordering::Relaxed);
        Ok(PagedMemory::new(ty.initial_size() as usize, 16))
    });
    let engine = Engine::new(Config::new().with_memory_backend(backend));
    let mut store = Store::new(engine);
    let instance = module.instantiate(&mut store, None)?;
    Ok((store, instance, created))
}

#[test]
fn paged_backend_works_for_module_memories() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory (export "memory") 1)
        )
        "#,
    )?;

    let module = Module::parse_bytes(&wasm)?;
    let config = Config::new().with_memory_backend(MemoryBackend::paged(8));
    let mut store = Store::new(Engine::new(config));
    let instance = module.instantiate(&mut store, None)?;
    let memory = instance.memory("memory")?;

    memory.copy_from_slice(&mut store, 6, &[1, 2, 3, 4, 5, 6, 7, 8])?;
    assert_eq!(memory.read_vec(&store, 6, 8)?, &[1, 2, 3, 4, 5, 6, 7, 8]);

    Ok(())
}

#[test]
fn custom_backend_factory_is_used_for_host_memories() -> Result<()> {
    let created = Arc::new(AtomicUsize::new(0));
    let seen_page_size = Arc::new(AtomicUsize::new(0));
    let factory_calls = created.clone();
    let page_size_seen = seen_page_size.clone();

    let backend = MemoryBackend::custom(move |ty| {
        factory_calls.fetch_add(1, Ordering::Relaxed);
        page_size_seen.store(ty.page_size() as usize, Ordering::Relaxed);
        Ok(PagedMemory::new(ty.initial_size() as usize, 16))
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
fn local_memory_without_observable_use_is_not_allocated() -> Result<()> {
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
fn exported_local_memory_is_not_eagerly_allocated() -> Result<()> {
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
fn exported_local_memory_materializes_on_first_method_call() -> Result<()> {
    let (store, instance, created) = instantiate_exported_memory_with_counting_backend(
        r#"
        (module
          (memory (export "memory") 1)
        )
        "#,
    )?;

    let memory = instance.memory("memory")?;
    assert_eq!(created.load(Ordering::Relaxed), 0);
    assert_eq!(memory.len(&store)?, 65536);
    assert_eq!(created.load(Ordering::Relaxed), 1);
    Ok(())
}

#[test]
fn active_data_segment_on_local_memory_is_allocated() -> Result<()> {
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
fn local_memory_instruction_is_allocated() -> Result<()> {
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
fn disabled_local_memory_allocation_optimization_keeps_old_behavior() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory 1)
          (func (export "run"))
        )
        "#,
    )?;
    let parser = Parser::with_options(ParserOptions::default().with_local_memory_allocation_optimization(false));
    let module = Module::from(parser.parse_module_bytes(&wasm)?);

    let created = instantiate_module_with_counting_backend(module)?;

    assert_eq!(created, 1);
    Ok(())
}

#[test]
fn read_returns_short_count_at_end_of_memory() -> Result<()> {
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
fn paged_read_and_write_stop_at_chunk_boundaries() -> Result<()> {
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

#[cfg(feature = "std")]
#[test]
fn memory_cursor_supports_read_write_and_seek() -> Result<()> {
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
