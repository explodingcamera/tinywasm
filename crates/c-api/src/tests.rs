use crate::{externals::*, function::*, module::*, objects::*, runtime::*, types::*, values::*, vectors::*};
use std::{cell::Cell, ffi::c_void, ptr};

struct Environment {
    nested: Cell<*mut wasm_func_t>,
    memory: Cell<*mut wasm_memory_t>,
    depth: Cell<u32>,
    calls: Cell<u32>,
    finalized: Cell<bool>,
}

unsafe extern "C" fn callback(
    env: *mut c_void,
    args: *const wasm_val_vec_t,
    results: *mut wasm_val_vec_t,
) -> *mut wasm_trap_t {
    let env = unsafe { &*env.cast::<Environment>() };
    env.depth.set(env.depth.get() + 1);
    env.calls.set(env.calls.get() + 1);
    unsafe {
        if env.depth.get() == 3 {
            (*results).data.write(wasm_val_t { kind: 0, of: wasm_val_union { i32: (*(*args).data).of.i32 + 1 } });
        } else {
            assert!(wasm_func_call(env.nested.get(), args, results).is_null());
        }
        let data = wasm_memory_data(env.memory.get());
        *data += 1;
        assert_eq!(wasm_memory_size(env.memory.get()), 1);
        assert!(*data > 0);
    }
    env.depth.set(env.depth.get() - 1);
    ptr::null_mut()
}

unsafe extern "C" fn finalize(env: *mut c_void) {
    unsafe { &*env.cast::<Environment>() }.finalized.set(true);
}

#[test]
fn callback_reentry() {
    let environment = Box::new(Environment {
        nested: Cell::new(ptr::null_mut()),
        memory: Cell::new(ptr::null_mut()),
        depth: Cell::new(0),
        calls: Cell::new(0),
        finalized: Cell::new(false),
    });
    let binary = Vector::from_vec(
        wat::parse_str(
            r#"
        (module
          (import "host" "call" (func $host (param i32) (result i32)))
          (memory (export "memory") 1 2)
          (func (export "run") (param i32) (result i32)
            local.get 0 call $host))
    "#,
        )
        .unwrap(),
    );
    unsafe {
        let engine = wasm_engine_new();
        let store = wasm_store_new(engine);
        let module = wasm_module_new(store, &binary);
        assert!(!module.is_null());
        let mut params = Vector::from_vec(vec![wasm_valtype_new(0)]);
        let mut results = params.clone();
        let ty = wasm_functype_new(&mut params, &mut results);
        let env = (&*environment as *const Environment).cast_mut().cast();
        let host = wasm_func_new_with_env(store, ty, callback, env, Some(finalize));
        wasm_functype_delete(ty);
        let imports = Vector::from_vec(vec![host]);
        let instance = wasm_instance_new(store, module, &imports, ptr::null_mut());
        assert!(!instance.is_null());
        let mut exports = Vector::default();
        wasm_instance_exports(instance, &mut exports);
        environment.memory.set(wasm_extern_as_memory(exports.as_slice()[0]));
        environment.nested.set(wasm_extern_as_func(exports.as_slice()[1]));
        let args = Vector::from_vec(vec![wasm_val_t { kind: 0, of: wasm_val_union { i32: 41 } }]);
        let mut results = Vector::from_vec(vec![wasm_val_t::default()]);
        assert!(wasm_func_call(environment.nested.get(), &args, &mut results).is_null());
        assert_eq!(results.as_slice()[0].of.i32, 42);
        assert_eq!(environment.calls.get(), 3);
        assert_eq!(*wasm_memory_data(environment.memory.get()), 3);
        wasm_instance_delete(instance);
        wasm_module_delete(module);
        drop(exports);
        drop(imports);
        wasm_store_delete(store);
        wasm_engine_delete(engine);
        assert!(environment.finalized.get());
    }
}

#[test]
fn vector_ownership() {
    unsafe {
        let mut bytes = Vector::default();
        wasm_byte_vec_new(&mut bytes, 0, ptr::null());
        assert!(bytes.data.is_null());
        wasm_byte_vec_delete(&mut bytes);
        let mut params = Vector::from_vec(vec![wasm_valtype_new(0), wasm_valtype_new(1)]);
        let mut results = Vector::default();
        let ty = wasm_functype_new(&mut params, &mut results);
        assert!(params.data.is_null());
        let copy = wasm_functype_copy(ty);
        wasm_functype_delete(ty);
        let params = &*wasm_functype_params(copy);
        assert_eq!(params.size, 2);
        assert_eq!(wasm_valtype_kind(params.as_slice()[1]), 1);
        wasm_externtype_delete(wasm_functype_as_externtype(copy));
    }
}

#[test]
fn memory_pointer_lifetime() {
    let binary = Vector::from_vec(
        wat::parse_str(
            r#"
        (module
          (memory (export "memory") 1 2)
          (func (export "write") i32.const 0 i32.const 42 i32.store8))
    "#,
        )
        .unwrap(),
    );
    unsafe {
        let engine = wasm_engine_new();
        let store = wasm_store_new(engine);
        let module = wasm_module_new(store, &binary);
        let imports = Vector::default();
        let instance = wasm_instance_new(store, module, &imports, ptr::null_mut());
        let mut exports = Vector::default();
        wasm_instance_exports(instance, &mut exports);
        let memory = wasm_extern_as_memory(exports.as_slice()[0]);
        let data = wasm_memory_data(memory);
        *data = 7;
        let second = wasm_memory_data(memory);
        assert_eq!(data, second);
        assert_eq!(*data, 7);
        let mut empty = Vector::default();
        let empty_ptr = &raw mut empty;
        assert!(wasm_func_call(wasm_extern_as_func(exports.as_slice()[1]), empty_ptr, empty_ptr).is_null());
        assert_eq!(*data, 42);
        *data = 12;
        assert_eq!(*wasm_memory_data(memory), 12);
        drop(exports);
        wasm_instance_delete(instance);
        wasm_module_delete(module);
        wasm_store_delete(store);
        wasm_engine_delete(engine);
    }
}
