#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), feature(error_in_core))]

mod std;
extern crate alloc;

mod error;
pub use error::*;

pub mod store;
pub use store::Store;

pub mod module;
pub use module::Module;

pub mod instance;
pub use instance::ModuleInstance;

pub mod export;
pub use export::ExportInstance;

pub mod func;
pub use func::{FuncHandle, TypedFuncHandle};

pub use tinywasm_parser as parser;
pub use tinywasm_types::*;
pub mod runtime;

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
