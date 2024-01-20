#![no_std]
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), feature(error_in_core))]

mod std;
extern crate alloc;

// log for logging (optional).
#[cfg(feature = "logging")]
#[allow(clippy::single_component_path_imports)]
use log;

// noop fallback if logging is disabled.
#[cfg(not(feature = "logging"))]
mod log {
    macro_rules! debug    ( ($($tt:tt)*) => {{}} );
    pub(crate) use debug;
}

mod conversion;
mod error;
mod module;
use alloc::vec::Vec;
pub use error::*;
use module::ModuleReader;
use tinywasm_types::{Function, WasmFunction};
use wasmparser::Validator;

pub use tinywasm_types::TinyWasmModule;

#[derive(Default)]
pub struct Parser {}

impl Parser {
    pub fn new() -> Self {
        Self {}
    }

    pub fn parse_module_bytes(&self, wasm: impl AsRef<[u8]>) -> Result<TinyWasmModule> {
        let wasm = wasm.as_ref();
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
    pub fn parse_module_file(&self, path: impl AsRef<crate::std::path::Path> + Clone) -> Result<TinyWasmModule> {
        use alloc::format;
        let f = crate::std::fs::File::open(path.clone())
            .map_err(|e| ParseError::Other(format!("Error opening file {:?}: {}", path.as_ref(), e)))?;

        let mut reader = crate::std::io::BufReader::new(f);
        self.parse_module_stream(&mut reader)
    }

    #[cfg(feature = "std")]
    pub fn parse_module_stream(&self, mut stream: impl std::io::Read) -> Result<TinyWasmModule> {
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
                    let read_bytes = stream
                        .read(&mut buffer[len..])
                        .map_err(|e| ParseError::Other(format!("Error reading from stream: {}", e)))?;
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

        let func_types = reader.func_addrs;
        let funcs = reader
            .code
            .into_iter()
            .zip(func_types)
            .map(|(f, ty)| {
                Function::WasmFunction(WasmFunction {
                    instructions: f.body,
                    locals: f.locals,
                    ty,
                })
            })
            .collect::<Vec<_>>();

        let globals = reader.globals;
        let table_types = reader.table_types;

        Ok(TinyWasmModule {
            version: reader.version,
            start_func: reader.start_func,
            func_types: reader.func_types.into_boxed_slice(),
            funcs: funcs.into_boxed_slice(),
            exports: reader.exports.into_boxed_slice(),
            globals: globals.into_boxed_slice(),
            table_types: table_types.into_boxed_slice(),
            memory_types: reader.memory_types.into_boxed_slice(),
            imports: reader.imports.into_boxed_slice(),
            data: reader.data.into_boxed_slice(),
            elements: reader.elements.into_boxed_slice(),
        })
    }
}
