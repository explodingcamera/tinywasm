use crate::{boxed, failure, objects::*, runtime::*, types::*, values::*};
use std::{ptr, rc::Rc};
use tinywasm::{Global, Memory, Table, WasmValue};

export! { pub unsafe extern "C" fn wasm_extern_kind(value: *const wasm_extern_t) -> u8 { unsafe { &*value }.0.kind.extern_kind().expect("expected extern") }}
export! { pub unsafe extern "C" fn wasm_extern_type(value: *const wasm_extern_t) -> *mut wasm_externtype_t {
    unsafe { match wasm_extern_kind(value) {
        0 => crate::function::wasm_func_type(value), 1 => wasm_global_type(value),
        2 => wasm_table_type(value), 3 => wasm_memory_type(value), _ => unreachable!(),
    } }
}}

macro_rules! object_type {
    ($name:ident, $variant:ident, $convert:ident) => {
        export! { pub unsafe extern "C" fn $name(value: *const wasm_ref_t) -> *mut wasm_externtype_t {
            let object = unsafe { &(*value).0 };
            with_object(object, |_, access| {
                let ObjectKind::$variant(item) = &object.kind else { return Err(tinywasm::Error::Other("incorrect object kind".into())); };
                wasm_externtype_t::$convert(item.ty(access.store())?)
                    .map(boxed).ok_or_else(|| tinywasm::Error::Other("type is not representable by wasm.h".into()))
            })
        }}
    };
}
object_type!(wasm_memory_type, Memory, from_memory);
object_type!(wasm_table_type, Table, from_table);
object_type!(wasm_global_type, Global, from_global);

export! { pub unsafe extern "C" fn wasm_memory_new(store: *mut wasm_store_t, ty: *const wasm_memorytype_t) -> *mut wasm_memory_t {
    let state = unsafe { &(*store).0 };
    state.access(|access| Ok(state.intern(ObjectKind::Memory(Memory::try_new(access.store(), unsafe { (*ty).memory() })?))))
        .map_or_else(failure, |object| boxed(wasm_ref_t(object)))
}}
export! { pub unsafe extern "C" fn wasm_global_new(store: *mut wasm_store_t, ty: *const wasm_globaltype_t, value: *const wasm_val_t) -> *mut wasm_global_t {
    let state = unsafe { &(*store).0 };
    state.access(|access| {
        let value = unsafe { (*value).to_runtime(state, access.store()) }?;
        Ok(state.intern(ObjectKind::Global(Global::try_new(access.store(), unsafe { (*ty).global() }, value)?)))
    }).map_or_else(failure, |object| boxed(wasm_ref_t(object)))
}}
export! { pub unsafe extern "C" fn wasm_table_new(store: *mut wasm_store_t, ty: *const wasm_tabletype_t, init: *mut wasm_ref_t) -> *mut wasm_table_t {
    let state = unsafe { &(*store).0 };
    let ty = unsafe { (*ty).table() };
    state.access(|access| {
        let kind = value_kind(tinywasm::types::WasmType::Ref(ty.element_type)).unwrap();
        let init = unsafe { reference_to_runtime(init, kind, state, access.store()) }?;
        Ok(state.intern(ObjectKind::Table(Table::try_new(access.store(), ty, init.into())?)))
    }).map_or_else(failure, |object| boxed(wasm_ref_t(object)))
}}

fn with_object<T: Default>(
    object: &Object,
    action: impl FnOnce(&Rc<StoreState>, &mut Access<'_>) -> tinywasm::Result<T>,
) -> T {
    let result = object.state().and_then(|state| state.access(|access| action(&state, access)));
    result.unwrap_or_else(failure)
}

export! { pub unsafe extern "C" fn wasm_memory_data(value: *mut wasm_memory_t) -> *mut u8 {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Memory(memory) = &object.kind else { return ptr::null_mut(); };
    with_object(object, |_, access| memory.data_ptr(access.store()))
}}
export! { pub unsafe extern "C" fn wasm_memory_data_size(value: *const wasm_memory_t) -> usize {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Memory(memory) = &object.kind else { return 0; };
    with_object(object, |_, access| memory.len(access.store()))
}}
export! { pub unsafe extern "C" fn wasm_memory_size(value: *const wasm_memory_t) -> u32 {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Memory(memory) = &object.kind else { return 0; };
    with_object(object, |_, access| Ok(memory.page_count(access.store())? as u32))
}}
export! { pub unsafe extern "C" fn wasm_memory_grow(value: *mut wasm_memory_t, delta: u32) -> bool {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Memory(memory) = &object.kind else { return false; };
    with_object(object, |_, access| Ok(memory.grow(access.store(), delta as i64)?.is_some()))
}}
export! { pub unsafe extern "C" fn wasm_global_get(value: *const wasm_global_t, out: *mut wasm_val_t) {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Global(global) = &object.kind else { return; };
    let result = with_object(object, |state, access| {
        let kind = value_kind(global.ty(access.store())?.ty).ok_or_else(|| tinywasm::Error::Other("unsupported global type".into()))?;
        let value = global.get(access.store())?;
        wasm_val_t::from_runtime(&value, kind, state, access.store())
    });
    unsafe { out.write(result) };
}}
export! { pub unsafe extern "C" fn wasm_global_set(value: *mut wasm_global_t, new_value: *const wasm_val_t) {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Global(global) = &object.kind else { return; };
    with_object(object, |state, access| {
        let value = unsafe { (*new_value).to_runtime(state, access.store()) }?;
        global.set(access.store(), value)
    });
}}
export! { pub unsafe extern "C" fn wasm_table_size(value: *const wasm_table_t) -> u32 {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Table(table) = &object.kind else { return 0; };
    with_object(object, |_, access| Ok(table.size(access.store())? as u32))
}}
export! { pub unsafe extern "C" fn wasm_table_get(value: *const wasm_table_t, index: u32) -> *mut wasm_ref_t {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Table(table) = &object.kind else { return ptr::null_mut(); };
    with_object(object, |state, access| {
        let kind = value_kind(tinywasm::types::WasmType::Ref(table.ty(access.store())?.element_type)).unwrap();
        let value = table.get(access.store(), index)?;
        let result = wasm_val_t::from_runtime(&value, kind, state, access.store())?;
        Ok(unsafe { result.of.reference })
    })
}}
export! { pub unsafe extern "C" fn wasm_table_set(value: *mut wasm_table_t, index: u32, element: *mut wasm_ref_t) -> bool {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Table(table) = &object.kind else { return false; };
    with_object(object, |state, access| {
        let kind = value_kind(tinywasm::types::WasmType::Ref(table.ty(access.store())?.element_type)).unwrap();
        let value = unsafe { reference_to_runtime(element, kind, state, access.store()) }?;
        table.set(access.store(), index, WasmValue::Ref(value))?;
        Ok(true)
    })
}}
export! { pub unsafe extern "C" fn wasm_table_grow(value: *mut wasm_table_t, delta: u32, init: *mut wasm_ref_t) -> bool {
    let object = unsafe { &(*value).0 };
    let ObjectKind::Table(table) = &object.kind else { return false; };
    with_object(object, |state, access| {
        let kind = value_kind(tinywasm::types::WasmType::Ref(table.ty(access.store())?.element_type)).unwrap();
        let init = unsafe { reference_to_runtime(init, kind, state, access.store()) }?;
        Ok(table.grow(access.store(), delta as i32, init.into())?.is_some())
    })
}}
