use eyre::Result;
use tinywasm::{Module, Store};

#[test]
fn memory_ref_mut_copy_within_uses_src_then_dst_order() -> Result<()> {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory (export "memory") 1)
        )
        "#,
    )?;

    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = module.instantiate(&mut store, None)?;

    let memory = instance.memory("memory")?;
    memory.copy_from_slice(&mut store, 0, &[1, 2, 3, 4])?;
    memory.copy_within(&mut store, 0, 4, 4)?;

    assert_eq!(memory.read_vec(&store, 0, 8)?, &[1, 2, 3, 4, 1, 2, 3, 4]);

    Ok(())
}
