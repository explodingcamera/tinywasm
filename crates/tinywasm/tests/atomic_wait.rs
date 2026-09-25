#![cfg(feature = "parser")]

use tinywasm::{Error, ModuleInstance, Store, Trap};

#[test]
fn ordinary_memory_atomics_and_wait() -> Result<(), Box<dyn core::error::Error>> {
    let wasm = wat::parse_str(
        r#"(module
            (memory 1)
            (func (export "add") (result i32)
                i32.const 0 i32.const 1 i32.atomic.rmw.add)
            (func (export "notify") (result i32)
                i32.const 0 i32.const 1 memory.atomic.notify)
            (func (export "wait") (param i32) (result i32)
                local.get 0 i32.const 0 i64.const 0 memory.atomic.wait32))"#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
    assert_eq!(instance.func::<(), i32>(&store, "add")?.call(&mut store, ())?, 0);
    assert_eq!(instance.func::<(), i32>(&store, "add")?.call(&mut store, ())?, 1);
    assert_eq!(instance.func::<(), i32>(&store, "notify")?.call(&mut store, ())?, 0);
    let wait = instance.func::<i32, i32>(&store, "wait")?;
    assert!(matches!(wait.call(&mut store, 0), Err(Error::Trap(Trap::Other("atomic wait requires shared memory")))));
    assert!(matches!(
        wait.call(&mut store, 65536),
        Err(Error::Trap(Trap::Other("atomic wait requires shared memory")))
    ));
    assert!(matches!(wait.call(&mut store, 1), Err(Error::Trap(Trap::UnalignedAtomic))));
    Ok(())
}
