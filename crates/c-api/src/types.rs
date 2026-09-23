use crate::{boxed, failure, vectors::*};
use std::ptr;
use tinywasm::types::{FuncType, GlobalType, MemoryArch, MemoryType, RefType, TableType, WasmType};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct wasm_limits_t {
    pub min: u32,
    pub max: u32,
}

#[derive(Clone)]
pub struct wasm_valtype_t(pub(crate) WasmType);

/// All extern type views use the same allocation, so casts remain borrowed views.
#[derive(Clone)]
pub enum wasm_externtype_t {
    Func { params: wasm_valtype_vec_t, results: wasm_valtype_vec_t },
    Global { content: Box<wasm_valtype_t>, mutable: bool },
    Table { element: Box<wasm_valtype_t>, limits: wasm_limits_t },
    Memory(wasm_limits_t),
    Tag(Box<wasm_externtype_t>),
}

pub type wasm_functype_t = wasm_externtype_t;
pub type wasm_globaltype_t = wasm_externtype_t;
pub type wasm_tabletype_t = wasm_externtype_t;
pub type wasm_memorytype_t = wasm_externtype_t;
pub type wasm_tagtype_t = wasm_externtype_t;

#[derive(Clone)]
pub struct wasm_importtype_t {
    pub(crate) module: wasm_byte_vec_t,
    pub(crate) name: wasm_byte_vec_t,
    pub(crate) ty: Box<wasm_externtype_t>,
}

#[derive(Clone)]
pub struct wasm_exporttype_t {
    pub(crate) name: wasm_byte_vec_t,
    pub(crate) ty: Box<wasm_externtype_t>,
}

/// Converts a C value kind, rejecting kinds outside the pinned header.
pub(crate) fn value_type(kind: u8) -> Option<WasmType> {
    Some(match kind {
        0 => WasmType::I32,
        1 => WasmType::I64,
        2 => WasmType::F32,
        3 => WasmType::F64,
        128 => WasmType::Ref(RefType::EXTERNREF),
        129 => WasmType::Ref(RefType::FUNCREF),
        _ => return None,
    })
}

/// Converts only types that the standard header can represent exactly.
pub(crate) fn value_kind(ty: WasmType) -> Option<u8> {
    Some(match ty {
        WasmType::I32 => 0,
        WasmType::I64 => 1,
        WasmType::F32 => 2,
        WasmType::F64 => 3,
        WasmType::Ref(ty) if ty == RefType::EXTERNREF => 128,
        WasmType::Ref(ty) if ty == RefType::FUNCREF => 129,
        _ => return None,
    })
}

impl wasm_externtype_t {
    pub(crate) fn kind(&self) -> u8 {
        match self {
            Self::Func { .. } => 0,
            Self::Global { .. } => 1,
            Self::Table { .. } => 2,
            Self::Memory(_) => 3,
            Self::Tag(_) => 4,
        }
    }

    pub(crate) fn from_func(ty: &FuncType) -> Option<Self> {
        if !ty.params().iter().chain(ty.results()).all(|ty| value_kind(*ty).is_some()) {
            return None;
        }
        let values = |types: &[WasmType]| Vector::from_vec(types.iter().map(|ty| boxed(wasm_valtype_t(*ty))).collect());
        Some(Self::Func { params: values(ty.params()), results: values(ty.results()) })
    }

    pub(crate) fn from_global(ty: GlobalType) -> Option<Self> {
        value_kind(ty.ty)?;
        Some(Self::Global { content: Box::new(wasm_valtype_t(ty.ty)), mutable: ty.mutable })
    }

    pub(crate) fn from_memory(ty: MemoryType) -> Option<Self> {
        if ty.arch() != MemoryArch::I32 || ty.page_size() != 65536 {
            return None;
        }
        Some(Self::Memory(wasm_limits_t {
            min: ty.page_count_initial().try_into().ok()?,
            max: match ty.page_count_max_declared() {
                Some(max) => max.try_into().ok()?,
                None => u32::MAX,
            },
        }))
    }

    pub(crate) fn from_table(ty: TableType) -> Option<Self> {
        if ty.arch() != MemoryArch::I32 {
            return None;
        }
        value_kind(WasmType::Ref(ty.element_type))?;
        Some(Self::Table {
            element: Box::new(wasm_valtype_t(WasmType::Ref(ty.element_type))),
            limits: wasm_limits_t {
                min: ty.size_initial.try_into().ok()?,
                max: match ty.size_max {
                    Some(max) => max.try_into().ok()?,
                    None => u32::MAX,
                },
            },
        })
    }

    pub(crate) fn func(&self) -> FuncType {
        let Self::Func { params, results } = self else { panic!("expected function type") };
        let types = |values: &wasm_valtype_vec_t| unsafe {
            values.as_slice().iter().map(|value| (**value).0).collect::<Vec<_>>()
        };
        FuncType::new(&types(params), &types(results))
    }

    pub(crate) fn global(&self) -> GlobalType {
        let Self::Global { content, mutable } = self else { panic!("expected global type") };
        GlobalType::new(content.0, *mutable)
    }

    pub(crate) fn memory(&self) -> MemoryType {
        let Self::Memory(limits) = self else { panic!("expected memory type") };
        MemoryType::default()
            .with_page_count_initial(limits.min as u64)
            .with_page_count_max((limits.max != u32::MAX).then_some(limits.max as u64))
    }

    pub(crate) fn table(&self) -> TableType {
        let Self::Table { element, limits } = self else { panic!("expected table type") };
        let WasmType::Ref(element) = element.0 else { panic!("expected reference type") };
        TableType::new(element, limits.min as u64, (limits.max != u32::MAX).then_some(limits.max as u64))
    }
}

macro_rules! owned_type {
    ($ty:ty, $copy:ident, $delete:ident) => {
        export! { pub unsafe extern "C" fn $copy(value: *const $ty) -> *mut $ty { unsafe { boxed((*value).clone()) } }}
        export! { pub unsafe extern "C" fn $delete(value: *mut $ty) { unsafe { crate::delete(value) }; }}
    };
}
owned_type!(wasm_valtype_t, wasm_valtype_copy, wasm_valtype_delete);
owned_type!(wasm_functype_t, wasm_functype_copy, wasm_functype_delete);
owned_type!(wasm_globaltype_t, wasm_globaltype_copy, wasm_globaltype_delete);
owned_type!(wasm_tabletype_t, wasm_tabletype_copy, wasm_tabletype_delete);
owned_type!(wasm_memorytype_t, wasm_memorytype_copy, wasm_memorytype_delete);
owned_type!(wasm_tagtype_t, wasm_tagtype_copy, wasm_tagtype_delete);
owned_type!(wasm_externtype_t, wasm_externtype_copy, wasm_externtype_delete);
owned_type!(wasm_importtype_t, wasm_importtype_copy, wasm_importtype_delete);
owned_type!(wasm_exporttype_t, wasm_exporttype_copy, wasm_exporttype_delete);

macro_rules! type_casts {
    ($kind:literal, $up:ident, $down:ident, $up_const:ident, $down_const:ident) => {
        export! { pub unsafe extern "C" fn $up(value: *mut wasm_externtype_t) -> *mut wasm_externtype_t { value }}
        export! { pub unsafe extern "C" fn $up_const(value: *const wasm_externtype_t) -> *const wasm_externtype_t { value }}
        export! { pub unsafe extern "C" fn $down(value: *mut wasm_externtype_t) -> *mut wasm_externtype_t {
            if unsafe { value.as_ref() }.is_some_and(|ty| ty.kind() == $kind) { value } else { ptr::null_mut() }
        }}
        export! { pub unsafe extern "C" fn $down_const(value: *const wasm_externtype_t) -> *const wasm_externtype_t { unsafe { $down(value.cast_mut()) } }}
    };
}
type_casts!(
    0,
    wasm_functype_as_externtype,
    wasm_externtype_as_functype,
    wasm_functype_as_externtype_const,
    wasm_externtype_as_functype_const
);
type_casts!(
    1,
    wasm_globaltype_as_externtype,
    wasm_externtype_as_globaltype,
    wasm_globaltype_as_externtype_const,
    wasm_externtype_as_globaltype_const
);
type_casts!(
    2,
    wasm_tabletype_as_externtype,
    wasm_externtype_as_tabletype,
    wasm_tabletype_as_externtype_const,
    wasm_externtype_as_tabletype_const
);
type_casts!(
    3,
    wasm_memorytype_as_externtype,
    wasm_externtype_as_memorytype,
    wasm_memorytype_as_externtype_const,
    wasm_externtype_as_memorytype_const
);
type_casts!(
    4,
    wasm_tagtype_as_externtype,
    wasm_externtype_as_tagtype,
    wasm_tagtype_as_externtype_const,
    wasm_externtype_as_tagtype_const
);

export! { pub unsafe extern "C" fn wasm_valtype_new(kind: u8) -> *mut wasm_valtype_t {
    value_type(kind).map_or_else(|| failure("invalid value kind"), |ty| boxed(wasm_valtype_t(ty)))
}}
export! { pub unsafe extern "C" fn wasm_valtype_kind(ty: *const wasm_valtype_t) -> u8 { unsafe { value_kind((*ty).0).unwrap() } }}
export! { pub unsafe extern "C" fn wasm_externtype_kind(ty: *const wasm_externtype_t) -> u8 { unsafe { (*ty).kind() } }}

export! { pub unsafe extern "C" fn wasm_functype_new(params: *mut wasm_valtype_vec_t, results: *mut wasm_valtype_vec_t) -> *mut wasm_functype_t {
    unsafe { boxed(wasm_externtype_t::Func { params: ptr::replace(params, Default::default()), results: ptr::replace(results, Default::default()) }) }
}}
export! { pub unsafe extern "C" fn wasm_functype_params(ty: *const wasm_functype_t) -> *const wasm_valtype_vec_t {
    let wasm_externtype_t::Func { params, .. } = (unsafe { &*ty }) else { return ptr::null(); }; params
}}
export! { pub unsafe extern "C" fn wasm_functype_results(ty: *const wasm_functype_t) -> *const wasm_valtype_vec_t {
    let wasm_externtype_t::Func { results, .. } = (unsafe { &*ty }) else { return ptr::null(); }; results
}}
export! { pub unsafe extern "C" fn wasm_globaltype_new(content: *mut wasm_valtype_t, mutable: u8) -> *mut wasm_globaltype_t {
    let content = unsafe { Box::from_raw(content) };
    if mutable > 1 { return failure("invalid mutability"); }
    boxed(wasm_externtype_t::Global { content, mutable: mutable == 1 })
}}
export! { pub unsafe extern "C" fn wasm_globaltype_content(ty: *const wasm_globaltype_t) -> *const wasm_valtype_t {
    let wasm_externtype_t::Global { content, .. } = (unsafe { &*ty }) else { return ptr::null(); }; &**content
}}
export! { pub unsafe extern "C" fn wasm_globaltype_mutability(ty: *const wasm_globaltype_t) -> u8 {
    let wasm_externtype_t::Global { mutable, .. } = (unsafe { &*ty }) else { return 0; }; u8::from(*mutable)
}}
export! { pub unsafe extern "C" fn wasm_tabletype_new(element: *mut wasm_valtype_t, limits: *const wasm_limits_t) -> *mut wasm_tabletype_t {
    let element = unsafe { Box::from_raw(element) };
    let limits = unsafe { *limits };
    if !matches!(element.0, WasmType::Ref(_)) || limits.min > limits.max { return failure("invalid table type"); }
    boxed(wasm_externtype_t::Table { element, limits })
}}
export! { pub unsafe extern "C" fn wasm_tabletype_element(ty: *const wasm_tabletype_t) -> *const wasm_valtype_t {
    let wasm_externtype_t::Table { element, .. } = (unsafe { &*ty }) else { return ptr::null(); }; &**element
}}
export! { pub unsafe extern "C" fn wasm_tabletype_limits(ty: *const wasm_tabletype_t) -> *const wasm_limits_t {
    let wasm_externtype_t::Table { limits, .. } = (unsafe { &*ty }) else { return ptr::null(); }; limits
}}
export! { pub unsafe extern "C" fn wasm_memorytype_new(limits: *const wasm_limits_t) -> *mut wasm_memorytype_t {
    let limits = unsafe { *limits };
    if limits.min > 65536 || limits.min > limits.max || (limits.max != u32::MAX && limits.max > 65536) { return failure("invalid memory limits"); }
    boxed(wasm_externtype_t::Memory(limits))
}}
export! { pub unsafe extern "C" fn wasm_memorytype_limits(ty: *const wasm_memorytype_t) -> *const wasm_limits_t {
    let wasm_externtype_t::Memory(limits) = (unsafe { &*ty }) else { return ptr::null(); }; limits
}}
export! { pub unsafe extern "C" fn wasm_tagtype_new(ty: *mut wasm_functype_t) -> *mut wasm_tagtype_t {
    let ty = unsafe { Box::from_raw(ty) };
    if !ty.func().results().is_empty() { return failure("tag type must have no results"); }
    boxed(wasm_externtype_t::Tag(ty))
}}
export! { pub unsafe extern "C" fn wasm_tagtype_functype(ty: *const wasm_tagtype_t) -> *const wasm_functype_t {
    let wasm_externtype_t::Tag(ty) = (unsafe { &*ty }) else { return ptr::null(); }; &**ty
}}
export! { pub unsafe extern "C" fn wasm_importtype_new(module: *mut wasm_byte_vec_t, name: *mut wasm_byte_vec_t, ty: *mut wasm_externtype_t) -> *mut wasm_importtype_t {
    unsafe { boxed(wasm_importtype_t { module: ptr::replace(module, Default::default()), name: ptr::replace(name, Default::default()), ty: Box::from_raw(ty) }) }
}}
export! { pub unsafe extern "C" fn wasm_exporttype_new(name: *mut wasm_byte_vec_t, ty: *mut wasm_externtype_t) -> *mut wasm_exporttype_t {
    unsafe { boxed(wasm_exporttype_t { name: ptr::replace(name, Default::default()), ty: Box::from_raw(ty) }) }
}}
export! { pub unsafe extern "C" fn wasm_importtype_module(ty: *const wasm_importtype_t) -> *const wasm_byte_vec_t { unsafe { &(*ty).module } }}
export! { pub unsafe extern "C" fn wasm_importtype_name(ty: *const wasm_importtype_t) -> *const wasm_byte_vec_t { unsafe { &(*ty).name } }}
export! { pub unsafe extern "C" fn wasm_importtype_type(ty: *const wasm_importtype_t) -> *const wasm_externtype_t { unsafe { &*(*ty).ty } }}
export! { pub unsafe extern "C" fn wasm_exporttype_name(ty: *const wasm_exporttype_t) -> *const wasm_byte_vec_t { unsafe { &(*ty).name } }}
export! { pub unsafe extern "C" fn wasm_exporttype_type(ty: *const wasm_exporttype_t) -> *const wasm_externtype_t { unsafe { &*(*ty).ty } }}
