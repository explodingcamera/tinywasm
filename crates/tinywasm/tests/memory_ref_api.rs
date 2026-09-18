use tinywasm::{ModuleInstance, Store};

#[test]
fn memory_ref_mut_copy_within_uses_src_then_dst_order() -> Result<(), Box<dyn core::error::Error>> {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory (export "memory") 1)
        )
        "#,
    )?;

    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

    let memory = instance.memory("memory")?;
    memory.copy_from_slice(&mut store, 0, &[1, 2, 3, 4])?;
    memory.copy_within(&mut store, 0, 4, 4)?;

    assert_eq!(memory.read_vec(&store, 0, 8)?, &[1, 2, 3, 4, 1, 2, 3, 4]);

    Ok(())
}

#[test]
fn read_js_string_decodes_surrogate_pairs() -> Result<(), Box<dyn core::error::Error>> {
    let mut store = Store::default();
    let memory =
        tinywasm::Memory::try_new(&mut store, tinywasm::types::MemoryType::default().with_page_count_initial(1))?;
    memory.copy_from_slice(&mut store, 0, &[0x3d, 0xd8, 0x00, 0xde])?;

    assert_eq!(memory.read_js_string(&store, 0, 4)?, "😀");
    assert!(memory.read_js_string(&store, 0, 3).is_err());

    Ok(())
}
