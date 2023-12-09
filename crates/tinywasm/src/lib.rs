#![no_std]
#![forbid(unsafe_code)]
#![doc(test(
    no_crate_inject,
    attr(
        deny(warnings, rust_2018_idioms),
        allow(dead_code, unused_assignments, unused_variables)
    )
))]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms, unreachable_pub)]
#![cfg_attr(feature = "nightly", feature(error_in_core))]

//! ## A tiny WebAssembly Runtime written in Rust

// compiler error when using no_std without nightly
#[cfg(all(not(feature = "std"), not(nightly)))]
const _: () = { compile_error!("`nightly` feature is required for `no_std`") };

mod std;
extern crate alloc;

// log for logging (optional).
#[cfg(feature = "logging")]
#[allow(clippy::single_component_path_imports)]
use log;

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

mod export;
pub use export::ExportInstance;

mod func;
pub use func::{FuncHandle, TypedFuncHandle};

mod runtime;
pub use runtime::*;

#[cfg(feature = "parser")]
/// Re-export of [`tinywasm_parser`]. Requires `parser` feature.
pub mod parser {
    pub use tinywasm_parser::*;
}

pub use tinywasm_types::*;

#[cfg(test)]
mod tests {

    // #[test]
    // fn naive_add() -> Result<()> {
    //     let wasm = include_bytes!("../../../examples/wasm/add.wasm");
    //     let mut module = naive::Module::new(wasm)?;

    //     let args = [WasmValue::I32(1), WasmValue::I32(2)];
    //     let res = naive::run(&mut module, "add", &args)?;
    //     println!("res: {:?}", res);

    //     let args = [WasmValue::I64(1), WasmValue::I64(2)];
    //     let res = naive::run(&mut module, "add_64", &args)?;
    //     println!("res: {:?}", res);

    //     Ok(())
    // }
}
