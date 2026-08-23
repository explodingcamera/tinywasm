use tinywasm::types::{ExternRef, RefValue, WasmValue};
use tinywasm::{Engine, ExecProgress, ModuleInstance, Store, engine::Config};

const MODULE: &str = r#"
    (module
      (type $node (struct (field i32)))
      (type $bytes (array (mut i8)))
      (func (export "new") (result anyref)
        (struct.new $node (i32.const 42)))
      (func (export "new-extern") (result externref)
        (extern.convert_any (struct.new $node (i32.const 42))))
      (func (export "read") (param anyref) (result i32)
        (struct.get $node 0 (ref.cast (ref $node) (local.get 0))))
      (func (export "read-extern") (param externref) (result i32)
        (struct.get $node 0
          (ref.cast (ref $node) (any.convert_extern (local.get 0)))))
      (func (export "churn")
        (local $i i32)
        (loop $loop
          (drop (array.new_default $bytes (i32.const 16)))
          (local.set $i (i32.add (local.get $i) (i32.const 1)))
          (br_if $loop (i32.lt_u (local.get $i) (i32.const 100)))))
    )
"#;

fn store() -> Store {
    Store::new(Engine::new(Config::new().with_gc_collection_threshold(1)))
}

#[test]
fn host_result_is_pinned_and_rejected_by_another_store() {
    let module = tinywasm::parse_bytes(&wat::parse_str(MODULE).unwrap()).unwrap();
    let mut first_store = store();
    let first = ModuleInstance::instantiate(&mut first_store, &module, None).unwrap();
    let new = first.func_untyped(&first_store, "new").unwrap();
    let read = first.func_untyped(&first_store, "read").unwrap();
    let churn = first.func_untyped(&first_store, "churn").unwrap();

    let value = new.call(&mut first_store, &[]).unwrap().pop().unwrap();
    assert!(matches!(value, WasmValue::Ref(RefValue::Any(_))));
    churn.call(&mut first_store, &[]).unwrap();
    assert_eq!(read.call(&mut first_store, &[value]).unwrap(), [WasmValue::I32(42)]);

    let mut second_store = store();
    let second = ModuleInstance::instantiate(&mut second_store, &module, None).unwrap();
    let other_read = second.func_untyped(&second_store, "read").unwrap();
    assert!(other_read.call(&mut second_store, &[value]).is_err());
}

#[test]
fn externalized_gc_result_is_pinned() {
    let module = tinywasm::parse_bytes(&wat::parse_str(MODULE).unwrap()).unwrap();
    let mut store = store();
    let instance = ModuleInstance::instantiate(&mut store, &module, None).unwrap();
    let new = instance.func_untyped(&store, "new-extern").unwrap();
    let read = instance.func_untyped(&store, "read-extern").unwrap();
    let churn = instance.func_untyped(&store, "churn").unwrap();

    let value = new.call(&mut store, &[]).unwrap().pop().unwrap();
    assert!(matches!(value, WasmValue::Ref(RefValue::Extern(_))));
    churn.call(&mut store, &[]).unwrap();
    assert_eq!(read.call(&mut store, &[value]).unwrap(), [WasmValue::I32(42)]);
}

#[test]
fn host_externref_does_not_alias_a_gc_object() {
    let module = tinywasm::parse_bytes(&wat::parse_str(MODULE).unwrap()).unwrap();
    let mut store = store();
    let instance = ModuleInstance::instantiate(&mut store, &module, None).unwrap();
    let new = instance.func_untyped(&store, "new").unwrap();
    let read = instance.func_untyped(&store, "read-extern").unwrap();

    new.call(&mut store, &[]).unwrap();
    let host_ref = WasmValue::Ref(RefValue::Extern(ExternRef::new(0)));
    assert!(read.call(&mut store, &[host_ref]).is_err());
}

#[test]
fn resumable_gc_result_is_pinned() {
    let module = tinywasm::parse_bytes(&wat::parse_str(MODULE).unwrap()).unwrap();
    let mut store = store();
    let instance = ModuleInstance::instantiate(&mut store, &module, None).unwrap();
    let new = instance.func_untyped(&store, "new").unwrap();
    let read = instance.func_untyped(&store, "read").unwrap();
    let churn = instance.func_untyped(&store, "churn").unwrap();

    let mut execution = new.call_resumable(&mut store, &[]).unwrap();
    let value = match execution.resume_with_fuel(1000).unwrap() {
        ExecProgress::Completed(mut values) => values.pop().unwrap(),
        ExecProgress::Suspended => panic!("constructor unexpectedly suspended"),
    };
    drop(execution);

    churn.call(&mut store, &[]).unwrap();
    assert_eq!(read.call(&mut store, &[value]).unwrap(), [WasmValue::I32(42)]);
}

#[test]
fn element_initializers_root_previous_gc_values() {
    let module = tinywasm::parse_bytes(
        &wat::parse_str(
            r#"
                (module
                  (type $node (struct (field i32)))
                  (table 2 (ref null $node))
                  (elem (i32.const 0) (ref $node)
                    (struct.new $node (i32.const 1))
                    (struct.new $node (i32.const 2)))
                  (func (export "first") (result i32)
                    (struct.get $node 0 (table.get 0 (i32.const 0)))))
            "#,
        )
        .unwrap(),
    )
    .unwrap();
    let mut store = store();
    let instance = ModuleInstance::instantiate(&mut store, &module, None).unwrap();

    assert_eq!(instance.func_untyped(&store, "first").unwrap().call(&mut store, &[]).unwrap(), [WasmValue::I32(1)]);
}
