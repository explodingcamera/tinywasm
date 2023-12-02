#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), feature(error_in_core))]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

mod conversion;
mod error;
mod module;
use alloc::vec::Vec;
pub use error::*;
use module::ModuleReader;
use tinywasm_types::{Function, TinyWasmModule};
use wasmparser::Validator;

pub struct Parser {}

impl Parser {
    pub fn parse_module_bytes(wasm: &[u8]) -> Result<TinyWasmModule> {
        let mut validator = Validator::new();
        let mut reader = ModuleReader::new();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            reader.process_payload(payload?, &mut validator)?;
        }

        if !reader.end_reached {
            return Err(ParseError::EndNotReached);
        }

        reader.try_into()
    }

    #[cfg(feature = "std")]
    pub fn parse_module_file(path: impl AsRef<crate::std::path::Path>) -> Result<TinyWasmModule> {
        use alloc::format;
        let f = crate::std::fs::File::open("log.txt").map_err(|e| {
            ParseError::Other(format!("Error opening file {:?}: {}", path.as_ref(), e))
        })?;

        let mut reader = crate::std::io::BufReader::new(f);
        Self::parse_module_stream(&mut reader)
    }

    #[cfg(feature = "std")]
    pub fn parse_module_stream(mut stream: impl std::io::Read) -> Result<TinyWasmModule> {
        use alloc::format;

        let mut validator = Validator::new();
        let mut reader = ModuleReader::new();
        let mut buffer = Vec::new();
        let mut parser = wasmparser::Parser::new(0);
        let mut eof = false;

        loop {
            match parser.parse(&buffer, eof)? {
                wasmparser::Chunk::NeedMoreData(hint) => {
                    let len = buffer.len();
                    buffer.extend((0..hint).map(|_| 0u8));
                    let read_bytes = stream.read(&mut buffer[len..]).map_err(|e| {
                        ParseError::Other(format!("Error reading from stream: {}", e))
                    })?;
                    buffer.truncate(len + read_bytes);
                    eof = read_bytes == 0;
                }
                wasmparser::Chunk::Parsed { consumed, payload } => {
                    reader.process_payload(payload, &mut validator)?;
                    buffer.drain(..consumed);
                    if eof || reader.end_reached {
                        return reader.try_into();
                    }
                }
            };
        }
    }
}

impl TryFrom<ModuleReader> for TinyWasmModule {
    type Error = ParseError;

    fn try_from(reader: ModuleReader) -> Result<Self> {
        if !reader.end_reached {
            return Err(ParseError::EndNotReached);
        }

        let func_types = reader.function_section;
        let funcs = reader
            .code_section
            .into_iter()
            .zip(func_types)
            .map(|(f, ty)| Function {
                body: f.body,
                locals: f.locals,
                ty,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(TinyWasmModule {
            version: reader.version,
            start_func: reader.start_func,
            types: reader.type_section.into_boxed_slice(),
            funcs,
            exports: reader.export_section.into_boxed_slice(),
        })
    }
}
