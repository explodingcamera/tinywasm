use crate::{boxed, failure, objects::*, runtime::*, types::*, values::*, vectors::*};
use std::{ffi::c_void, ptr, rc::Rc};
use tinywasm::types::FuncType;
use tinywasm::{HostFunction, WasmValue};

type Callback = unsafe extern "C" fn(*const wasm_val_vec_t, *mut wasm_val_vec_t) -> *mut wasm_trap_t;
type CallbackWithEnv =
    unsafe extern "C" fn(*mut c_void, *const wasm_val_vec_t, *mut wasm_val_vec_t) -> *mut wasm_trap_t;

enum CallbackFn {
    Plain(Callback),
    WithEnv(CallbackWithEnv),
}
struct CallbackData {
    function: CallbackFn,
    environment: HostInfo,
}

/// Adapts thread-confined C callback data when TinyWasm's `send` feature is
/// enabled through Cargo feature unification.
///
/// This does not make C API stores thread-safe. Their callbacks and objects
/// must still be accessed and destroyed on the store's creating thread.
struct ThreadConfined<T>(T);

// SAFETY: no safe Rust API exposes C stores or their host callbacks. The C API
// requires a store and all of its objects to remain on their creating thread.
unsafe impl<T> Send for ThreadConfined<T> {}
unsafe impl<T> Sync for ThreadConfined<T> {}

impl<T> ThreadConfined<T> {
    /// Returns the wrapped value on its originating thread.
    ///
    /// # Safety
    /// The caller must be running on the thread that created the C API store.
    unsafe fn get(&self) -> &T {
        &self.0
    }
}

impl CallbackData {
    unsafe fn call(&self, args: *const wasm_val_vec_t, results: *mut wasm_val_vec_t) -> *mut wasm_trap_t {
        unsafe {
            match self.function {
                CallbackFn::Plain(function) => function(args, results),
                CallbackFn::WithEnv(function) => function(self.environment.data, args, results),
            }
        }
    }
}

fn new_function(state: &Rc<StoreState>, ty: &wasm_functype_t, callback: CallbackData) -> *mut wasm_func_t {
    let ty = ty.func();
    let params = ty.params().iter().map(|ty| value_kind(*ty).unwrap()).collect::<Vec<_>>();
    let results = ty.results().iter().map(|ty| value_kind(*ty).unwrap()).collect::<Vec<_>>();
    let callback = ThreadConfined((Rc::downgrade(state), callback));
    let host = HostFunction::from_untyped(&ty, move |mut context, args, out| {
        // SAFETY: invoking a C host callback through its store is restricted to
        // the thread that created that store.
        let (weak, callback) = unsafe { callback.get() };
        let state = weak.upgrade().ok_or_else(|| tinywasm::Error::Other("store has been deleted".into()))?;
        let mut c_args = Vector::from_vec((0..args.len()).map(|_| wasm_val_t::default()).collect());
        for ((out, value), kind) in unsafe { c_args.as_mut_slice() }.iter_mut().zip(args).zip(&params) {
            *out = wasm_val_t::from_runtime(value, *kind, &state, context.store())?;
        }
        let mut c_results = Vector::from_vec(
            results.iter().map(|kind| wasm_val_t { kind: *kind, of: wasm_val_union { i64: 0 } }).collect(),
        );
        let result_data = c_results.data;
        let returned_trap = state.callback(&mut context, || unsafe { callback.call(&c_args, &mut c_results) });
        // The result vector is caller-provided storage, not replaceable ownership.
        assert_eq!(c_results.data, result_data, "callback replaced result storage");
        assert_eq!(c_results.size, results.len(), "callback changed result count");
        if !returned_trap.is_null() {
            let returned_trap = unsafe { Box::from_raw(returned_trap) };
            if !returned_trap.0.store.ptr_eq(&Rc::downgrade(&state)) {
                return Err(tinywasm::Trap::InvalidStore.into());
            }
            *state.trap.borrow_mut() = Some(returned_trap.0);
            return Err(tinywasm::Error::Other("C host callback trapped".into()));
        }
        for ((out, value), expected) in out.iter_mut().zip(unsafe { c_results.as_slice() }).zip(&results) {
            if value.kind != *expected {
                return Err(tinywasm::Error::Other("C callback returned an incorrect value type".into()));
            }
            *out = unsafe { value.to_runtime(&state, context.store_mut()) }?;
        }
        Ok(())
    });
    let result = state.access(|access| {
        let function = host.instantiate(access.store())?;
        let reference = function.as_func_ref(access.store())?;
        Ok(state.intern(ObjectKind::Func(function, reference)))
    });
    result.map_or_else(failure, |object| boxed(wasm_ref_t(object)))
}

fn function_type(object: &Object) -> tinywasm::Result<FuncType> {
    let state = object.state()?;
    let ObjectKind::Func(func, _) = &object.kind else {
        return Err(tinywasm::Error::Other("expected function".into()));
    };
    state.access(|access| Ok(func.ty(access.store())?.clone()))
}

export! { pub unsafe extern "C" fn wasm_func_new(store: *mut wasm_store_t, ty: *const wasm_functype_t, callback: Callback) -> *mut wasm_func_t {
    unsafe { new_function(&(*store).0, &*ty, CallbackData { function: CallbackFn::Plain(callback), environment: HostInfo::default() }) }
}}
export! { pub unsafe extern "C" fn wasm_func_new_with_env(store: *mut wasm_store_t, ty: *const wasm_functype_t, callback: CallbackWithEnv, env: *mut c_void, finalizer: Option<unsafe extern "C" fn(*mut c_void)>) -> *mut wasm_func_t {
    unsafe { new_function(&(*store).0, &*ty, CallbackData { function: CallbackFn::WithEnv(callback), environment: HostInfo { data: env, finalizer } }) }
}}
export! { pub unsafe extern "C" fn wasm_func_type(value: *const wasm_func_t) -> *mut wasm_functype_t {
    function_type(unsafe { &(*value).0 })
        .and_then(|ty| wasm_externtype_t::from_func(&ty).ok_or_else(|| tinywasm::Error::Other("function type is not representable by wasm.h".into())))
        .map_or_else(failure, boxed)
}}
export! { pub unsafe extern "C" fn wasm_func_param_arity(value: *const wasm_func_t) -> usize {
    function_type(unsafe { &(*value).0 }).map(|ty| ty.params().len()).unwrap_or_else(failure)
}}
export! { pub unsafe extern "C" fn wasm_func_result_arity(value: *const wasm_func_t) -> usize {
    function_type(unsafe { &(*value).0 }).map(|ty| ty.results().len()).unwrap_or_else(failure)
}}

export! { pub unsafe extern "C" fn wasm_func_call(value: *const wasm_func_t, args: *const wasm_val_vec_t, results: *mut wasm_val_vec_t) -> *mut wasm_trap_t {
    let object = unsafe { (*value).0.clone() };
    let state = object.state().expect("function store must be alive");
    let result = state.access(|access| {
        let ObjectKind::Func(function, _) = &object.kind else { return Err(tinywasm::Error::Other("expected function".into())); };
        let ty = function.ty(access.store())?.clone();
        let args = unsafe { (*args).as_slice() };
        if args.len() != ty.params().len() || unsafe { (*results).size } != ty.results().len() {
            return Err(tinywasm::Error::Other("function argument or result count mismatch".into()));
        }
        let args = args.iter().zip(ty.params()).map(|(value, ty)| {
            if Some(value.kind) != value_kind(*ty) { return Err(tinywasm::Error::Other("function argument type mismatch".into())); }
            unsafe { value.to_runtime(&state, access.store()) }
        }).collect::<tinywasm::Result<Vec<_>>>()?;
        let kinds = ty.results().iter().map(|ty| value_kind(*ty).ok_or_else(|| tinywasm::Error::Other("result type is not representable by wasm.h".into()))).collect::<tinywasm::Result<Vec<_>>>()?;
        let mut values = vec![WasmValue::I32(0); kinds.len()];
        access.call(function, &args, &mut values)?;
        let mut converted = Vector::from_vec((0..values.len()).map(|_| wasm_val_t::default()).collect());
        for ((out, value), kind) in unsafe { converted.as_mut_slice() }.iter_mut().zip(&values).zip(kinds) {
            *out = wasm_val_t::from_runtime(value, kind, &state, access.store())?;
        }
        // Results may be uninitialized C storage. Transfer values with raw writes,
        // rather than reading or dropping the previous contents.
        for (index, value) in unsafe { converted.as_mut_slice() }.iter_mut().enumerate() {
            unsafe { (*results).data.add(index).write(std::mem::take(value)) };
        }
        Ok(())
    });
    match result {
        Ok(()) => ptr::null_mut(),
        Err(error) => {
            failure::<()>(&error);
            state.trap.borrow_mut().take().map_or_else(|| trap(&state, error.to_string()), |trap| boxed(wasm_ref_t(trap)))
        }
    }
}}
