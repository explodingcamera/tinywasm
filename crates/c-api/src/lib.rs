//! C API implementation for TinyWasm.
//!
//! See `include/wasm.h` and `include/tinywasm.h` for the C interface and ownership rules.
//! Use a store and its objects only on the thread that created the store.
//!
//! # Symbol prefix
//!
//! By default, the library exports the names in `wasm.h`. To avoid collisions
//! with another WebAssembly C API implementation, build with a prefix:
//!
//! ```sh
//! make -C crates/c-api TINYWASM_C_API_PREFIX=my_
//! ```
//!
//! Define the same prefix before including `tinywasm.h` in C or C++:
//!
//! ```c
//! #define TINYWASM_C_API_PREFIX my_
//! #include "tinywasm.h"
//! ```
//!
//! When building with Cargo directly, enable `custom-prefix` and set
//! `TINYWASM_C_API_PREFIX`. Include `tinywasm.h` before `wasm.h` so the aliases
//! take effect.
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
///
/// # Safety
/// `value` must be null or a live handle returned by `boxed` whose ownership
/// has been transferred to this call exactly once.
unsafe fn delete<T>(value: *mut T) {
    if !value.is_null() {
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
