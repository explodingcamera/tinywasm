use crate::{ParseError, Result};
use core::fmt::Display;

/// The input or parse-time expansion limit that was exceeded.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseLimitKind {
    /// Encoded module bytes.
    ModuleBytes,
    /// Materialized entries in one module section.
    SectionItems,
    /// Parameters plus declared locals in one function.
    FunctionLocals,
    /// Explicit targets in one `br_table`.
    BrTableTargets,
    /// Elements in one `array.new_fixed`.
    ArrayNewFixedElements,
}

impl Display for ParseLimitKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Self::ModuleBytes => "module bytes",
            Self::SectionItems => "section items",
            Self::FunctionLocals => "function locals",
            Self::BrTableTargets => "br_table targets",
            Self::ArrayNewFixedElements => "array.new_fixed elements",
        };
        f.write_str(name)
    }
}

/// Optional limits for parsing modules from untrusted sources.
///
/// These limit encoded input and specific forms of parse-time expansion. They do
/// not constitute a hard bound on the parser's total memory use. Validation
/// should remain enabled for untrusted modules.
///
/// ```
/// use tinywasm_parser::{ParseLimits, Parser, ParserOptions};
///
/// let limits = ParseLimits::new()
///     .with_max_module_bytes(16 * 1024 * 1024)
///     .with_max_section_items(100_000)
///     .with_max_function_locals(10_000)
///     .with_max_br_table_targets(4_096)
///     .with_max_array_new_fixed_elements(4_096);
/// let parser = Parser::new(ParserOptions::new().with_limits(limits));
/// ```
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

    pub(crate) fn check(&self, kind: ParseLimitKind, observed: usize) -> Result<()> {
        let limit = match kind {
            ParseLimitKind::ModuleBytes => self.max_module_bytes,
            ParseLimitKind::SectionItems => self.max_section_items,
            ParseLimitKind::FunctionLocals => self.max_function_locals,
            ParseLimitKind::BrTableTargets => self.max_br_table_targets,
            ParseLimitKind::ArrayNewFixedElements => self.max_array_new_fixed_elements,
        };
        if let Some(limit) = limit
            && observed > limit
        {
            return Err(ParseError::LimitExceeded { kind, limit });
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use crate::{ParseError, ParseLimitKind, ParseLimits, Parser, ParserOptions};
    use std::io::{Cursor, Read};
    use std::{vec, vec::Vec};

    #[cfg(parallel_parser)]
    use std::string::String;

    fn limited(limits: ParseLimits) -> Parser {
        Parser::new(ParserOptions::new().with_limits(limits))
    }

    fn assert_limit(error: ParseError, kind: ParseLimitKind, limit: usize) {
        assert_eq!(error, ParseError::LimitExceeded { kind, limit });
    }

    fn parse_error<T>(result: Result<T, ParseError>) -> ParseError {
        result.err().expect("expected a parse error")
    }

    fn uleb(mut value: u32, bytes: &mut Vec<u8>) {
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                break;
            }
        }
    }

    fn section(id: u8, payload: &[u8], wasm: &mut Vec<u8>) {
        wasm.push(id);
        uleb(payload.len() as u32, wasm);
        wasm.extend_from_slice(payload);
    }

    fn compact_local_declaration(count: u32) -> Vec<u8> {
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();
        section(1, &[1, 0x60, 0, 0], &mut wasm);
        section(3, &[1, 0], &mut wasm);
        let mut body = vec![1];
        uleb(count, &mut body);
        body.extend_from_slice(&[0x7f, 0x0b]);
        let mut code = vec![1];
        uleb(body.len() as u32, &mut code);
        code.extend_from_slice(&body);
        section(10, &code, &mut wasm);
        wasm
    }

    #[test]
    fn module_byte_limit_has_exact_boundary_for_bytes_and_streams() {
        let wasm = wat::parse_str("(module (func))").unwrap();
        let exact = wasm.len();
        let at_limit = limited(ParseLimits::new().with_max_module_bytes(exact));
        assert!(at_limit.parse_module_bytes(&wasm).is_ok());
        assert!(at_limit.parse_module_stream(Cursor::new(&wasm)).is_ok());

        let below = limited(ParseLimits::new().with_max_module_bytes(exact - 1));
        assert_limit(parse_error(below.parse_module_bytes(&wasm)), ParseLimitKind::ModuleBytes, exact - 1);
        assert_limit(
            parse_error(below.parse_module_stream(Cursor::new(&wasm))),
            ParseLimitKind::ModuleBytes,
            exact - 1,
        );

        let above = limited(ParseLimits::new().with_max_module_bytes(exact + 1));
        assert!(above.parse_module_bytes(&wasm).is_ok());
        assert!(above.parse_module_stream(Cursor::new(&wasm)).is_ok());
    }

    #[test]
    fn stream_limit_counts_total_bytes_even_when_buffer_slides() {
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();
        // A custom section with 65535 bytes of payload, supplied by an endless
        // reader. The parser must stop after the configured total input budget.
        wasm.extend_from_slice(&[0, 0xff, 0xff, 0x03]);
        let input = Cursor::new(wasm).chain(std::io::repeat(0));
        let parser = limited(ParseLimits::new().with_max_module_bytes(128));
        assert_limit(parse_error(parser.parse_module_stream(input)), ParseLimitKind::ModuleBytes, 128);
    }

    #[test]
    fn section_limit_rejects_declared_and_expanded_entries() {
        let functions = wat::parse_str("(module (func) (func))").unwrap();
        let parser = limited(ParseLimits::new().with_max_section_items(1));
        assert_limit(parse_error(parser.parse_module_bytes(&functions)), ParseLimitKind::SectionItems, 1);

        let rec_group = wat::parse_str("(module (rec (type (struct)) (type (array i32))))").unwrap();
        assert_limit(parse_error(parser.parse_module_bytes(&rec_group)), ParseLimitKind::SectionItems, 1);

        // One compact-import group materializes two function imports.
        let compact_imports =
            [0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 0x60, 0, 0, 2, 12, 1, 1, b'm', 0, 0x7e, 0, 0, 2, 1, b'a', 1, b'b'];
        assert_limit(parse_error(parser.parse_module_bytes(compact_imports)), ParseLimitKind::SectionItems, 1);
    }

    #[test]
    fn compact_local_count_is_checked_before_expansion() {
        let parser = limited(ParseLimits::new().with_max_function_locals(64));
        let too_many = compact_local_declaration(1_000_000);
        assert!(too_many.len() < 32);
        assert_limit(parse_error(parser.parse_module_bytes(&too_many)), ParseLimitKind::FunctionLocals, 64);

        let at_limit = compact_local_declaration(64);
        assert!(parser.parse_module_bytes(&at_limit).is_ok());
        let above = compact_local_declaration(65);
        assert_limit(parse_error(parser.parse_module_bytes(&above)), ParseLimitKind::FunctionLocals, 64);

        let with_param = wat::parse_str("(module (func (param i32) (local i32 i32)))").unwrap();
        let parser = limited(ParseLimits::new().with_max_function_locals(2));
        assert_limit(parse_error(parser.parse_module_bytes(&with_param)), ParseLimitKind::FunctionLocals, 2);
    }

    #[test]
    fn branch_table_fanout_is_checked_before_collection() {
        let wasm =
            wat::parse_str("(module (func (param i32) block block local.get 0 br_table 0 1 0 end end))").unwrap();
        let parser = limited(ParseLimits::new().with_max_br_table_targets(1));
        assert_limit(parse_error(parser.parse_module_bytes(&wasm)), ParseLimitKind::BrTableTargets, 1);
        let parser = limited(ParseLimits::new().with_max_br_table_targets(2));
        assert!(parser.parse_module_bytes(&wasm).is_ok());
    }

    #[test]
    fn array_new_fixed_fanout_is_checked_before_validation() {
        let wasm = wat::parse_str(
            "(module (type $a (array (mut i32))) (func unreachable array.new_fixed $a 1000000000 drop))",
        )
        .unwrap();
        assert!(wasm.len() < 64);
        let parser = limited(ParseLimits::new().with_max_array_new_fixed_elements(4));
        assert_limit(parse_error(parser.parse_module_bytes(&wasm)), ParseLimitKind::ArrayNewFixedElements, 4);

        let small = wat::parse_str("(module (type $a (array (mut i32))) (func i32.const 7 array.new_fixed $a 1 drop))")
            .unwrap();
        assert!(parser.parse_module_bytes(&small).is_ok());
    }

    #[test]
    fn fanout_and_local_limits_also_apply_without_validation() {
        let limits = ParseLimits::new()
            .with_max_function_locals(64)
            .with_max_br_table_targets(1)
            .with_max_array_new_fixed_elements(4);
        let parser = Parser::new(ParserOptions::new().with_validation(false).with_limits(limits));
        assert_limit(
            parse_error(parser.parse_module_bytes(compact_local_declaration(1_000_000))),
            ParseLimitKind::FunctionLocals,
            64,
        );
        let branch_table =
            wat::parse_str("(module (func (param i32) block block local.get 0 br_table 0 1 0 end end))").unwrap();
        assert_limit(parse_error(parser.parse_module_bytes(&branch_table)), ParseLimitKind::BrTableTargets, 1);
        let array = wat::parse_str(
            "(module (type $a (array (mut i32))) (func unreachable array.new_fixed $a 1000000000 drop))",
        )
        .unwrap();
        assert_limit(parse_error(parser.parse_module_bytes(&array)), ParseLimitKind::ArrayNewFixedElements, 4);
    }

    #[cfg(parallel_parser)]
    #[test]
    fn limits_match_for_serial_and_parallel_lowering() {
        let mut source = String::from("(module");
        for _ in 0..9 {
            source.push_str("(func ");
            source.push_str(&"nop ".repeat(2048));
            source.push(')');
        }
        source.push(')');
        let wasm = wat::parse_str(&source).unwrap();
        assert!(wasm.len() > 16 * 1024);
        let limits = ParseLimits::new()
            .with_max_module_bytes(wasm.len())
            .with_max_section_items(9)
            .with_max_function_locals(0)
            .with_max_br_table_targets(0)
            .with_max_array_new_fixed_elements(0);
        for threads in [1, 4] {
            let parser = Parser::new(ParserOptions::new().with_limits(limits).with_threads(threads));
            assert!(parser.parse_module_bytes(&wasm).is_ok());
            assert!(parser.parse_module_stream(Cursor::new(&wasm)).is_ok());
        }

        let mut oversized = String::from("(module");
        for index in 0..9 {
            oversized.push_str("(func ");
            if index == 8 {
                oversized.push_str("(local i32) ");
            }
            oversized.push_str(&"nop ".repeat(2048));
            oversized.push(')');
        }
        oversized.push(')');
        let oversized = wat::parse_str(&oversized).unwrap();
        let limits = ParseLimits::new()
            .with_max_module_bytes(oversized.len())
            .with_max_section_items(9)
            .with_max_function_locals(0);
        for threads in [1, 4] {
            let parser = Parser::new(ParserOptions::new().with_limits(limits).with_threads(threads));
            assert_limit(parse_error(parser.parse_module_bytes(&oversized)), ParseLimitKind::FunctionLocals, 0);
            assert_limit(
                parse_error(parser.parse_module_stream(Cursor::new(&oversized))),
                ParseLimitKind::FunctionLocals,
                0,
            );
        }
    }
}
