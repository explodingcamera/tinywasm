#![no_std]
#![forbid(unsafe_code)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
))]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms, unreachable_pub)]
#![cfg_attr(nightly, feature(error_in_core))]

//! A tiny WebAssembly Runtime written in Rust
//!
//! TinyWasm provides a minimal WebAssembly runtime for executing WebAssembly modules.
//! It currently supports a subset of the WebAssembly MVP specification and is intended
//! to be useful for embedded systems and other environments where a full-featured
//! runtime is not required.
//!
//! ## Getting Started
//!
//! The easiest way to get started is to use the [`Module::parse_bytes`] function to load a
//! WebAssembly module from bytes. This will parse the module and validate it, returning
//! a [`Module`] that can be used to instantiate the module.
//!
//! ```rust
//! use tinywasm::{Store, Module};
//!
//! // Load a module from bytes
//! let wasm = include_bytes!("../../../examples/wasm/add.wasm");
//! let module = Module::parse_bytes(wasm)?;
//!
//! // Create a new store
//! // Stores are used to allocate objects like functions and globals
//! let mut store = Store::default();
//!
//! // Instantiate the module
//! // This will allocate the module and its globals into the store
//! // and execute the module's start function.
//! // Every ModuleInstance has its own ID space for functions, globals, etc.
//! let instance = module.instantiate(&mut store, None)?;
//!
//! // Get a typed handle to the exported "add" function
//! // Alternatively, you can use `instance.get_func` to get an untyped handle
//! // that takes and returns WasmValue types
//! let func = instance.typed_func::<(i32, i32), (i32,)>(&mut store, "add")?;
//! let res = func.call(&mut store, (1, 2))?;
//!
//! assert_eq!(res, (3,));
//! # Ok::<(), tinywasm::Error>(())
//! ```
//!
//! ## Features
//! - `std` (default): Enables the use of `std` and `std::io` for parsing from files and streams.
//! - `logging` (default): Enables logging via the `log` crate.
//! - `parser` (default): Enables the `tinywasm_parser` crate for parsing WebAssembly modules.
//!
//! ## No-std support
//! TinyWasm supports `no_std` environments by disabling the `std` feature and registering
//! a custom allocator. This removes support for parsing from files and streams,
//! but otherwise the API is the same.
//!
//! Additionally, if you want proper error types, you must use a `nightly` compiler to have the error trait in core.

mod std;
extern crate alloc;

// log for logging (optional).
#[cfg(feature = "logging")]
#[allow(clippy::single_component_path_imports)]
use log;

// noop fallback if logging is disabled.
#[cfg(not(feature = "logging"))]
pub(crate) mod log {
    macro_rules! debug    ( ($($tt:tt)*) => {{}} );
    pub(crate) use debug;
}

mod error;
pub use error::*;

mod store;
pub use store::*;

mod module;
pub use module::Module;

mod instance;
pub use instance::ModuleInstance;

mod func;
pub use func::{FuncHandle, TypedFuncHandle};

mod imports;
pub use imports::*;

mod runtime;
pub use runtime::*;

#[cfg(feature = "parser")]
/// Re-export of [`tinywasm_parser`]. Requires `parser` feature.
pub mod parser {
    pub use tinywasm_parser::*;
}

/// Re-export of [`tinywasm_types`].
pub mod types {
    pub use tinywasm_types::*;
}
