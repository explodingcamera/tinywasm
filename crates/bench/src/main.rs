use std::time::Instant;

use tinywasm::{FuncContext, HostFunction, Imports, ModuleInstance, Store};

const COREMARK: &[u8] = include_bytes!("../fixtures/coremark-minimal.wasm");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !matches!(std::env::args().nth(1).as_deref(), Some("coremark")) {
        return Err("usage: cargo run --release -p bench -- coremark".into());
    }

    let module = tinywasm::parse_bytes(COREMARK)?;
    let clock = Instant::now();
    let mut imports = Imports::new();
    imports.define(
        "env",
        "clock_ms",
        HostFunction::from(move |_: FuncContext<'_>, (): ()| Ok(clock.elapsed().as_millis() as i64)),
    );

    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    let run = instance.func::<(), f32>(&store, "run")?;
    let start = Instant::now();
    let score = run.call(&mut store, ())?;
    if !score.is_finite() || score <= 0.0 {
        return Err(format!("invalid CoreMark score: {score}").into());
    }
    println!("CoreMark: {score:.2} (elapsed: {:.2}s)", start.elapsed().as_secs_f64());
    Ok(())
}
