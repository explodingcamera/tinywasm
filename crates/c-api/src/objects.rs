use crate::{
    boxed,
    runtime::{StoreState, wasm_store_t},
    vectors::*,
};
use std::{
    cell::RefCell,
    ffi::c_void,
    ptr,
    rc::{Rc, Weak},
};
use tinywasm::{FuncRef, Function, Global, Memory, Module, ModuleInstance, Table};

/// A finalizer owns exactly one C environment or host-info value.
pub(crate) struct HostInfo {
    pub(crate) data: *mut c_void,
    pub(crate) finalizer: Option<unsafe extern "C" fn(*mut c_void)>,
}
impl Default for HostInfo {
    fn default() -> Self {
        Self { data: ptr::null_mut(), finalizer: None }
    }
}
impl Drop for HostInfo {
    fn drop(&mut self) {
        if let Some(finalizer) = self.finalizer {
            unsafe { finalizer(self.data) };
        }
    }
}

/// Runtime payload shared by all C views of an object.
pub(crate) enum ObjectKind {
    Func(Function, FuncRef),
    Global(Global),
    Table(Table),
    Memory(Memory),
    Module(Module),
    Instance(ModuleInstance),
    Foreign,
    Trap(Vec<u8>),
}
impl ObjectKind {
    pub(crate) fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Func(_, a), Self::Func(_, b)) => a == b,
            (Self::Global(a), Self::Global(b)) => a == b,
            (Self::Memory(a), Self::Memory(b)) => a == b,
            (Self::Table(a), Self::Table(b)) => a == b,
            (Self::Instance(a), Self::Instance(b)) => a.id() == b.id(),
            _ => false,
        }
    }

    pub(crate) fn extern_kind(&self) -> Option<u8> {
        match self {
            Self::Func(..) => Some(0),
            Self::Global(_) => Some(1),
            Self::Table(_) => Some(2),
            Self::Memory(_) => Some(3),
            _ => None,
        }
    }
}

/// Object identity, independent of the allocation of each copied C handle.
pub(crate) struct Object {
    pub(crate) store: Weak<StoreState>,
    pub(crate) kind: ObjectKind,
    host: RefCell<HostInfo>,
}
impl Object {
    pub(crate) fn new(store: Weak<StoreState>, kind: ObjectKind) -> Self {
        Self { store, kind, host: RefCell::new(HostInfo::default()) }
    }
    pub(crate) fn state(&self) -> tinywasm::Result<Rc<StoreState>> {
        self.store.upgrade().ok_or_else(|| tinywasm::Error::Other("store has been deleted".into()))
    }
}

#[derive(Clone)]
pub struct wasm_ref_t(pub(crate) Rc<Object>);
pub type wasm_func_t = wasm_ref_t;
pub type wasm_global_t = wasm_ref_t;
pub type wasm_table_t = wasm_ref_t;
pub type wasm_memory_t = wasm_ref_t;
pub type wasm_extern_t = wasm_ref_t;
pub type wasm_module_t = wasm_ref_t;
pub type wasm_instance_t = wasm_ref_t;
pub type wasm_foreign_t = wasm_ref_t;
pub type wasm_trap_t = wasm_ref_t;

/// Creates a standalone trap handle. Its metadata survives propagation through C callbacks.
pub(crate) fn trap(store: &Rc<StoreState>, message: impl AsRef<[u8]>) -> *mut wasm_trap_t {
    let mut message = message.as_ref().to_vec();
    if message.last() != Some(&0) {
        message.push(0);
    }
    boxed(wasm_ref_t(Rc::new(Object::new(Rc::downgrade(store), ObjectKind::Trap(message)))))
}

macro_rules! reference_api {
    ($copy:ident, $delete:ident, $same:ident, $get:ident, $set:ident, $set_final:ident) => {
        export! { pub unsafe extern "C" fn $copy(value: *const wasm_ref_t) -> *mut wasm_ref_t { unsafe { value.as_ref().map_or(ptr::null_mut(), |value| boxed(value.clone())) } }}
        export! { pub unsafe extern "C" fn $delete(value: *mut wasm_ref_t) { unsafe { crate::delete(value) }; }}
        export! { pub unsafe extern "C" fn $same(a: *const wasm_ref_t, b: *const wasm_ref_t) -> bool {
            if a.is_null() || b.is_null() { return a == b; }
            unsafe { Rc::ptr_eq(&(*a).0, &(*b).0) }
        }}
        export! { pub unsafe extern "C" fn $get(value: *const wasm_ref_t) -> *mut c_void { unsafe { &*value }.0.host.borrow().data }}
        export! { pub unsafe extern "C" fn $set(value: *mut wasm_ref_t, data: *mut c_void) { unsafe { $set_final(value, data, None) }; }}
        export! { pub unsafe extern "C" fn $set_final(value: *mut wasm_ref_t, data: *mut c_void, finalizer: Option<unsafe extern "C" fn(*mut c_void)>) {
            // Release the RefCell borrow before invoking the previous C finalizer.
            let previous = unsafe { &*value }.0.host.replace(HostInfo { data, finalizer });
            drop(previous);
        }}
    };
}
reference_api!(
    wasm_ref_copy,
    wasm_ref_delete,
    wasm_ref_same,
    wasm_ref_get_host_info,
    wasm_ref_set_host_info,
    wasm_ref_set_host_info_with_finalizer
);
reference_api!(
    wasm_func_copy,
    wasm_func_delete,
    wasm_func_same,
    wasm_func_get_host_info,
    wasm_func_set_host_info,
    wasm_func_set_host_info_with_finalizer
);
reference_api!(
    wasm_global_copy,
    wasm_global_delete,
    wasm_global_same,
    wasm_global_get_host_info,
    wasm_global_set_host_info,
    wasm_global_set_host_info_with_finalizer
);
reference_api!(
    wasm_table_copy,
    wasm_table_delete,
    wasm_table_same,
    wasm_table_get_host_info,
    wasm_table_set_host_info,
    wasm_table_set_host_info_with_finalizer
);
reference_api!(
    wasm_memory_copy,
    wasm_memory_delete,
    wasm_memory_same,
    wasm_memory_get_host_info,
    wasm_memory_set_host_info,
    wasm_memory_set_host_info_with_finalizer
);
reference_api!(
    wasm_extern_copy,
    wasm_extern_delete,
    wasm_extern_same,
    wasm_extern_get_host_info,
    wasm_extern_set_host_info,
    wasm_extern_set_host_info_with_finalizer
);
reference_api!(
    wasm_module_copy,
    wasm_module_delete,
    wasm_module_same,
    wasm_module_get_host_info,
    wasm_module_set_host_info,
    wasm_module_set_host_info_with_finalizer
);
reference_api!(
    wasm_instance_copy,
    wasm_instance_delete,
    wasm_instance_same,
    wasm_instance_get_host_info,
    wasm_instance_set_host_info,
    wasm_instance_set_host_info_with_finalizer
);
reference_api!(
    wasm_foreign_copy,
    wasm_foreign_delete,
    wasm_foreign_same,
    wasm_foreign_get_host_info,
    wasm_foreign_set_host_info,
    wasm_foreign_set_host_info_with_finalizer
);
reference_api!(
    wasm_trap_copy,
    wasm_trap_delete,
    wasm_trap_same,
    wasm_trap_get_host_info,
    wasm_trap_set_host_info,
    wasm_trap_set_host_info_with_finalizer
);

macro_rules! reference_casts {
    ($pattern:pat, $up:ident, $down:ident, $up_const:ident, $down_const:ident) => {
        export! { pub unsafe extern "C" fn $up(value: *mut wasm_ref_t) -> *mut wasm_ref_t { value }}
        export! { pub unsafe extern "C" fn $up_const(value: *const wasm_ref_t) -> *const wasm_ref_t { value }}
        export! { pub unsafe extern "C" fn $down(value: *mut wasm_ref_t) -> *mut wasm_ref_t {
            if unsafe { value.as_ref() }.is_some_and(|value| matches!(&value.0.kind, $pattern)) { value } else { ptr::null_mut() }
        }}
        export! { pub unsafe extern "C" fn $down_const(value: *const wasm_ref_t) -> *const wasm_ref_t { unsafe { $down(value.cast_mut()) } }}
    };
}
reference_casts!(
    ObjectKind::Func(..),
    wasm_func_as_ref,
    wasm_ref_as_func,
    wasm_func_as_ref_const,
    wasm_ref_as_func_const
);
reference_casts!(
    ObjectKind::Global(_),
    wasm_global_as_ref,
    wasm_ref_as_global,
    wasm_global_as_ref_const,
    wasm_ref_as_global_const
);
reference_casts!(
    ObjectKind::Table(_),
    wasm_table_as_ref,
    wasm_ref_as_table,
    wasm_table_as_ref_const,
    wasm_ref_as_table_const
);
reference_casts!(
    ObjectKind::Memory(_),
    wasm_memory_as_ref,
    wasm_ref_as_memory,
    wasm_memory_as_ref_const,
    wasm_ref_as_memory_const
);
reference_casts!(
    ObjectKind::Func(..) | ObjectKind::Global(_) | ObjectKind::Table(_) | ObjectKind::Memory(_),
    wasm_extern_as_ref,
    wasm_ref_as_extern,
    wasm_extern_as_ref_const,
    wasm_ref_as_extern_const
);
reference_casts!(
    ObjectKind::Module(_),
    wasm_module_as_ref,
    wasm_ref_as_module,
    wasm_module_as_ref_const,
    wasm_ref_as_module_const
);
reference_casts!(
    ObjectKind::Instance(_),
    wasm_instance_as_ref,
    wasm_ref_as_instance,
    wasm_instance_as_ref_const,
    wasm_ref_as_instance_const
);
reference_casts!(
    ObjectKind::Foreign,
    wasm_foreign_as_ref,
    wasm_ref_as_foreign,
    wasm_foreign_as_ref_const,
    wasm_ref_as_foreign_const
);
reference_casts!(
    ObjectKind::Trap(_),
    wasm_trap_as_ref,
    wasm_ref_as_trap,
    wasm_trap_as_ref_const,
    wasm_ref_as_trap_const
);
reference_casts!(
    ObjectKind::Func(..),
    wasm_func_as_extern,
    wasm_extern_as_func,
    wasm_func_as_extern_const,
    wasm_extern_as_func_const
);
reference_casts!(
    ObjectKind::Global(_),
    wasm_global_as_extern,
    wasm_extern_as_global,
    wasm_global_as_extern_const,
    wasm_extern_as_global_const
);
reference_casts!(
    ObjectKind::Table(_),
    wasm_table_as_extern,
    wasm_extern_as_table,
    wasm_table_as_extern_const,
    wasm_extern_as_table_const
);
reference_casts!(
    ObjectKind::Memory(_),
    wasm_memory_as_extern,
    wasm_extern_as_memory,
    wasm_memory_as_extern_const,
    wasm_extern_as_memory_const
);

export! { pub unsafe extern "C" fn wasm_foreign_new(store: *mut wasm_store_t) -> *mut wasm_foreign_t {
    unsafe { boxed(wasm_ref_t((*store).0.intern(ObjectKind::Foreign))) }
}}
export! { pub unsafe extern "C" fn wasm_trap_new(store: *mut wasm_store_t, message: *const wasm_byte_vec_t) -> *mut wasm_trap_t {
    unsafe { trap(&(*store).0, (*message).as_slice()) }
}}
export! { pub unsafe extern "C" fn wasm_trap_message(value: *const wasm_trap_t, out: *mut wasm_byte_vec_t) {
    let ObjectKind::Trap(message) = &(unsafe { &*value }).0.kind else { panic!("expected trap") };
    unsafe { out.write(Vector::from_vec(message.clone())) };
}}

// TinyWasm currently has no public source-frame capture. The standard permits
// an unavailable origin and an empty trace. Frame accessors are provided for ABI
// completeness, but this implementation does not create frame objects yet.
#[derive(Clone)]
pub struct wasm_frame_t {
    instance: wasm_instance_t,
    index: u32,
    func_offset: usize,
    module_offset: usize,
}
export! { pub unsafe extern "C" fn wasm_trap_origin(_: *const wasm_trap_t) -> *mut wasm_frame_t { ptr::null_mut() }}
export! { pub unsafe extern "C" fn wasm_trap_trace(_: *const wasm_trap_t, out: *mut wasm_frame_vec_t) { unsafe { out.write(Default::default()) }; }}
export! { pub unsafe extern "C" fn wasm_frame_copy(value: *const wasm_frame_t) -> *mut wasm_frame_t { unsafe { boxed((*value).clone()) } }}
export! { pub unsafe extern "C" fn wasm_frame_delete(value: *mut wasm_frame_t) { unsafe { crate::delete(value) }; }}
export! { pub unsafe extern "C" fn wasm_frame_instance(value: *const wasm_frame_t) -> *mut wasm_instance_t { unsafe { ptr::addr_of!((*value).instance).cast_mut() } }}
export! { pub unsafe extern "C" fn wasm_frame_func_index(value: *const wasm_frame_t) -> u32 { unsafe { (*value).index } }}
export! { pub unsafe extern "C" fn wasm_frame_func_offset(value: *const wasm_frame_t) -> usize { unsafe { (*value).func_offset } }}
export! { pub unsafe extern "C" fn wasm_frame_module_offset(value: *const wasm_frame_t) -> usize { unsafe { (*value).module_offset } }}
