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
