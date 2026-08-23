use std::sync::{Arc, Mutex};
use tinywasm::types::{ExternRef, FuncType, RefType, RefValue, WasmType, WasmValue};
use tinywasm::{HostFunction, Imports, ModuleInstance, Store, Trap};

#[test]
fn direct_references_stay_live_until_the_last_clone_is_dropped() {
    let mut store = Store::default();
    let reference = ExternRef::try_new(&mut store, 7).unwrap();
    let clone = reference.clone();
    drop(reference);
    store.gc().unwrap();

    assert_eq!(clone.key(&store), Ok(7));
}

#[test]
fn roots_and_function_references_reject_another_store() {
    let mut first = Store::default();
    let mut second = Store::default();
    let root = ExternRef::try_new(&mut first, 1).unwrap();
    let function = HostFunction::from(|_, ()| -> tinywasm::Result<()> { Ok(()) }).instantiate(&mut first).unwrap();
    let func_ref = function.as_func_ref(&first).unwrap();
    let accepts_func_ref =
        HostFunction::from(|_, _: Option<tinywasm::types::FuncRef>| -> tinywasm::Result<()> { Ok(()) })
            .instantiate(&mut second)
            .unwrap();

    assert!(matches!(root.key(&second), Err(tinywasm::Error::Trap(Trap::InvalidStore))));
    assert!(matches!(
        accepts_func_ref.call(&mut second, &[WasmValue::Ref(RefValue::Func(func_ref))], &mut []),
        Err(tinywasm::Error::Trap(Trap::InvalidStore))
    ));
}

#[test]
fn callback_results_and_captured_clones_remain_valid() {
    let mut store = Store::default();
    let captured = Arc::new(Mutex::new(None));
    let callback_root = captured.clone();
    let ty = FuncType::new(&[], &[WasmType::Ref(RefType::EXTERNREF)]);
    let function = HostFunction::from_untyped(&ty, move |mut context, _, results| {
        let root = ExternRef::try_new(context.store_mut(), 17)?;
        *callback_root.lock().unwrap() = Some(root.clone());
        results[0] = root.into();
        Ok(())
    })
    .instantiate(&mut store)
    .unwrap();

    let mut results = [WasmValue::Ref(RefValue::Null)];
    function.call(&mut store, &[], &mut results).unwrap();
    let [result] = results;
    let WasmValue::Ref(RefValue::Extern(result)) = result else { panic!("expected externref") };
    assert_eq!(result.key(&store), Ok(17));
    assert_eq!(captured.lock().unwrap().as_ref().unwrap().key(&store), Ok(17));
}

#[test]
fn guest_callback_arguments_are_rooted_before_entering_untyped_host_code() {
    let wasm = wat::parse_str(
        r#"
        (module
          (import "host" "make" (func $make (result (ref extern))))
          (import "host" "check" (func $check (param externref)))
          (func (export "call")
            call $make
            call $check))
        "#,
    )
    .unwrap();
    let module = tinywasm::parse_bytes(&wasm).unwrap();
    let mut store = Store::default();
    let mut imports = Imports::new();
    imports.define("host", "make", HostFunction::from(|mut context, ()| ExternRef::try_new(context.store_mut(), 23)));
    imports.define(
        "host",
        "check",
        HostFunction::from_untyped(
            &FuncType::new(&[WasmType::Ref(RefType::EXTERNREF)], &[]),
            |context, args, _results| {
                let WasmValue::Ref(RefValue::Extern(value)) = &args[0] else { panic!("expected externref") };
                assert_eq!(value.key(context.store()), Ok(23));
                Ok(())
            },
        ),
    );
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports)).unwrap();

    instance.func::<(), ()>(&store, "call").unwrap().call(&mut store, ()).unwrap();
}

#[test]
fn typed_option_reference_signatures_are_nullable() {
    let mut store = Store::default();
    let function =
        HostFunction::from(|_context, value: Option<ExternRef>| -> tinywasm::Result<Option<ExternRef>> { Ok(value) })
            .instantiate(&mut store)
            .unwrap();

    assert_eq!(function.ty(&store).unwrap().params(), &[WasmType::Ref(RefType::EXTERNREF)]);
    let mut results = [WasmValue::Ref(RefValue::Null)];
    function.call(&mut store, &[WasmValue::Ref(RefValue::Null)], &mut results).unwrap();
    assert_eq!(results, [WasmValue::Ref(RefValue::Null)]);
}
