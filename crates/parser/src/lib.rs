#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), feature(error_in_core))]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod conversion;
mod error;
mod module;
pub use error::*;
use module::ModuleReader;
use tinywasm_types::TinyWasmModule;

pub struct Parser {}

impl Parser {
    pub fn parse_module_bytes(wasm: &[u8]) -> Result<TinyWasmModule> {
        let reader = ModuleReader::new();
        reader.try_into()
    }

    pub fn parse_module_file(file_name: &str) -> Result<TinyWasmModule> {
        let reader = ModuleReader::new();
        reader.try_into()
    }

    #[cfg(feature = "std")]
    pub fn parse_module_stream(stream: impl std::io::Read) -> Result<TinyWasmModule> {
        let reader = ModuleReader::new();
        reader.try_into()
    }

    pub fn read_module_bytes(bytes: &[u8]) -> Result<TinyWasmModule> {
        unimplemented!()
    }
    pub fn read_module_file(file_name: &str) -> Result<TinyWasmModule> {
        unimplemented!()
    }
    #[cfg(feature = "std")]
    pub fn read_module_stream(stream: impl std::io::Read) -> Result<TinyWasmModule> {
        unimplemented!()
    }
}

impl TryFrom<ModuleReader<'_>> for TinyWasmModule {
    type Error = ParseError;

    fn try_from(reader: ModuleReader<'_>) -> Result<Self> {
        if !reader.end_reached {
            return Err(ParseError::EndNotReached);
        }

        unimplemented!()
    }
}
