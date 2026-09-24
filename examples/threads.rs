use anyhow::Result;
use tinywasm::types::{MemoryArch, MemoryType};
use tinywasm::{Imports, MemoryShared, ModuleInstance, Store};

const WASM: &str = r#"
(module
  (import "host" "memory" (memory 1 2 shared))
  (func (export "increment")
    i32.const 0
    i32.const 1
    i32.atomic.rmw.add
    drop))
"#;

fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM)?;
    let module = tinywasm::parse_bytes(&wasm)?;

    // Prepare one shared memory and import set for all workers.
    let memory = MemoryShared::try_new(MemoryType::new(MemoryArch::I32, 1, Some(2), None))?;
    let mut imports = Imports::new();
    imports.define("host", "memory", memory.clone());

    std::thread::scope(|scope| -> Result<()> {
        let mut workers = Vec::new();
        for _ in 0..4 {
            let module = &module;
            let imports = &imports;
            workers.push(scope.spawn(move || -> Result<()> {
                // Each worker has its own store and module instance, but imports the same bytes.
                let mut store = Store::default();
                let instance = ModuleInstance::instantiate(&mut store, module, Some(imports))?;
                let increment = instance.func::<(), ()>(&store, "increment")?;
                for _ in 0..1000 {
                    increment.call(&mut store, ())?;
                }
                Ok(())
            }));
        }
        for worker in workers {
            worker.join().expect("worker thread panicked")?;
        }
        Ok(())
    })?;

    // Atomic read-modify-write operations serialize updates to the shared memory.
    let mut guard = memory.lock();
    assert_eq!(guard.data()[..4], 4000u32.to_le_bytes());
    assert_eq!(guard.grow(1)?, Some(1));
    assert_eq!(guard.page_count(), 2);
    Ok(())
}
