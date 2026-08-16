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
mod validation;
mod visit;

#[cfg(parallel_parser)]
mod parallel;

pub use error::*;
use module::ModuleReader;
use validation::Validator;

#[cfg(feature = "validate")]
use wasmparser::WasmFeatures;

pub use tinywasm_types::Module;

/// Parser optimization and lowering options.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct ParserOptions {
    /// Whether to validate modules while parsing. Enabled by default when the
    /// `validate` feature is enabled.
    ///
    /// Requires the `validate` feature to have any effect.
    ///
    /// Disable this only for trusted input. Parsing without validation may produce
    /// a module that violates runtime assumptions.
    pub validation: bool,
    /// Whether to optimize local memory allocation by skipping allocation of unused local memories.
    pub optimize_local_memory_allocation: bool,
    /// Whether to run the peephole rewrite optimizer.
    pub optimize_rewrite: bool,
    /// Whether to deduplicate immutable function operands while parsing.
    ///
    /// This uses more parse CPU to reduce precompiled module and archive size.
    pub deduplicate_operands: bool,

    #[cfg(parallel_parser)]
    /// Number of threads to use for parallel parsing.
    ///
    /// Requires the `parallel` feature. Ignored when the feature is disabled.
    ///
    /// - `None`: auto-detect based on available parallelism
    /// - `Some(1)`: force single-threaded
    /// - `Some(n)`: use up to `n` workers
    pub parser_threads: Option<usize>,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            validation: cfg!(feature = "validate"),
            optimize_local_memory_allocation: true,
            optimize_rewrite: true,
            deduplicate_operands: false,
            #[cfg(parallel_parser)]
            parser_threads: None,
        }
    }
}

impl ParserOptions {
    /// Create parser options with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable WebAssembly validation.
    ///
    /// Requires the `validate` feature to have any effect.
    ///
    /// Disable this only for trusted input. Parsing without validation may produce
    /// a module that violates runtime assumptions.
    pub const fn with_validation(mut self, enabled: bool) -> Self {
        assert!(!enabled || cfg!(feature = "validate"), "validation requires the `validate` feature");
        self.validation = enabled;
        self
    }

    /// Returns whether WebAssembly validation is enabled.
    pub const fn validation(&self) -> bool {
        self.validation
    }

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

    /// Enable or disable parse-time deduplication of immutable function operands.
    pub const fn with_operand_deduplication(mut self, enabled: bool) -> Self {
        self.deduplicate_operands = enabled;
        self
    }

    /// Returns whether immutable function operands are deduplicated while parsing.
    pub const fn deduplicate_operands(&self) -> bool {
        self.deduplicate_operands
    }

    #[cfg(parallel_parser)]
    /// Set the number of threads for parallel parsing.
    ///
    /// Requires the `parallel` feature to have any effect.
    pub const fn with_parser_threads(mut self, threads: usize) -> Self {
        self.parser_threads = Some(threads);
        self
    }

    #[cfg(parallel_parser)]
    /// Returns the configured parser thread count, or `None` for auto-detect.
    pub const fn parser_threads(&self) -> Option<usize> {
        self.parser_threads
    }
}

/// A WebAssembly parser
#[derive(Debug, Default)]
pub struct Parser {
    options: ParserOptions,
}

impl Parser {
    /// Create a parser with the given options.
    pub const fn new(options: ParserOptions) -> Self {
        Self { options }
    }

    /// Read back parser options.
    pub const fn options(&self) -> &ParserOptions {
        &self.options
    }

    fn validator(&self) -> Option<Validator> {
        #[cfg(feature = "validate")]
        {
            let features = WasmFeatures::WASM3
                .difference(WasmFeatures::THREADS)
                .union(WasmFeatures::CUSTOM_PAGE_SIZES)
                .union(WasmFeatures::WIDE_ARITHMETIC)
                .union(WasmFeatures::COMPACT_IMPORTS);
            self.options.validation().then(|| Validator::new_with_features(features))
        }
        #[cfg(not(feature = "validate"))]
        {
            assert!(!self.options.validation(), "validation requires the `validate` feature");
            None
        }
    }

    #[cfg(feature = "std")]
    fn read_more(stream: &mut impl std::io::Read, buffer: &mut alloc::vec::Vec<u8>, hint: usize) -> Result<usize> {
        let len = buffer.len();
        buffer.resize(len + hint, 0);
        let read_bytes = stream
            .read(&mut buffer[len..])
            .map_err(|e| ParseError::Other(alloc::format!("Error reading from stream: {e}")))?;
        buffer.truncate(len + read_bytes);
        Ok(read_bytes)
    }

    /// Parse a [`Module`] from bytes
    pub fn parse_module_bytes(&self, wasm: impl AsRef<[u8]>) -> Result<Module> {
        let wasm = wasm.as_ref();
        let mut validator = self.validator();
        let mut reader = ModuleReader::default();

        for payload in wasmparser::Parser::new(0).parse_all(wasm) {
            match payload? {
                wasmparser::Payload::CodeSectionStart { count, range, size } => {
                    reader.begin_code_section(count, range, size, validator.as_mut(), &self.options)?;
                }
                wasmparser::Payload::CodeSectionEntry(function) => {
                    reader.process_borrowed_code_section_entry(function, validator.as_mut(), &self.options)?;
                }
                payload => reader.process_payload(payload, validator.as_mut())?,
            }
        }

        if !reader.end_reached {
            return Err(ParseError::EndNotReached);
        }

        reader.process_pending_functions(&self.options)?;
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
        let mut validator = self.validator();
        let mut reader = ModuleReader::default();
        let mut buffer = alloc::vec::Vec::new();
        let mut parser = wasmparser::Parser::new(0);
        let mut eof = false;
        let mut buffer_offset = 0;

        loop {
            match parser.parse(&buffer[buffer_offset..], eof)? {
                wasmparser::Chunk::NeedMoreData(hint) => {
                    if buffer_offset != 0 {
                        buffer.copy_within(buffer_offset.., 0);
                        buffer.truncate(buffer.len() - buffer_offset);
                        buffer_offset = 0;
                    }
                    let read_bytes = Self::read_more(&mut stream, &mut buffer, hint as usize)?;
                    eof = read_bytes == 0;
                }
                wasmparser::Chunk::Parsed { consumed, payload } => {
                    #[cfg(parallel_parser)]
                    let mut deferred_code_section = None;

                    match payload {
                        wasmparser::Payload::CodeSectionStart { count, range, size } => {
                            let defer = reader.begin_code_section(
                                count,
                                range.clone(),
                                size,
                                validator.as_mut(),
                                &self.options,
                            )?;

                            #[cfg(parallel_parser)]
                            if defer {
                                deferred_code_section = Some((count, range.end - size as usize, size as usize));
                            }

                            #[cfg(not(parallel_parser))]
                            let _ = defer;
                        }
                        wasmparser::Payload::CodeSectionEntry(function) => {
                            reader.process_inline_code_section_entry(function, validator.as_mut(), &self.options)?;
                        }
                        payload => {
                            reader.process_payload(payload, validator.as_mut())?;
                        }
                    }
                    buffer_offset += consumed;

                    #[cfg(parallel_parser)]
                    if let Some((count, body_offset, section_size)) = deferred_code_section {
                        while buffer.len() - buffer_offset < section_size {
                            let remaining = section_size - (buffer.len() - buffer_offset);
                            let read_bytes = Self::read_more(&mut stream, &mut buffer, remaining)?;
                            if read_bytes == 0 {
                                return Err(ParseError::ParseError {
                                    message: "unexpected end-of-file".into(),
                                    offset: body_offset + buffer.len() - buffer_offset,
                                });
                            }
                        }

                        let section_end = buffer_offset + section_size;
                        let section_bytes = alloc::sync::Arc::<[u8]>::from(buffer[buffer_offset..section_end].to_vec());
                        reader.queue_owned_code_section(count, body_offset, section_bytes, validator.as_mut())?;
                        parser.skip_section();
                        buffer_offset = section_end;
                        continue;
                    }

                    if reader.end_reached {
                        if buffer_offset != buffer.len() {
                            return Err(ParseError::Other("trailing bytes after end of module".into()));
                        }

                        if !eof {
                            let read_bytes = Self::read_more(&mut stream, &mut buffer, 1)?;
                            eof = read_bytes == 0;

                            if !eof {
                                return Err(ParseError::Other("trailing bytes after end of module".into()));
                            }
                        }
                    }

                    if reader.end_reached || eof {
                        reader.process_pending_functions(&self.options)?;
                        return reader.into_module(&self.options);
                    }
                }
            };
        }
    }
}

impl TryFrom<ModuleReader<'_>> for Module {
    type Error = ParseError;

    fn try_from(reader: ModuleReader<'_>) -> Result<Self> {
        reader.into_module(&ParserOptions::default())
    }
}

/// Parse a module from bytes
pub fn parse_bytes(wasm: &[u8]) -> Result<Module> {
    Parser::default().parse_module_bytes(wasm)
}

#[cfg(feature = "std")]
/// Parse a module from a file. Requires the `std` feature.
pub fn parse_file(path: impl AsRef<crate::std::path::Path> + Clone) -> Result<Module> {
    Parser::default().parse_module_file(path)
}

#[cfg(feature = "std")]
/// Parse a module from a stream. Requires the `std` feature.
pub fn parse_stream(stream: impl crate::std::io::Read) -> Result<Module> {
    Parser::default().parse_module_stream(stream)
}
