#![cfg(feature = "state")]

use tinywasm::types::WasmValue;
use tinywasm::{HostFunction, Store};

struct Counter(i32);
struct Label(&'static str);

#[test]
fn stores_state_by_type() {
    let mut store = Store::default().with_state(Counter(1)).with_state(Label("first")).with_state(Counter(2));

    assert_eq!(store.state::<Counter>().unwrap().0, 2);
    assert_eq!(store.state::<Label>().unwrap().0, "first");

    store.state_mut::<Counter>().unwrap().0 = 3;
    assert_eq!(store.state::<Counter>().unwrap().0, 3);
}

#[test]
fn host_function_mutates_store_state() -> tinywasm::Result<()> {
    let mut store = Store::default().with_state(Counter(40));
    let function = HostFunction::from(|mut ctx, value: i32| {
        let counter = ctx.state_mut::<Counter>().expect("counter state");
        counter.0 += value;
        Ok(counter.0)
    })
    .instantiate(&mut store)?;

    let mut results = [WasmValue::I32(0)];
    function.call(&mut store, &[WasmValue::I32(2)], &mut results)?;

    assert_eq!(results, [WasmValue::I32(42)]);
    assert_eq!(store.state::<Counter>().unwrap().0, 42);
    Ok(())
}
