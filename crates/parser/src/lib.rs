#![no_std]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_assignments, unused_variables))
))]
#![warn(missing_docs, rust_2018_idioms, unreachable_pub)]
#![forbid(unsafe_code)]
//! See [`tinywasm`](https://docs.rs/tinywasm) for documentation.

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

// log for logging (optional).
#[cfg(feature = "log")]
#[allow(clippy::single_component_path_imports, unused_imports)]
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

mod conversion;
mod error;
mod macros;
mod module;
mod optimize;
mod visit;
pub use error::*;
use module::ModuleReader;
use wasmparser::{Validator, WasmFeatures};

pub use tinywasm_types::Module;

/// Parser optimization and lowering options.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ParserOptions {
    /// Whether to optimize local memory allocation by skipping allocation of unused local memories.
    pub optimize_local_memory_allocation: bool,
    /// Whether to run the peephole rewrite optimizer.
    pub optimize_rewrite: bool,
    /// Whether to remove `Nop` and `MergeBarrier` instructions after rewriting.
    pub optimize_remove_nop: bool,
    /// Whether to invert conditional branches over an unconditional jump.
    pub optimize_branch_inversion: bool,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            optimize_local_memory_allocation: true,
            optimize_rewrite: true,
            optimize_remove_nop: true,
            optimize_branch_inversion: false,
        }
    }
}

impl ParserOptions {
    /// Enable or disable the optimization that skips allocating unused local memories.
    pub const fn with_local_memory_allocation_optimization(mut self, enabled: bool) -> Self {
        self.optimize_local_memory_allocation = enabled;
        self
    }

    /// Returns whether unused local memory allocation optimization is enabled.
    pub const fn optimize_local_memory_allocation(&self) -> bool {
        self.optimize_local_memory_allocation
    }

    /// Enable or disable the peephole rewrite optimizer.
    pub const fn with_rewrite_optimization(mut self, enabled: bool) -> Self {
        self.optimize_rewrite = enabled;
        self
    }

    /// Returns whether the peephole rewrite optimizer is enabled.
    pub const fn optimize_rewrite(&self) -> bool {
        self.optimize_rewrite
    }

    /// Enable or disable `Nop`/`MergeBarrier` removal after rewriting.
    pub const fn with_nop_removal_optimization(mut self, enabled: bool) -> Self {
        self.optimize_remove_nop = enabled;
        self
    }

    /// Returns whether `Nop`/`MergeBarrier` removal is enabled.
    pub const fn optimize_remove_nop(&self) -> bool {
        self.optimize_remove_nop
    }

    /// Enable or disable the optimization that inverts conditional branches over an unconditional jump.
    pub const fn with_branch_inversion_optimization(mut self, enabled: bool) -> Self {
        self.optimize_branch_inversion = enabled;
        self
    }

    /// Returns whether conditional branch inversion optimization is enabled.
    pub const fn optimize_branch_inversion(&self) -> bool {
        self.optimize_branch_inversion
    }
}

/// A WebAssembly parser
#[derive(Debug, Default)]
pub struct Parser {
    options: ParserOptions,
}

impl Parser {
    /// Create a new parser instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new parser with explicit options.
    pub fn with_options(options: ParserOptions) -> Self {
        Self { options }
    }

    /// Read back parser options.
    pub const fn options(&self) -> &ParserOptions {
        &self.options
    }

    fn create_validator(_options: ParserOptions) -> Validator {
        let features = WasmFeatures::CALL_INDIRECT_OVERLONG
            | WasmFeatures::BULK_MEMORY_OPT
            | WasmFeatures::RELAXED_SIMD
            | WasmFeatures::GC_TYPES
            | WasmFeatures::REFERENCE_TYPES
            | WasmFeatures::MUTABLE_GLOBAL
            | WasmFeatures::MULTI_VALUE
            | WasmFeatures::FLOATS
            | WasmFeatures::BULK_MEMORY
            | WasmFeatures::SATURATING_FLOAT_TO_INT
            | WasmFeatures::SIGN_EXTENSION
            | WasmFeatures::EXTENDED_CONST
            | WasmFeatures::FUNCTION_REFERENCES
            | WasmFeatures::TAIL_CALL
            | WasmFeatures::MULTI_MEMORY
            | WasmFeatures::SIMD
            | WasmFeatures::MEMORY64
            | WasmFeatures::CUSTOM_PAGE_SIZES
            | WasmFeatures::WIDE_ARITHMETIC;
        Validator::new_with_features(features)
    }

    /// Parse a [`Module`] from bytes
    pub fn parse_module_bytes(&self, wasm: impl AsRef<[u8]>) -> Result<Module> {
        let wasm = wasm.as_ref();
        let mut validator = Self::create_validator(self.options.clone());
        let mut reader = ModuleReader::default();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            reader.process_payload(payload?, &mut validator)?;
        }

        if !reader.end_reached {
            return Err(ParseError::EndNotReached);
        }

        reader.into_module(&self.options)
    }

    #[cfg(feature = "std")]
    /// Parse a [`Module`] from a file. Requires `std` feature.
    pub fn parse_module_file(&self, path: impl AsRef<crate::std::path::Path> + Clone) -> Result<Module> {
        let file = crate::std::fs::File::open(&path)
            .map_err(|e| ParseError::Other(alloc::format!("Error opening file {:?}: {}", path.as_ref(), e)))?;
        self.parse_module_stream(&mut crate::std::io::BufReader::new(file))
    }

    #[cfg(feature = "std")]
    /// Parse a [`Module`] from a stream. Requires `std` feature.
    pub fn parse_module_stream(&self, mut stream: impl std::io::Read) -> Result<Module> {
        let mut validator = Self::create_validator(self.options.clone());
        let mut reader = ModuleReader::default();
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
                        .map_err(|e| ParseError::Other(alloc::format!("Error reading from stream: {e}")))?;
                    buffer.truncate(len + read_bytes);
                    eof = read_bytes == 0;
                }
                wasmparser::Chunk::Parsed { consumed, payload } => {
                    reader.process_payload(payload, &mut validator)?;
                    buffer.drain(..consumed);
                    if eof || reader.end_reached {
                        return reader.into_module(&self.options);
                    }
                }
            };
        }
    }
}

impl TryFrom<ModuleReader> for Module {
    type Error = ParseError;

    fn try_from(reader: ModuleReader) -> Result<Self> {
        reader.into_module(&ParserOptions::default())
    }
}

/// Parse a module from bytes
pub fn parse_bytes(wasm: &[u8]) -> Result<Module> {
    let data = Parser::new().parse_module_bytes(wasm)?;
    Ok(data)
}

#[cfg(feature = "std")]
/// Parse a module from a file. Requires the `std` feature.
pub fn parse_file(path: impl AsRef<crate::std::path::Path> + Clone) -> Result<Module> {
    let data = Parser::new().parse_module_file(path)?;
    Ok(data)
}

#[cfg(feature = "std")]
/// Parse a module from a stream. Requires `parser` and `std` features.
pub fn parse_stream(stream: impl crate::std::io::Read) -> Result<Module> {
    let data = Parser::new().parse_module_stream(stream)?;
    Ok(data)
}
