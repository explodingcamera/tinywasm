#![no_std]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
))]
#![warn(missing_docs, rust_2018_idioms, unreachable_pub)]
#![cfg_attr(not(feature = "simd-x86"), forbid(unsafe_code))]
#![cfg_attr(feature = "simd-x86", deny(unsafe_code))]

//! `tinywasm` provides a small, portable WebAssembly interpreter with support for
//! the WebAssembly MVP, WebAssembly 2.0, and a growing set of newer proposals.
//! It is designed to stay lightweight while still being practical to embed in
//! applications, tools, and `no_std + alloc` environments.
//!
//! ## Features
//! - **`std`**\
//!   Enables parsing from files and streams. Enabled by default.
//! - **`log`**\
//!   Enables integration with the `log` crate. Enabled by default.
//! - **`parser`**\
//!   Enables the bundled `tinywasm-parser` crate and top-level parse helpers. Enabled by default.
//! - **`archive`**\
//!   Enables serialization and deserialization of compiled modules in the internal `twasm` format. Enabled by default.
//! - **`canonicalize-nans`**\
//!   Canonicalizes NaN values to a single representation. Enabled by default.
//! - **`debug`**\
//!   Derives `Debug` for runtime types. Enabled by default.
//! - **`parallel-parser`**\
//!   Parallelizes function parsing and validation across threads when `std` is enabled. Enabled by default.
//! - **`guest-debug`**\
//!   Exposes module-internal by-index inspection APIs (`*_by_index`).
//! - **`simd-x86`**\
//!   Enables x86-specific SIMD intrinsics for selected operations and uses `unsafe` internally.
//!
//! With default features disabled, `tinywasm` only depends on `core`, `alloc`, and `libm`.
//! By disabling `std`, you can use `tinywasm` in `no_std` environments. This requires
//! a custom allocator and removes support for parsing from files and streams, but otherwise the API is the same.

#![cfg_attr(docsrs, feature(doc_cfg))]
//!
//! ## Getting Started
//! The easiest way to get started is to use the [`crate::parse_bytes`] function to load a
//! WebAssembly module from bytes. This will parse the module and validate it, returning
//! a [`Module`] that can be used to instantiate the module.
//!
//!
//! ```rust
//! use tinywasm::{ModuleInstance, Store};
//!
//! // Load a module from bytes
//! let wasm = include_bytes!("../../../examples/wasm/add.wasm");
//! let module = tinywasm::parse_bytes(wasm)?;
//!
//! // Create a new store
//! // Stores are used to allocate objects like functions and globals
//! let mut store = Store::default();
//!
//! // Instantiate the module
//! // This will allocate the module and its globals into the store
//! // and execute the module's start function.
//! // Every ModuleInstance has its own ID space for functions, globals, etc.
//! let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
//!
//! // Get a typed handle to the exported "add" function
//! // Alternatively, you can use `instance.func` to get an untyped handle
//! // that takes and returns [`WasmValue`]s
//! let func = instance.func::<(i32, i32), i32>(&mut store, "add")?;
//! let res = func.call(&mut store, (1, 2))?;
//!
//! assert_eq!(res, 3);
//! # Ok::<(), tinywasm::Error>(())
//! ```
//!
//! For non-default runtime behavior, construct a [`Store`] with a custom [`Engine`]
//! and [`engine::Config`] to control stack sizing, fuel accounting, memory backends,
//! and trap-on-OOM behavior.
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

mod std;
extern crate alloc;

// log for logging (optional).
#[cfg(feature = "log")]
#[expect(clippy::single_component_path_imports)]
use log;

// noop fallback if logging is disabled.
#[cfg(not(feature = "log"))]
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
pub use func::{ExecProgress, FuncContext, FuncExecution, FuncExecutionTyped, Function, FunctionTyped, HostFunction};
pub use imports::*;
pub use instance::{ExternItem, ModuleInstance};
pub use reference::*;
pub use store::*;

mod func;
mod imports;
mod instance;
mod reference;
mod store;

mod interpreter;
use interpreter::InterpreterRuntime;

/// Global configuration for the WebAssembly interpreter
pub mod engine;
pub use engine::{Engine, LazyLinearMemory, LinearMemory, MemoryBackend, PagedMemory, StackConfig, VecMemory};

#[cfg(feature = "parser")]
/// Re-export of [`tinywasm_parser`]. Requires `parser` feature.
pub mod parser {
    pub use tinywasm_parser::*;
}

#[cfg(feature = "parser")]
pub use parser::parse_bytes;
#[cfg(all(feature = "parser", feature = "std"))]
pub use parser::{parse_file, parse_stream};

/// Re-export of [`tinywasm_types`].
pub mod types {
    pub use tinywasm_types::*;
}

pub use tinywasm_types::Module;

pub(crate) fn unlikely(b: bool) -> bool {
    if b {
        core::hint::cold_path();
    };
    b
}
