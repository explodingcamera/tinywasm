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
pub use module::ModuleInstance;

pub mod types;
pub use types::*;

pub mod engine;
pub mod naive;

#[cfg(test)]
mod tests {
    use crate::std::println;
    use crate::{error::Result, naive, WasmValue};

    #[test]
    fn naive_add() -> Result<()> {
        let wasm = include_bytes!("../../../examples/wasm/add.wasm");
        let mut module = naive::Module::new(wasm)?;

        let args = [WasmValue::I32(1), WasmValue::I32(2)];
        let res = naive::run(&mut module, "add", &args)?;
        println!("res: {:?}", res);

        let args = [WasmValue::I64(1), WasmValue::I64(2)];
        let res = naive::run(&mut module, "add_64", &args)?;
        println!("res: {:?}", res);

        Ok(())
    }
}
