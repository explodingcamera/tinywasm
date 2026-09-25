use crate::{boxed, failure, objects::*, runtime::*, types::*, vectors::*};
use std::{ptr, rc::Rc};
use tinywasm::{
    Extern, ExternItem, Module, ModuleInstance,
    types::{ExportType, ImportType},
};

fn import_type(ty: ImportType<'_>) -> Option<wasm_externtype_t> {
    match ty {
        ImportType::Func(ty) => wasm_externtype_t::from_func(ty),
        ImportType::Global(ty) => wasm_externtype_t::from_global(*ty),
        ImportType::Table(ty) => wasm_externtype_t::from_table(*ty),
        ImportType::Memory(ty) => wasm_externtype_t::from_memory(*ty),
        ImportType::Tag(_) => None,
    }
}

fn export_type(ty: ExportType<'_>) -> Option<wasm_externtype_t> {
    match ty {
        ExportType::Func(ty) => wasm_externtype_t::from_func(ty),
        ExportType::Global(ty) => wasm_externtype_t::from_global(*ty),
        ExportType::Table(ty) => wasm_externtype_t::from_table(*ty),
        ExportType::Memory(ty) => wasm_externtype_t::from_memory(*ty),
        ExportType::Tag(_) => None,
    }
}

fn check_interface(module: Module) -> tinywasm::Result<Module> {
    if module.imports().all(|item| import_type(item.ty).is_some())
        && module.exports().all(|item| export_type(item.ty).is_some())
    {
        Ok(module)
    } else {
        Err(tinywasm::Error::Other("module interface contains types not representable by wasm.h".into()))
    }
}

fn module_handle(state: &Rc<StoreState>, module: Module) -> *mut wasm_module_t {
    boxed(wasm_ref_t(Rc::new(Object::new(Rc::downgrade(state), ObjectKind::Module(module)))))
}

export! { pub unsafe extern "C" fn wasm_module_new(store: *mut wasm_store_t, binary: *const wasm_byte_vec_t) -> *mut wasm_module_t {
    match tinywasm::parse_bytes(unsafe { (*binary).as_slice() }).map_err(tinywasm::Error::from).and_then(check_interface) {
        Ok(module) => module_handle(unsafe { &(*store).0 }, module), Err(error) => failure(error),
    }
}}
export! { pub unsafe extern "C" fn wasm_module_validate(_: *mut wasm_store_t, binary: *const wasm_byte_vec_t) -> bool {
    match tinywasm::parse_bytes(unsafe { (*binary).as_slice() }).map_err(tinywasm::Error::from).and_then(check_interface) {
        Ok(_) => true, Err(error) => failure(error),
    }
}}
export! { pub unsafe extern "C" fn wasm_module_imports(value: *const wasm_module_t, out: *mut wasm_importtype_vec_t) {
    let ObjectKind::Module(module) = &(unsafe { &*value }).0.kind else { panic!("expected module") };
    let values = module.imports().map(|item| boxed(wasm_importtype_t {
        module: Vector::from_vec(item.module.as_bytes().to_vec()), name: Vector::from_vec(item.name.as_bytes().to_vec()),
        ty: Box::new(import_type(item.ty).expect("checked module interface")),
    })).collect();
    unsafe { out.write(Vector::from_vec(values)) };
}}
export! { pub unsafe extern "C" fn wasm_module_exports(value: *const wasm_module_t, out: *mut wasm_exporttype_vec_t) {
    let ObjectKind::Module(module) = &(unsafe { &*value }).0.kind else { panic!("expected module") };
    let values = module.exports().map(|item| boxed(wasm_exporttype_t {
        name: Vector::from_vec(item.name.as_bytes().to_vec()), ty: Box::new(export_type(item.ty).expect("checked module interface")),
    })).collect();
    unsafe { out.write(Vector::from_vec(values)) };
}}

pub struct wasm_shared_module_t(Module);
export! { pub unsafe extern "C" fn wasm_module_share(value: *const wasm_module_t) -> *mut wasm_shared_module_t {
    let ObjectKind::Module(module) = &(unsafe { &*value }).0.kind else { panic!("expected module") };
    boxed(wasm_shared_module_t(module.clone()))
}}
export! { pub unsafe extern "C" fn wasm_module_obtain(store: *mut wasm_store_t, value: *const wasm_shared_module_t) -> *mut wasm_module_t { unsafe { module_handle(&(*store).0, (*value).0.clone()) } }}
export! { pub unsafe extern "C" fn wasm_shared_module_delete(value: *mut wasm_shared_module_t) { unsafe { crate::delete(value) }; }}
export! { pub unsafe extern "C" fn wasm_module_serialize(value: *const wasm_module_t, out: *mut wasm_byte_vec_t) {
    let ObjectKind::Module(module) = &(unsafe { &*value }).0.kind else { panic!("expected module") };
    let bytes = module.serialize_twasm().unwrap_or_else(failure);
    unsafe { out.write(Vector::from_vec(bytes)) };
}}
export! { pub unsafe extern "C" fn wasm_module_deserialize(store: *mut wasm_store_t, bytes: *const wasm_byte_vec_t) -> *mut wasm_module_t {
    // As with the Rust archive API, these bytes must come from a trusted serializer.
    match Module::try_from_twasm(unsafe { (*bytes).as_slice() }).map_err(tinywasm::Error::from).and_then(check_interface) {
        Ok(module) => module_handle(unsafe { &(*store).0 }, module), Err(error) => failure(error),
    }
}}

export! { pub unsafe extern "C" fn wasm_instance_new(store: *mut wasm_store_t, module: *const wasm_module_t, imports: *const wasm_extern_vec_t, trap_out: *mut *mut wasm_trap_t) -> *mut wasm_instance_t {
    if !trap_out.is_null() { unsafe { trap_out.write(ptr::null_mut()) }; }
    let state = unsafe { &(*store).0 };
    let object = unsafe { &(*module).0 };
    let result = state.access(|access| {
        if access.is_callback() { return Err(tinywasm::Error::Other("instantiation during a callback is not supported".into())); }
        if !object.store.ptr_eq(&Rc::downgrade(state)) { return Err(tinywasm::Trap::InvalidStore.into()); }
        let ObjectKind::Module(module) = &object.kind else { return Err(tinywasm::Error::Other("expected module".into())); };
        let imports = unsafe { (*imports).as_slice() }.iter().map(|value| {
            let object = unsafe { &(**value).0 };
            if !object.store.ptr_eq(&Rc::downgrade(state)) { return Err(tinywasm::Trap::InvalidStore.into()); }
            Ok(match &object.kind {
                ObjectKind::Func(func, _) => Extern::Function(func.clone()), ObjectKind::Global(global) => Extern::Global(*global),
                ObjectKind::Memory(memory) => Extern::Memory(*memory), ObjectKind::Table(table) => Extern::Table(*table),
                _ => return Err(tinywasm::Error::Other("expected external import".into())),
            })
        }).collect::<tinywasm::Result<Vec<_>>>()?;
        ModuleInstance::instantiate_ordered(access.store(), module, &imports)
    });
    match result {
        Ok(instance) => boxed(wasm_ref_t(state.intern(ObjectKind::Instance(instance)))),
        Err(error) => {
            let pending = state.trap.borrow_mut().take();
            if !trap_out.is_null() {
                let value = pending.map_or_else(|| trap(state, error.to_string()), |trap| boxed(wasm_ref_t(trap)));
                unsafe { trap_out.write(value) };
            }
            failure(error)
        }
    }
}}

export! { pub unsafe extern "C" fn wasm_instance_exports(value: *const wasm_instance_t, out: *mut wasm_extern_vec_t) {
    let object = unsafe { &(*value).0 };
    let result = (|| {
        let state = object.state()?;
        let ObjectKind::Instance(instance) = &object.kind else { return Err(tinywasm::Error::Other("expected instance".into())); };
        state.access(|access| {
            let kinds = instance.exports().map(|(_, item)| {
                Ok(match item {
                    ExternItem::Func(func) => { let reference = func.as_func_ref(access.store())?; ObjectKind::Func(func, reference) },
                    ExternItem::Global(global) => ObjectKind::Global(global), ExternItem::Memory(memory) => ObjectKind::Memory(memory),
                    ExternItem::Table(table) => ObjectKind::Table(table), ExternItem::Tag(_) => return Err(tinywasm::Error::Other("tag exports are unsupported".into())),
                    ExternItem::MemoryShared(_) => return Err(tinywasm::Error::Other("shared memory exports are unsupported".into())),
                })
            }).collect::<tinywasm::Result<Vec<_>>>()?;
            Ok(Vector::from_vec(kinds.into_iter().map(|kind| boxed(wasm_ref_t(state.intern(kind)))).collect()))
        })
    })();
    unsafe { out.write(result.unwrap_or_else(failure)) };
}}
