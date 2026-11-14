use crate::ParseLimitKind;
use alloc::boxed::Box;
use core::fmt::{Debug, Display};
use wasmparser::Encoding;

/// A WebAssembly section kind.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// Custom metadata.
    Custom,
    /// Type definitions.
    Type,
    /// Imports.
    Import,
    /// Function type declarations.
    Function,
    /// Tables.
    Table,
    /// Memories.
    Memory,
    /// Globals.
    Global,
    /// Exports.
    Export,
    /// The start function.
    Start,
    /// Element segments.
    Element,
    /// Function bodies.
    Code,
    /// Data segments.
    Data,
    /// The data segment count.
    DataCount,
    /// Exception tags.
    Tag,
    /// The end of the module.
    End,
    /// An unrecognized section.
    Unknown(u8),
}

impl Display for SectionKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Custom => "custom",
            Self::Type => "type",
            Self::Import => "import",
            Self::Function => "function",
            Self::Table => "table",
            Self::Memory => "memory",
            Self::Global => "global",
            Self::Export => "export",
            Self::Start => "start",
            Self::Element => "element",
            Self::Code => "code",
            Self::Data => "data",
            Self::DataCount => "data count",
            Self::Tag => "tag",
            Self::End => "end",
            Self::Unknown(id) => return write!(f, "unknown (id {id})"),
        })
    }
}

/// The kind of index rejected by the parser.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    /// A function index.
    Function,
    /// A global index.
    Global,
    /// A memory index.
    Memory,
    /// A table index.
    Table,
    /// A tag index.
    Tag,
    /// A local index.
    Local,
    /// A struct field index.
    StructField,
    /// A branch label depth.
    Label,
    /// A type index within a recursive group.
    RecursiveGroupType,
}

impl Display for IndexKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Function => "function",
            Self::Global => "global",
            Self::Memory => "memory",
            Self::Table => "table",
            Self::Tag => "tag",
            Self::Local => "local",
            Self::StructField => "struct field",
            Self::Label => "label",
            Self::RecursiveGroupType => "recursive group type",
        })
    }
}

/// The required kind of a referenced type.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    /// A function type.
    Function,
    /// A struct type.
    Struct,
    /// An array type.
    Array,
}

impl Display for TypeKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Function => "function",
            Self::Struct => "struct",
            Self::Array => "array",
        })
    }
}

/// Errors that can occur when parsing a WebAssembly module
#[non_exhaustive]
pub enum ParseError {
    /// An invalid type was encountered
    InvalidType,
    /// An unsupported section was encountered
    UnsupportedSection(SectionKind),
    /// A duplicate section was encountered
    DuplicateSection(SectionKind),
    /// An empty section was encountered
    EmptySection(SectionKind),
    /// A supported section appeared in an invalid position.
    SectionOutOfOrder(SectionKind),
    /// An unsupported operator was encountered
    UnsupportedOperator(&'static str),
    /// The WebAssembly reader or validator rejected the module.
    Wasmparser(wasmparser::Error),
    /// An invalid encoding was encountered
    InvalidEncoding(Encoding),
    /// An invalid local count was encountered
    InvalidLocalCount {
        /// The expected local count
        expected: u32,
        /// The actual local count
        actual: u32,
    },
    /// An index does not identify an available module item or control frame.
    IndexOutOfBounds {
        /// The kind of item being indexed.
        kind: IndexKind,
        /// The invalid index.
        index: u32,
    },
    /// A type index is missing or does not reference the required kind of type.
    InvalidTypeReference {
        /// The required type kind.
        expected: TypeKind,
        /// The referenced type index.
        index: u32,
    },
    /// The end of the module was not reached
    EndNotReached,
    /// The input ended before a declared section was complete.
    UnexpectedEnd {
        /// The byte offset at the end of the available input.
        offset: u64,
    },
    /// Bytes remained after the end of a module or section.
    TrailingBytes {
        /// The byte offset of the first trailing byte.
        offset: u64,
    },
    /// A configured parse limit was exceeded.
    LimitExceeded {
        /// The kind of limit.
        kind: ParseLimitKind,
        /// The configured maximum.
        limit: usize,
    },
    /// An unknown error occurred
    Other(Box<str>),
}

impl Display for ParseError {
    #[cfg(not(feature = "debug"))]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::InvalidType => "invalid type",
            Self::UnsupportedSection(_) => "unsupported section",
            Self::DuplicateSection(_) => "duplicate section",
            Self::EmptySection(_) => "empty section",
            Self::SectionOutOfOrder(_) => "section out of order",
            Self::UnsupportedOperator(_) => "unsupported operator",
            Self::Wasmparser(_) => "error parsing module",
            Self::InvalidEncoding(_) => "invalid encoding",
            Self::InvalidLocalCount { .. } => "invalid local count",
            Self::IndexOutOfBounds { .. } => "index out of bounds",
            Self::InvalidTypeReference { .. } => "invalid type reference",
            Self::EndNotReached => "end of module not reached",
            Self::UnexpectedEnd { .. } => "unexpected end-of-file",
            Self::TrailingBytes { .. } => "trailing bytes",
            Self::LimitExceeded { .. } => "parse limit exceeded",
            Self::Other(_) => "unknown parse error",
        })
    }

    #[cfg(feature = "debug")]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidType => write!(f, "invalid type"),
            Self::UnsupportedSection(section) => write!(f, "unsupported section: {section}"),
            Self::DuplicateSection(section) => write!(f, "duplicate section: {section} section"),
            Self::EmptySection(section) => write!(f, "empty section: {section} section"),
            Self::SectionOutOfOrder(section) => write!(f, "section out of order: {section} section"),
            Self::UnsupportedOperator(operator) => write!(f, "unsupported operator: {operator}"),
            Self::Wasmparser(error) => {
                write!(f, "error parsing module: {} at offset {}", error.message(), error.offset())
            }
            Self::InvalidEncoding(encoding) => write!(f, "invalid encoding: {encoding:?}"),
            Self::InvalidLocalCount { expected, actual } => {
                write!(f, "invalid local count: expected {expected}, actual {actual}")
            }
            Self::IndexOutOfBounds { kind, index } => write!(f, "{kind} index out of bounds: {index}"),
            Self::InvalidTypeReference { expected, index } => {
                write!(f, "type index {index} does not reference a {expected} type")
            }
            Self::EndNotReached => write!(f, "end of module not reached"),
            Self::UnexpectedEnd { offset } => write!(f, "unexpected end-of-file at offset {offset}"),
            Self::TrailingBytes { offset } => write!(f, "trailing bytes at offset {offset}"),
            Self::LimitExceeded { kind, limit } => write!(f, "parse limit exceeded: {kind} (maximum {limit})"),
            Self::Other(message) => write!(f, "unknown error: {message}"),
        }
    }
}

impl PartialEq for ParseError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::InvalidType, Self::InvalidType) | (Self::EndNotReached, Self::EndNotReached) => true,
            (Self::UnsupportedOperator(a), Self::UnsupportedOperator(b)) => a == b,
            (Self::Other(a), Self::Other(b)) => a == b,
            (Self::UnsupportedSection(a), Self::UnsupportedSection(b))
            | (Self::DuplicateSection(a), Self::DuplicateSection(b))
            | (Self::EmptySection(a), Self::EmptySection(b))
            | (Self::SectionOutOfOrder(a), Self::SectionOutOfOrder(b)) => a == b,
            (Self::UnexpectedEnd { offset: a }, Self::UnexpectedEnd { offset: b })
            | (Self::TrailingBytes { offset: a }, Self::TrailingBytes { offset: b }) => a == b,
            (Self::Wasmparser(a), Self::Wasmparser(b)) => a.message() == b.message() && a.offset() == b.offset(),
            (Self::InvalidEncoding(a), Self::InvalidEncoding(b)) => a == b,
            (
                Self::InvalidLocalCount { expected: ae, actual: aa },
                Self::InvalidLocalCount { expected: be, actual: ba },
            ) => ae == be && aa == ba,
            (Self::IndexOutOfBounds { kind: ak, index: ai }, Self::IndexOutOfBounds { kind: bk, index: bi }) => {
                ak == bk && ai == bi
            }
            (
                Self::InvalidTypeReference { expected: ae, index: ai },
                Self::InvalidTypeReference { expected: be, index: bi },
            ) => ae == be && ai == bi,
            (Self::LimitExceeded { kind: ak, limit: al }, Self::LimitExceeded { kind: bk, limit: bl }) => {
                ak == bk && al == bl
            }
            _ => false,
        }
    }
}

impl Eq for ParseError {}

impl core::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Wasmparser(error) => Some(error),
            _ => None,
        }
    }
}

impl Debug for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Display::fmt(self, f)
    }
}

impl From<wasmparser::Error> for ParseError {
    fn from(value: wasmparser::Error) -> Self {
        Self::Wasmparser(value)
    }
}

pub(crate) type Result<T, E = ParseError> = core::result::Result<T, E>;
