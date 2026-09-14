use anyhow::Result;
use tinywasm::{FuncContext, HostFunction, Imports, ModuleInstance, Store};

const WASM: &str = r#"
(module
  (import "host" "record" (func $record (param i32) (result i32)))

  (func (export "run") (result i32)
    i32.const 20
    call $record
    drop
    i32.const 22
    call $record))
"#;

#[derive(Default)]
struct State {
    calls: u32,
    total: i32,
}

// Requires the `parser`, `std`, and `state` features.
// Run with `cargo run --example state` from the repository root.
fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM)?;
    let module = tinywasm::parse_bytes(&wasm)?;

    // State is owned by the Store and is available to its host-function calls.
    let mut store = Store::default().with_state(State::default());

    let record = HostFunction::from(|mut ctx: FuncContext<'_>, value: i32| {
        // FuncContext accesses the active Store without captured shared state.
        let state = ctx.state_mut::<State>().expect("State was added to the store");
        state.calls += 1;
        state.total += value;
        Ok(state.total)
    });

    // The import definition captures no state and can be reused with other stores.
    let mut imports = Imports::new();
    imports.define("host", "record", record);

    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    let run = instance.func::<(), i32>(&store, "run")?;

    assert_eq!(run.call(&mut store, ())?, 42);

    // Host code can inspect the same state after guest execution.
    assert_eq!(store.state::<State>().unwrap().calls, 2);
    Ok(())
}
