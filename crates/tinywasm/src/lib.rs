#![no_std]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
))]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms, unreachable_pub)]
#![forbid(unsafe_code)]
#![cfg_attr(feature = "unstable-simd", feature(portable_simd))]

//! A tiny WebAssembly Runtime written in Rust
//!
//! `TinyWasm` provides a minimal WebAssembly runtime for executing WebAssembly modules.
//! It currently supports all features of the WebAssembly MVP specification and is
//! designed to be easy to use and integrate in other projects.
//!
//! ## Features
//!- **`std`**\
//!  Enables the use of `std` and `std::io` for parsing from files and streams. This is enabled by default.
//!- **`logging`**\
//!  Enables logging using the `log` crate. This is enabled by default.
//!- **`parser`**\
//!  Enables the `tinywasm-parser` crate. This is enabled by default.
//!- **`archive`**\
//!  Enables pre-parsing of archives. This is enabled by default.
//!
//! With all these features disabled, `TinyWasm` only depends on `core`, `alloc` and `libm`.
//! By disabling `std`, you can use `TinyWasm` in `no_std` environments. This requires
//! a custom allocator and removes support for parsing from files and streams, but otherwise the API is the same.
//!
//! ## Getting Started
//! The easiest way to get started is to use the [`Module::parse_bytes`] function to load a
//! WebAssembly module from bytes. This will parse the module and validate it, returning
//! a [`Module`] that can be used to instantiate the module.
//!
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
//! // that takes and returns [`WasmValue`]s
//! let func = instance.exported_func::<(i32, i32), i32>(&mut store, "add")?;
//! let res = func.call(&mut store, (1, 2))?;
//!
//! assert_eq!(res, 3);
//! # Ok::<(), tinywasm::Error>(())
//! ```
//!
//! For more examples, see the [`examples`](https://github.com/explodingcamera/tinywasm/tree/main/examples) directory.
//!
//! ## Imports
//!
//! To provide imports to a module, you can use the [`Imports`] struct.
//! This struct allows you to register custom functions, globals, memories, tables,
//! and other modules to be linked into the module when it is instantiated.
//!
//! See the [`Imports`] documentation for more information.
//!
//! ## Runtime Configuration
//!
//! For resource-constrained targets, you can configure the initial memory allocation:
//!
//! ```rust
//! use tinywasm::{Store, Module, StackConfig};
//!
//! // Create a store with minimal initial allocation (90% reduction in pre-allocated memory)
//! let config = StackConfig::new()
//!     .with_value_stack_32_capacity(1024)  // 1KB instead of 32KB
//!     .with_value_stack_64_capacity(512)   // 512B instead of 16KB
//!     .with_value_stack_128_capacity(256)  // 256B instead of 8KB
//!     .with_value_stack_ref_capacity(128)  // 128B instead of 1KB

//!     .with_block_stack_capacity(32);      // 32 instead of 128
//! let mut store = Store::with_config(config);
//!
//! // Or create a partial configuration (only override what you need)
//! let config = StackConfig::new()
//!     .with_value_stack_32_capacity(2048); // Only override 32-bit stack size
//! let mut store = Store::with_config(config);
//! ```

mod std;
extern crate alloc;

// log for logging (optional).
#[cfg(feature = "logging")]
#[expect(clippy::single_component_path_imports)]
use log;

// noop fallback if logging is disabled.
#[cfg(not(feature = "logging"))]
#[allow(unused_imports, unused_macros)]
pub(crate) mod log {
    macro_rules! debug    ( ($($tt:tt)*) => {{}} );
    macro_rules! info    ( ($($tt:tt)*) => {{}} );
    macro_rules! error    ( ($($tt:tt)*) => {{}} );
    pub(crate) use debug;
    pub(crate) use error;
    pub(crate) use info;
}

mod error;
pub use error::*;
pub use func::{FuncHandle, FuncHandleTyped};
pub use imports::*;
pub use instance::ModuleInstance;
pub use module::Module;
pub use reference::*;
pub use store::*;

mod func;
mod imports;
mod instance;
mod module;
mod reference;
mod store;

/// Runtime for executing WebAssembly modules.
pub mod interpreter;
pub use interpreter::InterpreterRuntime;

/// Configuration for the WebAssembly interpreter's stack preallocation.
pub mod config;
pub use config::StackConfig;

#[cfg(feature = "parser")]
/// Re-export of [`tinywasm_parser`]. Requires `parser` feature.
pub mod parser {
    pub use tinywasm_parser::*;
}

/// Re-export of [`tinywasm_types`].
pub mod types {
    pub use tinywasm_types::*;
}

#[cold]
pub(crate) fn cold() {}

pub(crate) fn unlikely(b: bool) -> bool {
    if b {
        cold();
    };
    b
}
