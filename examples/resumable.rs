use eyre::Result;
use tinywasm::{ExecProgress, Module, Store};

const WASM: &str = r#"
(module
  (func (export "count_down") (param $n i32) (result i32)
    (local $cur i32)
    local.get $n
    local.set $cur
    block
      loop
        local.get $cur
        i32.eqz
        br_if 1
        local.get $cur
        i32.const 1
        i32.sub
        local.set $cur
        br 0
      end
    end
    local.get $cur))
"#;

fn main() -> Result<()> {
    let wasm = wat::parse_str(WASM)?;
    let module = Module::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = module.instantiate(&mut store, None)?;
    let count_down = instance.exported_func::<i32, i32>(&store, "count_down")?;

    let mut execution = count_down.call_resumable(&mut store, 10_000)?;
    let fuel_per_round = 128;
    let mut fuel_rounds = 0;

    let result = loop {
        fuel_rounds += 1;
        match execution.resume_with_fuel(fuel_per_round)? {
            ExecProgress::Completed(value) => break value,
            ExecProgress::Suspended => {}
        }
    };

    println!("completed in {fuel_rounds} rounds of {fuel_per_round} fuel, result={result}");
    Ok(())
}
