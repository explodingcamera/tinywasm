#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
))]
#![warn(missing_docs, rust_2018_idioms, unreachable_pub)]
#![deny(unsafe_op_in_unsafe_fn)]

//! `wasm-c-api` for TinyWasm.
//!
//! The canonical public API lives in the vendored `include/wasm.h` header.

use core::ffi::c_char;
use core::ptr;

#[allow(non_camel_case_types, missing_docs, unreachable_pub)]
mod abi {
    use super::{c_char, ptr};

    pub type wasm_byte_t = c_char;
    pub type wasm_valkind_t = u8;

    pub const WASM_EXTERNREF: wasm_valkind_t = 128;

    #[repr(C)]
    pub struct wasm_byte_vec_t {
        pub size: usize,
        pub data: *mut wasm_byte_t,
    }

    #[repr(C)]
    pub union wasm_val_inner_t {
        pub i32: i32,
        pub i64: i64,
        pub f32: f32,
        pub f64: f64,
        pub ref_: *mut wasm_ref_t,
    }

    #[repr(C)]
    pub struct wasm_val_t {
        pub kind: wasm_valkind_t,
        pub of: wasm_val_inner_t,
    }

    #[repr(C)]
    pub struct wasm_ref_t {
        _private: u8,
    }

    #[repr(C)]
    pub struct wasm_config_t {
        _private: u8,
    }

    #[repr(C)]
    pub struct wasm_engine_t {
        _private: u8,
    }

    #[repr(C)]
    pub struct wasm_store_t {
        _private: u8,
    }

    fn into_owned_ptr<T>(value: T) -> *mut T {
        Box::into_raw(Box::new(value))
    }

    unsafe fn drop_owned_ptr<T>(ptr: *mut T) {
        if !ptr.is_null() {
            unsafe {
                drop(Box::from_raw(ptr));
            }
        }
    }

    unsafe fn byte_vec_from_parts(data: *const wasm_byte_t, size: usize) -> wasm_byte_vec_t {
        if size == 0 {
            return wasm_byte_vec_t { size: 0, data: ptr::null_mut() };
        }

        let mut bytes = vec![0 as wasm_byte_t; size];
        if !data.is_null() {
            unsafe {
                ptr::copy_nonoverlapping(data, bytes.as_mut_ptr(), size);
            }
        }

        let leaked = bytes.leak();
        wasm_byte_vec_t { size, data: leaked.as_mut_ptr() }
    }

    unsafe fn byte_vec_delete_inner(vec: *mut wasm_byte_vec_t) {
        if vec.is_null() {
            return;
        }

        let byte_vec = unsafe { &mut *vec };
        if !byte_vec.data.is_null() {
            unsafe {
                drop(Vec::from_raw_parts(byte_vec.data, byte_vec.size, byte_vec.size));
            }
        }
        byte_vec.size = 0;
        byte_vec.data = ptr::null_mut();
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn wasm_config_new() -> *mut wasm_config_t {
        into_owned_ptr(wasm_config_t { _private: 0 })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_config_delete(config: *mut wasm_config_t) {
        unsafe { drop_owned_ptr(config) }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn wasm_engine_new() -> *mut wasm_engine_t {
        into_owned_ptr(wasm_engine_t { _private: 0 })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_engine_new_with_config(config: *mut wasm_config_t) -> *mut wasm_engine_t {
        unsafe { wasm_config_delete(config) };
        wasm_engine_new()
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_engine_delete(engine: *mut wasm_engine_t) {
        unsafe { drop_owned_ptr(engine) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_store_new(_engine: *mut wasm_engine_t) -> *mut wasm_store_t {
        into_owned_ptr(wasm_store_t { _private: 0 })
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_store_delete(store: *mut wasm_store_t) {
        unsafe { drop_owned_ptr(store) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_byte_vec_new_empty(out: *mut wasm_byte_vec_t) {
        if out.is_null() {
            return;
        }

        unsafe {
            *out = wasm_byte_vec_t { size: 0, data: ptr::null_mut() };
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_byte_vec_new_uninitialized(out: *mut wasm_byte_vec_t, size: usize) {
        if out.is_null() {
            return;
        }

        unsafe {
            *out = byte_vec_from_parts(ptr::null(), size);
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_byte_vec_new(out: *mut wasm_byte_vec_t, size: usize, data: *const wasm_byte_t) {
        if out.is_null() {
            return;
        }

        unsafe {
            *out = byte_vec_from_parts(data, size);
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_byte_vec_copy(out: *mut wasm_byte_vec_t, src: *const wasm_byte_vec_t) {
        if out.is_null() {
            return;
        }

        if src.is_null() {
            unsafe { wasm_byte_vec_new_empty(out) };
            return;
        }

        let src = unsafe { &*src };
        unsafe {
            *out = byte_vec_from_parts(src.data, src.size);
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_byte_vec_delete(vec: *mut wasm_byte_vec_t) {
        unsafe { byte_vec_delete_inner(vec) }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_val_delete(val: *mut wasm_val_t) {
        if val.is_null() {
            return;
        }

        let val = unsafe { &mut *val };
        val.kind = WASM_EXTERNREF;
        val.of = wasm_val_inner_t { ref_: ptr::null_mut() };
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_val_copy(out: *mut wasm_val_t, src: *const wasm_val_t) {
        if out.is_null() {
            return;
        }

        if src.is_null() {
            unsafe {
                (*out).kind = WASM_EXTERNREF;
                (*out).of = wasm_val_inner_t { ref_: ptr::null_mut() };
            }
            return;
        }

        unsafe {
            ptr::copy_nonoverlapping(src, out, 1);
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn wasm_ref_delete(reference: *mut wasm_ref_t) {
        unsafe { drop_owned_ptr(reference) }
    }
}
