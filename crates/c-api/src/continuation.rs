//! Experimental numeric continuation adapter. All state is opt-in and thread-confined.
use crate::{function, objects::*, runtime::*, values::*, vectors::*};
use std::{
    ffi::c_void,
    ptr,
    rc::{Rc, Weak},
};
use tinywasm::{HostExecProgress, HostExecution, WasmValue};
type Callback =
    unsafe extern "C" fn(*mut c_void, *const wasm_val_vec_t, *mut wasm_val_vec_t, *mut *mut wasm_trap_t) -> i32;
type Cancel = Option<unsafe extern "C" fn(*mut c_void)>;
pub(crate) struct Host {
    callback: Callback,
    env: *mut c_void,
    cancel: Cancel,
    state: Weak<StoreState>,
}
pub struct wasm_ext_call_t {
    // Drop the borrowed execution before its stable backing buffers and owning store.
    execution: Option<HostExecution<'static>>,
    values: Box<[WasmValue]>,
    result_count: usize,
    state: Rc<StoreState>,
    pending: Option<Rc<Host>>,
    status: i32,
}
fn numeric(ty: &tinywasm::types::FuncType) -> bool {
    ty.results().len() <= 1
        && ty.params().iter().chain(ty.results()).all(|x| {
            matches!(
                x,
                tinywasm::types::WasmType::I32
                    | tinywasm::types::WasmType::I64
                    | tinywasm::types::WasmType::F32
                    | tinywasm::types::WasmType::F64
            )
        })
}
unsafe extern "C" fn release_host(env: *mut c_void) {
    unsafe {
        drop(Rc::from_raw(env.cast::<Host>()));
    }
}
unsafe extern "C" fn sync_host(env: *mut c_void, _: *const wasm_val_vec_t, _: *mut wasm_val_vec_t) -> *mut wasm_trap_t {
    let host = unsafe { &*env.cast::<Host>() };
    host.state.upgrade().map_or(ptr::null_mut(), |state| trap(&state, "async host requires an extension invocation"))
}

export! { pub unsafe extern "C" fn wasm_ext_func_new(store: *mut wasm_store_t, ty: *const crate::types::wasm_functype_t, cb: Option<Callback>, env: *mut c_void, cancel: Cancel) -> *mut wasm_func_t {
    if store.is_null() || ty.is_null() || cb.is_none() { return ptr::null_mut(); }
    let state = unsafe { &(*store).0 };
    if state.ext_reserved.get() || !numeric(&unsafe { &*ty }.func()) { return ptr::null_mut(); }
    let host = Rc::new(Host { callback: cb.unwrap(), env, cancel, state: Rc::downgrade(state) });
    let raw = Rc::into_raw(host.clone()).cast_mut().cast();
    let func = unsafe { function::wasm_func_new_with_env(store, ty, sync_host, raw, Some(release_host)) };
    if func.is_null() { return func; }
    let func_ref = unsafe { &*func };
    let ObjectKind::Func(_, reference) = &func_ref.0.kind else { unreachable!() };
    state.ext_hosts.borrow_mut().insert(*reference, host);
    func
}}
export! { pub unsafe extern "C" fn wasm_ext_call_new(func: *const wasm_func_t, args: *const wasm_val_vec_t) -> *mut wasm_ext_call_t {
    if func.is_null() || args.is_null() { return ptr::null_mut(); }
    if unsafe { (*args).size != 0 && (*args).data.is_null() } { return ptr::null_mut(); }
    let object = unsafe { &(*func).0 };
    let Ok(state) = object.state() else { return ptr::null_mut(); };
    if !state.ext_available() { return ptr::null_mut(); }
    let ObjectKind::Func(function, _) = &object.kind else { return ptr::null_mut(); };
    let prepared = state.access(|access| {
        let ty = function.ty(access.store())?;
        if !numeric(ty) { return Err(tinywasm::Error::Other("unsupported continuation signature".into())); }
        let count = ty.results().len();
        if unsafe { (*args).as_slice() }.len()!=ty.params().len() || !unsafe { (*args).as_slice() }.iter().zip(ty.params()).all(|(v,t)|Some(v.kind)==crate::types::value_kind(*t)) { return Err(tinywasm::Error::Other("invalid arguments".into())); }
        let args = unsafe { (*args).as_slice() }.iter().map(|v| unsafe { v.to_runtime(&state, access.store()) }).collect::<tinywasm::Result<Vec<_>>>()?;
        Ok((args, count))
    });
    let Ok((args, count)) = prepared else { return ptr::null_mut(); };
    let mut call = Box::new(wasm_ext_call_t { execution: None, values: vec![WasmValue::I32(0);count].into_boxed_slice(), result_count: count, state, pending: None, status: 1 });
    call.state.ext_reserved.set(true);
    // SAFETY: reservation excludes all idle C access; Rc pins the Store. Box pins results.
    // The execution is dropped before either backing object and before releasing reservation.
    let store: &'static mut tinywasm::Store = unsafe { &mut *call.state.store.get() };
    let values: &'static mut [WasmValue] = unsafe { &mut *(&mut *call.values as *mut [WasmValue]) };
    let execution = function.call_resumable_with_host_calls(store, &args, values);
    match execution {
        Ok(execution) => { call.execution = Some(execution); Box::into_raw(call) }
        Err(_) => { call.state.ext_reserved.set(false); ptr::null_mut() }
    }
}}
impl wasm_ext_call_t {
    fn finish(&mut self, status: i32) {
        self.execution.take();
        self.state.ext_reserved.set(false);
        self.status = status;
    }
    fn cancel(&mut self) {
        if self.execution.is_none() {
            return;
        }
        self.status = 2;
        if let Some(host) = self.pending.take()
            && let Some(cancel) = host.cancel
        {
            unsafe {
                cancel(host.env);
            }
        }
        self.finish(2);
    }
}
impl Drop for wasm_ext_call_t {
    fn drop(&mut self) {
        self.cancel();
    }
}
export! { pub unsafe extern "C" fn wasm_ext_call_cancel(call: *mut wasm_ext_call_t) { if let Some(call) = unsafe { call.as_mut() } { call.cancel(); } }}
export! { pub unsafe extern "C" fn wasm_ext_call_delete(call: *mut wasm_ext_call_t) { unsafe { crate::delete(call); } }}
export! { pub unsafe extern "C" fn wasm_ext_call_resume(call: *mut wasm_ext_call_t, completion: *const wasm_val_vec_t, results: *mut wasm_val_vec_t, out_trap: *mut *mut wasm_trap_t) -> i32 {
    if out_trap.is_null() { return -1; }
    unsafe { *out_trap = ptr::null_mut(); }
    let Some(call) = (unsafe { call.as_mut() }) else { return -1; };
    if call.execution.is_none() { return if call.status == 2 { 2 } else { -1 }; }
    if results.is_null() || unsafe { (*results).size } != call.result_count || unsafe { (*results).size != 0 && (*results).data.is_null() } { return -1; }
    if call.pending.is_some() {
        if completion.is_null() { return 1; }
        if unsafe { (*completion).size != 0 && (*completion).data.is_null() } { return -1; }
        let mut host_call = call.execution.as_mut().unwrap().host_call().unwrap();
        let state = call.state.clone();
        let ty=host_call.function().ty(host_call.store()).unwrap();
        if unsafe { (*completion).as_slice() }.len()!=ty.results().len() || !unsafe { (*completion).as_slice() }.iter().zip(ty.results()).all(|(v,t)|Some(v.kind)==crate::types::value_kind(*t)) { return -1; }
        let input = host_call.with_context(|mut ctx| unsafe { (*completion).as_slice() }.iter().map(|v| unsafe { v.to_runtime(&state, ctx.store_mut()) }).collect::<tinywasm::Result<Vec<_>>>());
        let Ok(input) = input else { return -1; };
        if host_call.complete(&input).is_err() { return -1; }
        call.pending = None;
    } else if !completion.is_null() { return -1; }
    let state = call.state.clone();
    let mut last = None;
    loop {
        let progress = call.execution.as_mut().unwrap().resume_with_fuel_selected(u32::MAX, |reference| {
            if let Some((previous, selected)) = last && previous == reference { return selected; }
            let selected = state.ext_hosts.borrow().contains_key(&reference);
            last = Some((reference, selected));
            selected
        });
        match progress {
            Ok(HostExecProgress::Suspended) => continue,
            Ok(HostExecProgress::Completed) => {
                call.execution.take(); // End the exclusive result-buffer borrow before reading it.
                // Numeric v1 results need no store lookup or reference allocation.
                for (i,v) in call.values.iter().enumerate() {
                    let value = match v { WasmValue::I32(x)=>wasm_val_t{kind:0,of:wasm_val_union{i32:*x}}, WasmValue::I64(x)=>wasm_val_t{kind:1,of:wasm_val_union{i64:*x}}, WasmValue::F32(x)=>wasm_val_t{kind:2,of:wasm_val_union{f32:*x}}, WasmValue::F64(x)=>wasm_val_t{kind:3,of:wasm_val_union{f64:*x}}, _=>unreachable!() };
                    unsafe { (*results).data.add(i).write(value); }
                }
                call.finish(0); return 0;
            }
            Err(error) => { unsafe { *out_trap = call.state.trap.borrow_mut().take().map_or_else(|| trap(&call.state,error.to_string()), |object| crate::boxed(wasm_ref_t(object))); } call.finish(-1); return -1; }
            Ok(HostExecProgress::HostCall) => {
                let mut request = call.execution.as_mut().unwrap().host_call().unwrap();
                let reference = request.function().as_func_ref(request.store()).unwrap();
                let host = call.state.ext_hosts.borrow().get(&reference).cloned();
                let Some(host) = host else { if let Err(error) = request.invoke() { unsafe { *out_trap = call.state.trap.borrow_mut().take().map_or_else(|| trap(&call.state,error.to_string()), |object| crate::boxed(wasm_ref_t(object))); } call.finish(-1); return -1; } continue; };
                let ty = request.function().ty(request.store()).unwrap().clone();
                let state=call.state.clone();
                let args=Vector::from_vec(request.params().iter().zip(ty.params()).map(|(v,t)| wasm_val_t::from_runtime(v,crate::types::value_kind(*t).unwrap(),&state,request.store()).unwrap()).collect());
                let mut result_values=ty.results().iter().map(|t|wasm_val_t{kind:crate::types::value_kind(*t).unwrap(),of:wasm_val_union{i64:0}}).collect::<Vec<_>>();
                // Numeric-only borrowed output: the backing Vec frees storage without treating
                // a malformed callback's union bits as an owned reference.
                let mut out=std::mem::ManuallyDrop::new(Vector { size: result_values.len(), data: result_values.as_mut_ptr() });
                let data=out.data; let count=out.size; let mut trapped=ptr::null_mut();
                let status=request.with_context(|mut ctx| Ok(state.callback(&mut ctx,||unsafe{(host.callback)(host.env,&args,&mut *out,&mut trapped)}))).unwrap();
                if out.data!=data || out.size!=count { std::process::abort(); }
                // Callback owns neither vector; modifications to ownership are C contract violations.

                if status==1 && trapped.is_null() { call.pending=Some(host); return 1; }
                if status!=0 || !trapped.is_null() { unsafe{*out_trap=trapped;} call.finish(-1); return -1; }
                if !unsafe { out.as_slice() }.iter().zip(ty.results()).all(|(v,t)|Some(v.kind)==crate::types::value_kind(*t)) {
                    unsafe { *out_trap = trap(&call.state,"host result type mismatch"); }
                    call.finish(-1); return -1;
                }
                let values=request.with_context(|mut ctx|unsafe{out.as_slice()}.iter().map(|v|unsafe{v.to_runtime(&state,ctx.store_mut())}).collect::<tinywasm::Result<Vec<_>>>());
                if values.and_then(|v|request.complete(&v)).is_err() { call.finish(-1); return -1; }
            }
        }
    }
}}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;
    use std::cell::Cell;
    struct Counts {
        calls: Cell<u32>,
        cancels: Cell<u32>,
    }
    unsafe extern "C" fn pending(
        env: *mut c_void,
        _: *const wasm_val_vec_t,
        _: *mut wasm_val_vec_t,
        _: *mut *mut wasm_trap_t,
    ) -> i32 {
        let counts = unsafe { &*env.cast::<Counts>() };
        counts.calls.set(counts.calls.get() + 1);
        1
    }
    unsafe extern "C" fn cancel(env: *mut c_void) {
        let counts = unsafe { &*env.cast::<Counts>() };
        counts.cancels.set(counts.cancels.get() + 1);
    }
    #[test]
    fn owned_execution_releases_borrows_before_results_and_cancel() {
        let counts = Counts { calls: Cell::new(0), cancels: Cell::new(0) };
        unsafe {
            let engine = wasm_engine_new();
            let store = wasm_store_new(engine);
            let mut params = Vector::from_vec(vec![wasm_valtype_new(0)]);
            let mut results = params.clone();
            let ty = wasm_functype_new(&mut params, &mut results);
            let host =
                wasm_ext_func_new(store, ty, Some(pending), (&counts as *const Counts).cast_mut().cast(), Some(cancel));
            assert!(!host.is_null());
            wasm_functype_delete(ty);
            let args = Vector::from_vec(vec![wasm_val_t { kind: 0, of: wasm_val_union { i32: 20 } }]);
            let mut out = Vector::from_vec(vec![wasm_val_t::default()]);
            let completion = Vector::from_vec(vec![wasm_val_t { kind: 0, of: wasm_val_union { i32: 42 } }]);
            let mut trap = ptr::null_mut();
            for _ in 0..3 {
                let call = wasm_ext_call_new(host, &args);
                assert!(!call.is_null());
                assert_eq!(wasm_ext_call_resume(call, ptr::null(), &mut out, &mut trap), 1);
                assert_eq!(wasm_ext_call_resume(call, &completion, &mut out, &mut trap), 0);
                assert_eq!(out.as_slice()[0].of.i32, 42);
                assert!(trap.is_null());
                wasm_ext_call_delete(call);
            }
            let call = wasm_ext_call_new(host, &args);
            assert_eq!(wasm_ext_call_resume(call, ptr::null(), &mut out, &mut trap), 1);
            wasm_ext_call_cancel(call);
            wasm_ext_call_cancel(call);
            wasm_ext_call_delete(call);
            assert_eq!(counts.calls.get(), 4);
            assert_eq!(counts.cancels.get(), 1);
            let binary = Vector::from_vec(include_bytes!("../tests/continuations/fixture.wasm").to_vec());
            let module = crate::module::wasm_module_new(store, &binary);
            assert!(!module.is_null());
            unsafe extern "C" fn increment(args: *const wasm_val_vec_t, out: *mut wasm_val_vec_t) -> *mut wasm_trap_t {
                unsafe {
                    (*out).data.write(wasm_val_t { kind: 0, of: wasm_val_union { i32: (*(*args).data).of.i32 + 1 } });
                }
                ptr::null_mut()
            }
            let mut p = Vector::from_vec(vec![wasm_valtype_new(0)]);
            let mut r = p.clone();
            let ty = wasm_functype_new(&mut p, &mut r);
            let inc = crate::function::wasm_func_new(store, ty, increment);
            wasm_functype_delete(ty);
            let imports = Vector::from_vec(vec![crate::objects::wasm_func_copy(host), inc]);
            let instance = crate::module::wasm_instance_new(store, module, &imports, &mut trap);
            assert!(!instance.is_null());
            let mut exports = Vector::default();
            crate::module::wasm_instance_exports(instance, &mut exports);
            // fixture.wat declares indirect, twice, trap, then run.
            let function = crate::objects::wasm_extern_as_func(exports.as_slice()[3]);
            let call = wasm_ext_call_new(function, &args);
            assert!(!call.is_null());
            assert_eq!(wasm_ext_call_resume(call, ptr::null(), &mut out, &mut trap), 1);
            assert_eq!(wasm_ext_call_resume(call, &completion, &mut out, &mut trap), 0);
            assert_eq!(out.as_slice()[0].of.i32, 85);
            wasm_ext_call_delete(call);
            let three = Vector::from_vec(vec![wasm_val_t { kind: 0, of: wasm_val_union { i32: 3 } }]);
            let function = crate::objects::wasm_extern_as_func(exports.as_slice()[5]);
            let call = wasm_ext_call_new(function, &three);
            assert_eq!(wasm_ext_call_resume(call, ptr::null(), &mut out, &mut trap), 0);
            assert_eq!(out.as_slice()[0].of.i32, 9);
            wasm_ext_call_delete(call);
            crate::objects::wasm_instance_delete(instance);
            crate::objects::wasm_module_delete(module);
            drop(exports);
            drop(imports);
            wasm_func_delete(host);
            wasm_store_delete(store);
            wasm_engine_delete(engine);
        }
    }
}
