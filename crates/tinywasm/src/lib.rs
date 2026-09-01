#![no_std]
#![cfg_attr(feature = "nightly-tail-calls", allow(incomplete_features, internal_features))]
#![cfg_attr(
  feature = "nightly-tail-calls",
  feature(explicit_tail_calls, variant_count, core_intrinsics)
)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
))]
#![warn(missing_docs, rust_2018_idioms, unreachable_pub)]
#![cfg_attr(not(any(feature = "simd-x86", feature = "nightly-tail-calls")), forbid(unsafe_code))]
#![cfg_attr(any(feature = "simd-x86", feature = "nightly-tail-calls"), deny(unsafe_code))]

//! A small and portable WebAssembly interpreter.
//!
//! `tinywasm` passes the full WebAssembly 3.0 core testsuite and supports the
//! [Lime1](https://github.com/WebAssembly/tool-conventions/blob/main/Lime.md#lime1)
//! interoperability target. It is designed for embedding in applications, tools,
//! and `no_std + alloc` environments.

#![cfg_attr(docsrs, feature(doc_cfg))]
//!
//! ## Getting started
//!
//! Use [`parse_bytes`] to parse and validate a WebAssembly module, then instantiate it
//! in a [`Store`]. A module can be reused, while each store owns its runtime state.
//!
//! ```rust
//! # #[cfg(feature = "parser")]
//! # fn main() -> tinywasm::Result<()> {
//! use tinywasm::{ModuleInstance, Store};
//!
//! let wasm = include_bytes!("../../../examples/wasm/add.wasm");
//! let module = tinywasm::parse_bytes(wasm)?;
//!
//! let mut store = Store::default();
//! let instance = ModuleInstance::instantiate(&mut store, &module, None)?;
//!
//! let add = instance.func::<(i32, i32), i32>(&store, "add")?;
//! let result = add.call(&mut store, (1, 2))?;
//!
//! assert_eq!(result, 3);
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "parser"))]
//! # fn main() {}
//! ```
//!
//! Typed functions convert Rust values directly. Use [`ModuleInstance::func_untyped`]
//! and [`WasmValue`] when types are selected at runtime. References such as [`StructRef`]
//! and [`ExternRef`] are owned handles tied to their store. Cloning a managed reference
//! keeps its referent live.
//!
//! Construct a store with a custom [`Engine`] and [`engine::Config`] to configure
//! stack limits, fuel, and GC collection. A [`ResourceLimiter`] can bound guest memory,
//! table, and GC heap growth.
//!
//! ## References and GC
//!
//! Runtime references belong to a [`Store`]. Passing a reference to another store
//! returns [`Trap::InvalidStore`]. Managed references are cloneable owned handles, and
//! their referents become collectible after the last handle is dropped. Nullable typed
//! parameters and results use `Option<T>`. Use [`RefValue`] for dynamic reference values
//! and [`Store::gc`] to request collection explicitly.
//!
//! For more examples, see the [`examples`](https://github.com/explodingcamera/tinywasm/tree/next/examples) directory.
//!
//! ## Cargo features
//!
//! - **`full`:** Enables `archive`, `parallel-parser`, `parser`, and `validate`. Enabled by default.
//! - **`std`:** Enables `std` and parsing from files and streams. Enabled by default.
//! - **`parser`:** Enables `tinywasm-parser` and top-level parse helpers. Enabled by default.
//! - **`validate`:** Enables WebAssembly validation while parsing. Enabled by default and configurable through `ParserOptions`.
//! - **`parallel-parser`:** Parallelizes function parsing when `std` is enabled. Enabled by default.
//! - **`archive`:** Enables serialization and deserialization of the internal `twasm` format. Enabled by default.
//! - **`log`:** Enables integration with the `log` crate. Enabled by default.
//! - **`send`:** Makes stores and store-local handles movable across threads.
//! - **`portable-atomic`:** Supports targets without native atomic compare-and-swap.
//! - **`canonicalize-nans`:** Uses a [canonical NaN](https://en.wikipedia.org/wiki/NaN#Canonical_NaN) for normalized NaN results. Enabled by default.
//! - **`debug`:** Derives `Debug` for runtime types. Enabled by default.
//! - **`guest-debug`:** Exposes module-internal by-index inspection APIs (`*_by_index`).
//! - **`nightly-tail-calls`:** Uses Rust's unstable explicit tail calls for interpreter dispatch. Requires nightly Rust (recommended for maximum performance).
//! - **`simd-x86`:** Enables x86-specific SIMD intrinsics and uses `unsafe` internally.
//!
//! With default features disabled, `tinywasm` depends only on `core`, `alloc`, and `libm`,
//! making it usable in `no_std + alloc` environments with a custom allocator.
//!
//! ## Imports
//!
//! To provide imports to a module, you can use the [`Imports`] struct.
//! This struct allows you to register custom functions, globals, memories, tables,
//! tags, and other modules to be linked into the module when it is instantiated.
//!
//! See the [`Imports`] documentation for more information.

#[macro_use]
mod macros;
mod std;
extern crate alloc;

#[cfg(all(not(feature = "portable-atomic"), not(target_has_atomic = "32")))]
compile_error!("tinywasm requires native 32-bit atomics; enable the `portable-atomic` feature");

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
pub use func::{
    ExecProgress, FromWasmValues, FuncContext, FuncExecution, FuncExecutionTyped, Function, FunctionTyped,
    HostFunction, HostFunctionCallback, IntoWasmValues, WasmTypes, WasmValueType,
};
pub use imports::*;
pub use instance::{ExternItem, ModuleInstance};
pub use reference::*;
pub use store::*;

mod func;
mod imports;
mod instance;
mod reference;
mod shared;
mod store;

mod interpreter;
use interpreter::InterpreterRuntime;

/// Global configuration for the WebAssembly interpreter
pub mod engine;
pub use engine::{Engine, StackConfig};

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
    pub use crate::{AnyRef, ArrayRef, EqRef, ExnRef, ExternRef, FuncRef, I31Ref, RefValue, StructRef, WasmValue};
    pub use tinywasm_types::*;
}

pub use tinywasm_types::Module;
