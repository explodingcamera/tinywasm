#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), feature(error_in_core))]

mod std;
extern crate alloc;

mod error;
pub mod instructions;
pub mod module;
pub use error::*;
pub use module::Module;
pub mod types;
pub use types::*;
pub mod runtime;

pub mod naive_runtime;

pub struct Store {}

pub struct Instance {}

#[cfg(test)]
mod tests {
    use crate::{error::Result, Module};

    #[test]
    fn it_works() -> Result<()> {
        let wasm = include_bytes!("../../../examples/wasm/add.wasm");
        let module = Module::new(wasm)?;

        Ok(())
    }
}
