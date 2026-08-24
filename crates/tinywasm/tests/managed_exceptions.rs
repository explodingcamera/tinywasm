use std::sync::{Arc, Mutex};

use tinywasm::engine::Config;
use tinywasm::types::{FuncType, RefType, RefValue, WasmType, WasmValue};
use tinywasm::{Engine, Error, ExnRef, ModuleInstance, ResourceLimiter, Store, Tag, Trap};

const MODULE: &str = r#"
    (module
      (tag $scalar (export "scalar-tag") (param i32))
      (tag $graph (export "graph-tag") (param anyref))
      (type $node (struct (field i32)))

      (func (export "throw-scalar") (param i32)
        local.get 0
        throw $scalar)

      (func (export "catch-scalar") (param i32)
        (drop
          (block $caught (result i32)
            (try_table (result i32) (catch $scalar $caught)
              local.get 0
              throw $scalar
              i32.const 0))))

      (func (export "throw-graph")
        i32.const 42
        struct.new $node
        throw $graph)

      (func (export "rethrow") (param exnref)
        local.get 0
        throw_ref)

      (func (export "read-node") (param anyref) (result i32)
        local.get 0
        ref.cast (ref $node)
        struct.get $node 0)
    )
"#;

fn instantiate(store: &mut Store) -> ModuleInstance {
    let wasm = wat::parse_str(MODULE).unwrap();
    let module = tinywasm::parse_bytes(&wasm).unwrap();
    ModuleInstance::instantiate(store, &module, None).unwrap()
}

fn exception(error: Error) -> ExnRef {
    let Error::Exception(exception) = error else { panic!("expected exception") };
    exception
}

#[test]
fn exception_accessors_validate_store_and_expose_payload() {
    let mut store = Store::default();
    let instance = instantiate(&mut store);
    let throw = instance.func_untyped(&store, "throw-scalar").unwrap();
    let exception = exception(throw.call(&mut store, &[WasmValue::I32(17)], &mut []).unwrap_err());

    assert_eq!(exception.tag(&store).unwrap(), instance.tag("scalar-tag").unwrap());
    assert_eq!(exception.field(&mut store, 0).unwrap(), WasmValue::I32(17));
    assert_eq!(exception.fields(&mut store).unwrap(), [WasmValue::I32(17)]);
    assert!(exception.field(&mut store, 1).is_err());

    let other = Store::default();
    assert_eq!(exception.tag(&other).unwrap_err(), Error::Trap(Trap::InvalidStore));
}

#[test]
fn exception_roots_keep_payload_graphs_live() {
    let mut store = Store::new(Engine::new(Config::new().with_gc_collection_threshold(1)));
    let instance = instantiate(&mut store);
    let throw = instance.func_untyped(&store, "throw-graph").unwrap();
    let read = instance.func_untyped(&store, "read-node").unwrap();
    let exception = exception(throw.call(&mut store, &[], &mut []).unwrap_err());

    store.gc().unwrap();
    let payload = exception.field(&mut store, 0).unwrap();
    assert!(matches!(payload, WasmValue::Ref(RefValue::Any(_))));
    let mut results = [WasmValue::I32(0)];
    read.call(&mut store, &[payload], &mut results).unwrap();
    assert_eq!(results, [WasmValue::I32(42)]);
}

#[test]
fn owned_exception_references_survive_collection_and_reject_another_store() {
    let mut store = Store::default();
    let instance = instantiate(&mut store);
    let throw = instance.func_untyped(&store, "throw-scalar").unwrap();
    let first = exception(throw.call(&mut store, &[WasmValue::I32(1)], &mut []).unwrap_err());

    store.gc().unwrap();
    let current = exception(throw.call(&mut store, &[WasmValue::I32(2)], &mut []).unwrap_err());
    assert_eq!(current.field(&mut store, 0).unwrap(), WasmValue::I32(2));
    assert_eq!(first.field(&mut store, 0).unwrap(), WasmValue::I32(1));
    let rethrow = instance.func_untyped(&store, "rethrow").unwrap();
    let rethrown = exception(rethrow.call(&mut store, &[WasmValue::Ref(RefValue::Exn(first))], &mut []).unwrap_err());
    assert_eq!(rethrown.field(&mut store, 0).unwrap(), WasmValue::I32(1));

    let mut other = Store::default();
    let other_instance = instantiate(&mut other);
    let other_rethrow = other_instance.func_untyped(&other, "rethrow").unwrap();
    assert_eq!(
        other_rethrow.call(&mut other, &[WasmValue::Ref(RefValue::Exn(current))], &mut []).unwrap_err(),
        Error::Trap(Trap::InvalidStore)
    );
}

#[test]
fn host_tags_reject_unbranded_concrete_types() {
    let mut store = Store::default();
    let ty = FuncType::new(&[WasmType::Ref(RefType::new_concrete(true, 0))], &[]);

    assert!(Tag::try_new(&mut store, ty).is_err());
}

struct GcUsage(Arc<Mutex<Vec<(usize, usize)>>>);

impl ResourceLimiter for GcUsage {
    fn gc_growing(&self, current: usize, desired: usize, _maximum: Option<usize>) -> Result<bool, Trap> {
        self.0.lock().unwrap().push((current, desired));
        Ok(true)
    }
}

#[test]
fn dropped_exception_roots_release_counted_gc_bytes() {
    let usage = Arc::new(Mutex::new(Vec::new()));
    let config =
        Config::new().with_gc_collection_threshold(usize::MAX).with_resource_limiter(Arc::new(GcUsage(usage.clone())));
    let mut store = Store::new(Engine::new(config));
    let instance = instantiate(&mut store);
    let catch = instance.func_untyped(&store, "catch-scalar").unwrap();

    for value in 0..32 {
        catch.call(&mut store, &[WasmValue::I32(value)], &mut []).unwrap();
    }
    store.gc().unwrap();
    usage.lock().unwrap().clear();

    catch.call(&mut store, &[WasmValue::I32(33)], &mut []).unwrap();
    assert_eq!(usage.lock().unwrap()[0].0, 0);
}
