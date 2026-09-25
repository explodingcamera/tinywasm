use std::{ptr, slice};

use crate::{objects::*, types::*, values::wasm_val_t};

/// An element owned by a C API vector.
pub trait Element: Default {
    /// Copies the element and any owned handle.
    ///
    /// # Safety
    /// The element and any handle it contains must be initialized and live.
    unsafe fn copy(&self) -> Self;
    /// Releases any owned handle.
    ///
    /// # Safety
    /// The element must own its handle and must not have been destroyed already.
    unsafe fn destroy(&mut self);
}

impl Element for u8 {
    unsafe fn copy(&self) -> Self {
        *self
    }
    unsafe fn destroy(&mut self) {}
}

impl<T: Clone> Element for *mut T {
    unsafe fn copy(&self) -> Self {
        unsafe { self.as_ref().map_or(ptr::null_mut(), |value| crate::boxed(value.clone())) }
    }
    unsafe fn destroy(&mut self) {
        unsafe { crate::delete(*self) };
        *self = ptr::null_mut();
    }
}

/// An owned, capacity-free C vector backed by a boxed slice.
#[repr(C)]
pub struct Vector<T: Element> {
    pub size: usize,
    pub data: *mut T,
}

impl<T: Element> Default for Vector<T> {
    fn default() -> Self {
        Self { size: 0, data: ptr::null_mut() }
    }
}

impl<T: Element> Vector<T> {
    /// Transfers a Rust vector to C ownership.
    pub(crate) fn from_vec(values: Vec<T>) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        let size = values.len();
        Self { size, data: Box::into_raw(values.into_boxed_slice()).cast() }
    }

    /// Borrows initialized elements. Zero-length vectors may have null data.
    ///
    /// # Safety
    /// For nonempty vectors, `data` must point to `size` live, initialized
    /// elements that are not mutated for the duration of the borrow.
    pub(crate) unsafe fn as_slice(&self) -> &[T] {
        if self.size == 0 { &[] } else { unsafe { slice::from_raw_parts(self.data, self.size) } }
    }

    /// Borrows initialized elements exclusively.
    ///
    /// # Safety
    /// For nonempty vectors, `data` must point to `size` live, initialized
    /// elements exclusively accessible for the duration of the borrow.
    pub(crate) unsafe fn as_mut_slice(&mut self) -> &mut [T] {
        if self.size == 0 { &mut [] } else { unsafe { slice::from_raw_parts_mut(self.data, self.size) } }
    }
}

impl<T: Element> Clone for Vector<T> {
    fn clone(&self) -> Self {
        // SAFETY: the source vector must contain initialized, live elements.
        unsafe { Self::from_vec(self.as_slice().iter().map(|value| value.copy()).collect()) }
    }
}

impl<T: Element> Drop for Vector<T> {
    fn drop(&mut self) {
        if self.size == 0 {
            return;
        }
        // SAFETY: owned vectors originate from `from_vec`, with exactly this length.
        unsafe {
            let mut values = Box::from_raw(ptr::slice_from_raw_parts_mut(self.data, self.size));
            for value in &mut values {
                value.destroy();
            }
        }
    }
}

macro_rules! vector_api {
    ($ty:ident, $element:ty, $empty:ident, $uninit:ident, $new:ident, $copy:ident, $delete:ident) => {
        pub type $ty = Vector<$element>;
        export! { pub unsafe extern "C" fn $empty(out: *mut $ty) {
            unsafe { out.write($ty::default()) };
        }}
        export! { pub unsafe extern "C" fn $uninit(out: *mut $ty, size: usize) {
            unsafe { out.write($ty::from_vec((0..size).map(|_| Default::default()).collect())) };
        }}
        export! { pub unsafe extern "C" fn $new(out: *mut $ty, size: usize, data: *const $element) {
            let values = (0..size).map(|i| unsafe { data.add(i).read() }).collect();
            unsafe { out.write($ty::from_vec(values)) };
        }}
        export! { pub unsafe extern "C" fn $copy(out: *mut $ty, source: *const $ty) {
            unsafe { out.write((*source).clone()) };
        }}
        export! { pub unsafe extern "C" fn $delete(value: *mut $ty) {
            if !value.is_null() { unsafe { drop(ptr::replace(value, $ty::default())) }; }
        }}
    };
}

vector_api!(
    wasm_byte_vec_t,
    u8,
    wasm_byte_vec_new_empty,
    wasm_byte_vec_new_uninitialized,
    wasm_byte_vec_new,
    wasm_byte_vec_copy,
    wasm_byte_vec_delete
);
vector_api!(
    wasm_val_vec_t,
    wasm_val_t,
    wasm_val_vec_new_empty,
    wasm_val_vec_new_uninitialized,
    wasm_val_vec_new,
    wasm_val_vec_copy,
    wasm_val_vec_delete
);
vector_api!(
    wasm_valtype_vec_t,
    *mut wasm_valtype_t,
    wasm_valtype_vec_new_empty,
    wasm_valtype_vec_new_uninitialized,
    wasm_valtype_vec_new,
    wasm_valtype_vec_copy,
    wasm_valtype_vec_delete
);
vector_api!(
    wasm_functype_vec_t,
    *mut wasm_functype_t,
    wasm_functype_vec_new_empty,
    wasm_functype_vec_new_uninitialized,
    wasm_functype_vec_new,
    wasm_functype_vec_copy,
    wasm_functype_vec_delete
);
vector_api!(
    wasm_globaltype_vec_t,
    *mut wasm_globaltype_t,
    wasm_globaltype_vec_new_empty,
    wasm_globaltype_vec_new_uninitialized,
    wasm_globaltype_vec_new,
    wasm_globaltype_vec_copy,
    wasm_globaltype_vec_delete
);
vector_api!(
    wasm_tabletype_vec_t,
    *mut wasm_tabletype_t,
    wasm_tabletype_vec_new_empty,
    wasm_tabletype_vec_new_uninitialized,
    wasm_tabletype_vec_new,
    wasm_tabletype_vec_copy,
    wasm_tabletype_vec_delete
);
vector_api!(
    wasm_memorytype_vec_t,
    *mut wasm_memorytype_t,
    wasm_memorytype_vec_new_empty,
    wasm_memorytype_vec_new_uninitialized,
    wasm_memorytype_vec_new,
    wasm_memorytype_vec_copy,
    wasm_memorytype_vec_delete
);
vector_api!(
    wasm_tagtype_vec_t,
    *mut wasm_tagtype_t,
    wasm_tagtype_vec_new_empty,
    wasm_tagtype_vec_new_uninitialized,
    wasm_tagtype_vec_new,
    wasm_tagtype_vec_copy,
    wasm_tagtype_vec_delete
);
vector_api!(
    wasm_externtype_vec_t,
    *mut wasm_externtype_t,
    wasm_externtype_vec_new_empty,
    wasm_externtype_vec_new_uninitialized,
    wasm_externtype_vec_new,
    wasm_externtype_vec_copy,
    wasm_externtype_vec_delete
);
vector_api!(
    wasm_importtype_vec_t,
    *mut wasm_importtype_t,
    wasm_importtype_vec_new_empty,
    wasm_importtype_vec_new_uninitialized,
    wasm_importtype_vec_new,
    wasm_importtype_vec_copy,
    wasm_importtype_vec_delete
);
vector_api!(
    wasm_exporttype_vec_t,
    *mut wasm_exporttype_t,
    wasm_exporttype_vec_new_empty,
    wasm_exporttype_vec_new_uninitialized,
    wasm_exporttype_vec_new,
    wasm_exporttype_vec_copy,
    wasm_exporttype_vec_delete
);
vector_api!(
    wasm_extern_vec_t,
    *mut wasm_extern_t,
    wasm_extern_vec_new_empty,
    wasm_extern_vec_new_uninitialized,
    wasm_extern_vec_new,
    wasm_extern_vec_copy,
    wasm_extern_vec_delete
);
vector_api!(
    wasm_frame_vec_t,
    *mut wasm_frame_t,
    wasm_frame_vec_new_empty,
    wasm_frame_vec_new_uninitialized,
    wasm_frame_vec_new,
    wasm_frame_vec_copy,
    wasm_frame_vec_delete
);
