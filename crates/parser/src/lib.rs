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
mod emitter;
mod error;
mod macros;
mod module;
mod selection;
mod validation;
mod visit;

#[cfg(all(test, feature = "std"))]
mod tests;

#[cfg(parallel_parser)]
mod parallel;

pub use error::*;
use module::ModuleReader;
use validation::Validator;

#[cfg(feature = "validate")]
use wasmparser::WasmFeatures;

pub use tinywasm_types::Module;

/// Optional limits for parsing modules from untrusted sources.
///
/// These limit encoded input and specific forms of parse-time expansion. They do
/// not constitute a hard bound on the parser's total memory use. Validation
/// should remain enabled for untrusted modules.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default)]
pub struct ParseLimits {
    /// Maximum encoded module size in bytes, including custom sections.
    pub max_module_bytes: Option<usize>,
    /// Maximum materialized entries in any one section. Recursive type groups
    /// and compact imports count by their expanded entries.
    pub max_section_items: Option<usize>,
    /// Maximum parameters plus declared locals in any one function.
    pub max_function_locals: Option<usize>,
    /// Maximum explicit targets in one `br_table` (excluding its default).
    pub max_br_table_targets: Option<usize>,
    /// Maximum elements in one `array.new_fixed`.
    pub max_array_new_fixed_elements: Option<usize>,
}

impl ParseLimits {
    /// Create limits with every bound disabled.
    pub const fn new() -> Self {
        Self {
            max_module_bytes: None,
            max_section_items: None,
            max_function_locals: None,
            max_br_table_targets: None,
            max_array_new_fixed_elements: None,
        }
    }

    /// Bound the encoded size of a module.
    pub const fn with_max_module_bytes(mut self, limit: usize) -> Self {
        self.max_module_bytes = Some(limit);
        self
    }

    /// Bound the number of materialized entries in each section.
    pub const fn with_max_section_items(mut self, limit: usize) -> Self {
        self.max_section_items = Some(limit);
        self
    }

    /// Bound parameters plus declared locals in each function.
    pub const fn with_max_function_locals(mut self, limit: usize) -> Self {
        self.max_function_locals = Some(limit);
        self
    }

    /// Bound explicit targets in each `br_table`.
    pub const fn with_max_br_table_targets(mut self, limit: usize) -> Self {
        self.max_br_table_targets = Some(limit);
        self
    }

    /// Bound elements in each `array.new_fixed`.
    pub const fn with_max_array_new_fixed_elements(mut self, limit: usize) -> Self {
        self.max_array_new_fixed_elements = Some(limit);
        self
    }
}

pub(crate) fn check_parse_limit(kind: ParseLimitKind, limit: Option<usize>, observed: usize) -> Result<()> {
    if let Some(limit) = limit
        && observed > limit
    {
        return Err(ParseError::LimitExceeded { kind, limit });
    }
    Ok(())
}

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

    /// Whether to enable some of the optimizations during lowering, such as bounded instruction selection.
    pub optimize: bool,

    /// Whether to deduplicate immutable function operands while parsing.
    pub deduplicate_operands: bool,

    /// Optional limits for parsing untrusted modules. Unlimited by default.
    pub limits: ParseLimits,

    #[cfg(parallel_parser)]
    /// Number of threads to use for parallel parsing.
    ///
    /// Requires the `parallel` feature. Ignored when the feature is disabled.
    ///
    /// - `None`: auto-detect based on available parallelism
    /// - `Some(1)`: force single-threaded
    /// - `Some(n)`: use up to `n` workers
    pub threads: Option<usize>,
}

impl Default for ParserOptions {
    fn default() -> Self {
        Self {
            validation: cfg!(feature = "validate"),
            optimize: true,
            deduplicate_operands: true,
            limits: ParseLimits::default(),
            #[cfg(parallel_parser)]
            threads: None,
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

    /// Enable or disable some of the optimizations during lowering, such as bounded instruction selection.
    pub const fn with_optimize(mut self, enabled: bool) -> Self {
        self.optimize = enabled;
        self
    }

    /// Returns whether some of the optimizations during lowering, such as bounded instruction selection, are enabled.
    pub const fn optimize(&self) -> bool {
        self.optimize
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

    /// Set parse-time limits.
    pub const fn with_limits(mut self, limits: ParseLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Returns the configured parse-time limits.
    pub const fn limits(&self) -> &ParseLimits {
        &self.limits
    }

    #[cfg(parallel_parser)]
    /// Set the number of threads for parallel parsing.
    ///
    /// Requires the `parallel` feature to have any effect.
    pub const fn with_threads(mut self, threads: usize) -> Self {
        self.threads = Some(threads);
        self
    }

    #[cfg(parallel_parser)]
    /// Returns the configured parser thread count, or `None` for auto-detect.
    pub const fn threads(&self) -> Option<usize> {
        self.threads
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
    fn read_more(
        stream: &mut impl std::io::Read,
        buffer: &mut alloc::vec::Vec<u8>,
        hint: usize,
        total_read: &mut usize,
        max_module_bytes: Option<usize>,
    ) -> Result<usize> {
        let len = buffer.len();
        // Size hints can come from untrusted section lengths.
        let mut increment = hint.clamp(1, 64 * 1024);
        if let Some(limit) = max_module_bytes {
            let remaining = limit.saturating_sub(*total_read);
            if remaining == 0 {
                // A module exactly at the limit may still need an EOF probe.
                let mut extra = [0];
                let read_bytes = loop {
                    match stream.read(&mut extra) {
                        Ok(read_bytes) => break read_bytes,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(ParseError::Other(alloc::format!("Error reading from stream: {e}"))),
                    }
                };
                if read_bytes != 0 {
                    return Err(ParseError::LimitExceeded { kind: ParseLimitKind::ModuleBytes, limit });
                }
                return Ok(0);
            }
            increment = increment.min(remaining);
        }
        let new_len =
            len.checked_add(increment).ok_or_else(|| ParseError::Other("stream buffer is too large".into()))?;
        buffer
            .try_reserve(increment)
            .map_err(|e| ParseError::Other(alloc::format!("Error reserving stream buffer: {e}")))?;
        buffer.resize(new_len, 0);
        let read_bytes = loop {
            match stream.read(&mut buffer[len..]) {
                Ok(read_bytes) => break read_bytes,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    buffer.truncate(len);
                    return Err(ParseError::Other(alloc::format!("Error reading from stream: {e}")));
                }
            }
        };
        buffer.truncate(len + read_bytes);
        *total_read =
            total_read.checked_add(read_bytes).ok_or_else(|| ParseError::Other("stream byte count overflow".into()))?;
        Ok(read_bytes)
    }

    /// Parse a [`Module`] from bytes
    pub fn parse_module_bytes(&self, wasm: impl AsRef<[u8]>) -> Result<Module> {
        let wasm = wasm.as_ref();
        check_parse_limit(ParseLimitKind::ModuleBytes, self.options.limits.max_module_bytes, wasm.len())?;
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
                payload => reader.process_payload(payload, validator.as_mut(), &self.options)?,
            }
        }

        if !reader.end_reached {
            return Err(ParseError::EndNotReached);
        }

        reader.process_pending_functions(&self.options)?;
        reader.into_module()
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
        let mut total_read = 0;

        loop {
            match parser.parse(&buffer[buffer_offset..], eof)? {
                wasmparser::Chunk::NeedMoreData(hint) => {
                    if buffer_offset != 0 {
                        buffer.copy_within(buffer_offset.., 0);
                        buffer.truncate(buffer.len() - buffer_offset);
                        buffer_offset = 0;
                    }
                    let read_bytes = Self::read_more(
                        &mut stream,
                        &mut buffer,
                        hint,
                        &mut total_read,
                        self.options.limits.max_module_bytes,
                    )?;
                    eof = read_bytes == 0;
                }
                wasmparser::Chunk::Parsed { consumed, payload } => {
                    #[cfg(parallel_parser)]
                    let mut deferred_code_section = None;

                    match payload {
                        wasmparser::Payload::CodeSectionStart { count, range, size } => {
                            let defer =
                                reader.begin_code_section(count, range, size, validator.as_mut(), &self.options)?;

                            #[cfg(parallel_parser)]
                            if defer {
                                deferred_code_section = Some((count, size as usize));
                            }

                            #[cfg(not(parallel_parser))]
                            let _ = defer;
                        }
                        wasmparser::Payload::CodeSectionEntry(function) => {
                            reader.process_inline_code_section_entry(function, validator.as_mut(), &self.options)?;
                        }
                        payload => {
                            reader.process_payload(payload, validator.as_mut(), &self.options)?;
                        }
                    }
                    buffer_offset += consumed;

                    #[cfg(parallel_parser)]
                    if let Some((count, section_size)) = deferred_code_section {
                        while buffer.len() - buffer_offset < section_size {
                            let remaining = section_size - (buffer.len() - buffer_offset);
                            let read_bytes = Self::read_more(
                                &mut stream,
                                &mut buffer,
                                remaining,
                                &mut total_read,
                                self.options.limits.max_module_bytes,
                            )?;
                            if read_bytes == 0 {
                                return Err(ParseError::ParseError {
                                    message: "unexpected end-of-file".into(),
                                    offset: parser.offset() + (buffer.len() - buffer_offset) as u64,
                                });
                            }
                        }

                        let section_end = buffer_offset + section_size;
                        let section_bytes =
                            tinywasm_types::Shared::<[u8]>::from(buffer[buffer_offset..section_end].to_vec());
                        reader.queue_owned_code_section(count, parser.offset(), section_bytes, validator.as_mut())?;
                        parser.skip_section();
                        buffer_offset = section_end;
                        continue;
                    }

                    if reader.end_reached {
                        if buffer_offset != buffer.len() {
                            return Err(ParseError::Other("trailing bytes after end of module".into()));
                        }

                        if !eof {
                            let read_bytes = Self::read_more(
                                &mut stream,
                                &mut buffer,
                                1,
                                &mut total_read,
                                self.options.limits.max_module_bytes,
                            )?;
                            eof = read_bytes == 0;

                            if !eof {
                                return Err(ParseError::Other("trailing bytes after end of module".into()));
                            }
                        }
                    }

                    if reader.end_reached || eof {
                        reader.process_pending_functions(&self.options)?;
                        return reader.into_module();
                    }
                }
            };
        }
    }
}

impl TryFrom<ModuleReader<'_>> for Module {
    type Error = ParseError;

    fn try_from(reader: ModuleReader<'_>) -> Result<Self> {
        reader.into_module()
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
