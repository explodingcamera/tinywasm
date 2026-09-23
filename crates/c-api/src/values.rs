use crate::{
    boxed,
    objects::{ObjectKind, wasm_ref_t},
    runtime::StoreState,
    vectors::Element,
};
use std::{ptr, rc::Rc};
use tinywasm::{ExternRef, Function, RefValue, Store, WasmValue};

#[repr(C)]
#[derive(Clone, Copy)]
pub union wasm_val_union {
    pub i32: i32,
    pub i64: i64,
    pub f32: f32,
    pub f64: f64,
    pub reference: *mut wasm_ref_t,
}

#[repr(C)]
pub struct wasm_val_t {
    pub kind: u8,
    pub of: wasm_val_union,
}

impl Default for wasm_val_t {
    fn default() -> Self {
        Self { kind: 0, of: wasm_val_union { i64: 0 } }
    }
}

impl Element for wasm_val_t {
    unsafe fn copy(&self) -> Self {
        let of =
            if self.kind >= 128 { wasm_val_union { reference: unsafe { self.of.reference.copy() } } } else { self.of };
        Self { kind: self.kind, of }
    }
    unsafe fn destroy(&mut self) {
        if self.kind >= 128 {
            unsafe { self.of.reference.destroy() };
        }
        *self = Self::default();
    }
}

impl wasm_val_t {
    /// Reads a valid C value and roots any reference in the owning runtime store.
    pub(crate) unsafe fn to_runtime(&self, state: &Rc<StoreState>, store: &mut Store) -> tinywasm::Result<WasmValue> {
        Ok(unsafe {
            match self.kind {
                0 => WasmValue::I32(self.of.i32),
                1 => WasmValue::I64(self.of.i64),
                2 => WasmValue::F32(self.of.f32),
                3 => WasmValue::F64(self.of.f64),
                128 | 129 => WasmValue::Ref(reference_to_runtime(self.of.reference, self.kind, state, store)?),
                _ => return Err(tinywasm::Error::Other("invalid value kind".into())),
            }
        })
    }

    /// Allocates owned C references for a runtime result.
    pub(crate) fn from_runtime(
        value: &WasmValue,
        kind: u8,
        state: &Rc<StoreState>,
        store: &Store,
    ) -> tinywasm::Result<Self> {
        let of = match value {
            WasmValue::I32(value) => wasm_val_union { i32: *value },
            WasmValue::I64(value) => wasm_val_union { i64: *value },
            WasmValue::F32(value) => wasm_val_union { f32: *value },
            WasmValue::F64(value) => wasm_val_union { f64: *value },
            WasmValue::Ref(RefValue::Null) => wasm_val_union { reference: ptr::null_mut() },
            WasmValue::Ref(RefValue::Func(reference)) => {
                let function = Function::from_func_ref(store, *reference)?;
                wasm_val_union { reference: boxed(wasm_ref_t(state.intern(ObjectKind::Func(function, *reference)))) }
            }
            WasmValue::Ref(RefValue::Extern(reference)) => {
                let key = reference.key(store)?;
                let object = state
                    .objects
                    .borrow()
                    .get(key as usize)
                    .cloned()
                    .ok_or_else(|| tinywasm::Error::Other("unknown external reference".into()))?;
                wasm_val_union { reference: boxed(wasm_ref_t(object)) }
            }
            _ => return Err(tinywasm::Error::Other("value is not representable by wasm.h".into())),
        };
        Ok(Self { kind, of })
    }
}

/// Converts a borrowed C reference, preserving store identity.
pub(crate) unsafe fn reference_to_runtime(
    value: *const wasm_ref_t,
    kind: u8,
    state: &Rc<StoreState>,
    store: &mut Store,
) -> tinywasm::Result<RefValue> {
    let Some(value) = (unsafe { value.as_ref() }) else {
        return Ok(RefValue::Null);
    };
    if !value.0.store.ptr_eq(&Rc::downgrade(state)) {
        return Err(tinywasm::Trap::InvalidStore.into());
    }
    if kind == 129 {
        let ObjectKind::Func(_, reference) = &value.0.kind else {
            return Err(tinywasm::Error::Other("expected function reference".into()));
        };
        return Ok(RefValue::Func(*reference));
    }
    let mut objects = state.objects.borrow_mut();
    let key = match objects.iter().position(|object| Rc::ptr_eq(object, &value.0)) {
        Some(index) => index,
        None => {
            let index = objects.len();
            objects.push(value.0.clone());
            index
        }
    };
    let key = u32::try_from(key).map_err(|_| tinywasm::Error::Other("too many external references".into()))?;
    Ok(RefValue::Extern(ExternRef::try_new(store, key)?))
}

export! { pub unsafe extern "C" fn wasm_val_copy(out: *mut wasm_val_t, value: *const wasm_val_t) { unsafe { out.write((*value).copy()) }; }}
export! { pub unsafe extern "C" fn wasm_val_delete(value: *mut wasm_val_t) { unsafe { (*value).destroy() }; }}
