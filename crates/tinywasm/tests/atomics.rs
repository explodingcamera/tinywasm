use tinywasm::{Error, ModuleInstance, Store, Trap};

type TestResult = Result<(), Box<dyn core::error::Error>>;

#[test]
fn atomic_operations_on_ordinary_memory() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory (export "memory") 1)
          (func (export "run") (result i64)
            i32.const 12
            i32.const 255
            i32.atomic.store8
            i32.const 12
            i32.const 2
            i32.atomic.rmw8.add_u
            drop
            i32.const 12
            i32.const 1
            i32.const 7
            i32.atomic.rmw8.cmpxchg_u
            drop
            i32.const 16
            i64.const -1
            i64.atomic.store32
            i32.const 16
            i64.atomic.load32_u
            atomic.fence))
    "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<(), i64>(&store, "run")?.call(&mut store, ())?, 0xffff_ffff);
    let memory = instance.memory("memory")?;
    assert_eq!(memory.read_vec(&store, 12, 1)?, [7]);
    Ok(())
}

#[test]
fn atomic_alignment_and_bounds() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module (memory 1)
          (func (export "load") (param i32) (result i32)
            local.get 0
            i32.atomic.load offset=2))
    "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let load = instance.func::<i32, i32>(&store, "load")?;
    assert!(matches!(load.call(&mut store, 0), Err(Error::Trap(Trap::UnalignedAtomic))));
    assert!(matches!(load.call(&mut store, 65534), Err(Error::Trap(Trap::MemoryOutOfBounds { .. }))));

    let invalid_alignment = wat::parse_str("(module (memory 1) (func i32.const 0 i32.atomic.load align=1 drop))")?;
    assert!(tinywasm::parse_bytes(&invalid_alignment).is_err());
    Ok(())
}

#[test]
fn atomic_rmw_operations_wrap_at_byte_width() -> TestResult {
    for (op, expected, updated) in [
        ("add", 0xff, 1),
        ("sub", 0xff, 0xfd),
        ("and", 0xff, 2),
        ("or", 0xff, 0xff),
        ("xor", 0xff, 0xfd),
        ("xchg", 0xff, 2),
    ] {
        let wat = format!(
            r#"
            (module (memory 1)
              (func (export "run") (result i32)
                i32.const 0
                i32.const 255
                i32.atomic.store8
                i32.const 0
                i32.const 2
                i32.atomic.rmw8.{op}_u)
              (func (export "read") (result i32)
                i32.const 0
                i32.atomic.load8_u))
        "#
        );
        let module = tinywasm::parse_bytes(&wat::parse_str(&wat)?)?;
        let mut store = Store::default();
        let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
        assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, expected);
        assert_eq!(instance.func::<(), i32>(&store, "read")?.call(&mut store, ())?, updated);
    }
    Ok(())
}

#[test]
fn narrow_cmpxchg_truncates_expected_and_preserves_old_value() -> TestResult {
    let wasm = wat::parse_str(
        r#"
        (module
          (memory (export "memory") 1)
          (func (export "exchange") (param i64 i64) (result i64)
            i32.const 4
            local.get 0
            local.get 1
            i64.atomic.rmw32.cmpxchg_u))
    "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    let memory = instance.memory("memory")?;
    let exchange = instance.func::<(i64, i64), i64>(&store, "exchange")?;
    memory.copy_from_slice(&mut store, 4, &0x89ab_cdef_u32.to_le_bytes())?;

    assert_eq!(exchange.call(&mut store, (0x1234_89ab_cdef, 7))?, 0x89ab_cdef);
    assert_eq!(exchange.call(&mut store, (0x89ab_cdef, 9))?, 7);
    assert_eq!(memory.read_vec(&store, 4, 4)?, 7u32.to_le_bytes());
    Ok(())
}
