//! Cargo-built implementation of the WebAssembly C API.
//!
//! The public contract is in `include/wasm.h` and `include/tinywasm.h`.
//! All pointer arguments follow those headers' ownership and lifetime rules.
//! Stores and their objects are confined to the creating thread.
#![allow(non_camel_case_types)]
#![deny(unsafe_op_in_unsafe_fn)]

#[macro_use]
mod macros;
mod externals;
mod function;
mod module;
mod objects;
mod runtime;
mod types;
mod values;
mod vectors;

#[cfg(test)]
mod tests;

use std::cell::RefCell;

thread_local! {
    static LAST_ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Records an error for the calling thread and returns the ABI failure value.
fn failure<T: Default>(error: impl std::fmt::Display) -> T {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = error.to_string().into_bytes());
    T::default()
}

/// Allocates an owned opaque handle.
fn boxed<T>(value: T) -> *mut T {
    Box::into_raw(Box::new(value))
}

/// Deletes a nullable owned opaque handle.
unsafe fn delete<T>(value: *mut T) {
    if !value.is_null() {
        // SAFETY: the caller transfers a handle allocated by `boxed` exactly once.
        unsafe { drop(Box::from_raw(value)) };
    }
}

export! {
/// Copies the calling thread's last error into an owned, nul-terminated byte vector.
///
/// # Safety
/// `out` must point to writable vector storage that does not own an allocation.
pub unsafe extern "C" fn tinywasm_last_error_message(out: *mut vectors::wasm_byte_vec_t) {
    let mut message = LAST_ERROR.with(|slot| slot.borrow().clone());
    message.push(0);
    unsafe { out.write(vectors::wasm_byte_vec_t::from_vec(message)) };
}
}
