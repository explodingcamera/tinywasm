#![no_std]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
))]
#![warn(missing_docs, missing_debug_implementations, rust_2018_idioms, unreachable_pub)]
#![forbid(unsafe_code)]
//! See [`tinywasm`](https://docs.rs/tinywasm) for documentation.

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

// log for logging (optional).
#[cfg(feature = "logging")]
#[allow(clippy::single_component_path_imports, unused_imports)]
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

mod conversion;
mod error;
mod module;
mod visit;
pub use error::*;
use module::ModuleReader;
use wasmparser::{Validator, WasmFeaturesInflated};

pub use tinywasm_types::TinyWasmModule;

/// A WebAssembly parser
#[derive(Default, Debug)]
pub struct Parser {}

impl Parser {
    /// Create a new parser instance
    pub fn new() -> Self {
        Self {}
    }

    fn create_validator() -> Validator {
        let features = WasmFeaturesInflated {
            bulk_memory: true,
            floats: true,
            multi_value: true,
            mutable_global: true,
            reference_types: true,
            sign_extension: true,
            saturating_float_to_int: true,
            function_references: true,
            tail_call: true,
            multi_memory: true,
            memory64: false,
            simd: true,
            custom_page_sizes: true,

            gc_types: true,
            stack_switching: false,
            component_model: false,
            component_model_nested_names: false,
            component_model_values: false,
            component_model_more_flags: false,
            exceptions: false,
            extended_const: false,
            gc: false,
            memory_control: false,
            relaxed_simd: false,
            threads: false,
            shared_everything_threads: false,
            component_model_multiple_returns: false,
            legacy_exceptions: false,
        };
        Validator::new_with_features(features.into())
    }

    /// Parse a [`TinyWasmModule`] from bytes
    pub fn parse_module_bytes(&self, wasm: impl AsRef<[u8]>) -> Result<TinyWasmModule> {
        let wasm = wasm.as_ref();
        let mut validator = Self::create_validator();
        let mut reader = ModuleReader::new();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            reader.process_payload(payload?, &mut validator)?;
        }

        if !reader.end_reached {
            return Err(ParseError::EndNotReached);
        }

        reader.into_module()
    }

    #[cfg(feature = "std")]
    /// Parse a [`TinyWasmModule`] from a file. Requires `std` feature.
    pub fn parse_module_file(&self, path: impl AsRef<crate::std::path::Path> + Clone) -> Result<TinyWasmModule> {
        use alloc::format;
        let f = crate::std::fs::File::open(path.clone())
            .map_err(|e| ParseError::Other(format!("Error opening file {:?}: {}", path.as_ref(), e)))?;

        let mut reader = crate::std::io::BufReader::new(f);
        self.parse_module_stream(&mut reader)
    }

    #[cfg(feature = "std")]
    /// Parse a [`TinyWasmModule`] from a stream. Requires `std` feature.
    pub fn parse_module_stream(&self, mut stream: impl std::io::Read) -> Result<TinyWasmModule> {
        use alloc::format;

        let mut validator = Self::create_validator();
        let mut reader = ModuleReader::new();
        let mut buffer = alloc::vec::Vec::new();
        let mut parser = wasmparser::Parser::new(0);
        let mut eof = false;

        loop {
            match parser.parse(&buffer, eof)? {
                wasmparser::Chunk::NeedMoreData(hint) => {
                    let len = buffer.len();
                    buffer.extend((0..hint).map(|_| 0u8));
                    let read_bytes = stream
                        .read(&mut buffer[len..])
                        .map_err(|e| ParseError::Other(format!("Error reading from stream: {e}")))?;
                    buffer.truncate(len + read_bytes);
                    eof = read_bytes == 0;
                }
                wasmparser::Chunk::Parsed { consumed, payload } => {
                    reader.process_payload(payload, &mut validator)?;
                    buffer.drain(..consumed);
                    if eof || reader.end_reached {
                        return reader.into_module();
                    }
                }
            };
        }
    }
}

impl TryFrom<ModuleReader> for TinyWasmModule {
    type Error = ParseError;

    fn try_from(reader: ModuleReader) -> Result<Self> {
        reader.into_module()
    }
}
