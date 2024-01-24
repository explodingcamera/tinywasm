use alloc::string::{String, ToString};
use core::fmt::Display;
use tinywasm_types::FuncType;

#[cfg(feature = "parser")]
use tinywasm_parser::ParseError;

/// A tinywasm error
#[derive(Debug)]
pub enum Error {
    #[cfg(feature = "parser")]
    /// A parsing error occurred
    ParseError(ParseError),

    #[cfg(feature = "std")]
    /// An I/O error occurred
    Io(crate::std::io::Error),

    /// A WebAssembly feature is not supported
    UnsupportedFeature(String),

    /// An unknown error occurred
    Other(String),

    /// A WebAssembly trap occurred
    Trap(Trap),

    /// A linking error occurred
    Linker(LinkingError),

    /// A function did not return a value
    FuncDidNotReturn,

    /// The stack is empty
    StackUnderflow,

    /// The label stack is empty
    LabelStackUnderflow,

    /// An invalid label type was encountered
    InvalidLabelType,

    /// The call stack is empty
    CallStackEmpty,

    /// The store is not the one that the module instance was instantiated in
    InvalidStore,
}

#[derive(Debug)]
/// A linking error
pub enum LinkingError {
    /// An unknown import was encountered
    UnknownImport {
        /// The module name
        module: String,
        /// The import name
        name: String,
    },
    /// A mismatched import type was encountered
    IncompatibleImportType {
        /// The module name
        module: String,
        /// The import name
        name: String,
    },
}

impl LinkingError {
    pub(crate) fn incompatible_import_type(import: &tinywasm_types::Import) -> Self {
        Self::IncompatibleImportType { module: import.module.to_string(), name: import.name.to_string() }
    }
}

#[derive(Debug)]
/// A WebAssembly trap
///
/// See <https://webassembly.github.io/spec/core/intro/overview.html#trap>
pub enum Trap {
    /// An unreachable instruction was executed
    Unreachable,

    /// An out-of-bounds memory access occurred
    MemoryOutOfBounds {
        /// The offset of the access
        offset: usize,
        /// The size of the access
        len: usize,
        /// The maximum size of the memory
        max: usize,
    },

    /// An out-of-bounds table access occurred
    TableOutOfBounds {
        /// The offset of the access
        offset: usize,
        /// The size of the access
        len: usize,
        /// The maximum size of the memory
        max: usize,
    },

    /// A division by zero occurred
    DivisionByZero,

    /// Invalid Integer Conversion
    InvalidConversionToInt,

    /// Integer Overflow
    IntegerOverflow,

    /// Call stack overflow
    CallStackOverflow,

    /// An undefined element was encountered
    UndefinedElement {
        /// The element index
        index: usize,
    },

    /// An uninitialized element was encountered
    UninitializedElement {
        /// The element index
        index: usize,
    },

    /// Indirect call type mismatch
    IndirectCallTypeMismatch {
        /// The expected type
        expected: FuncType,
        /// The actual type
        actual: FuncType,
    },
}

impl Trap {
    /// Get the message of the trap
    pub fn message(&self) -> &'static str {
        match self {
            Self::Unreachable => "unreachable",
            Self::MemoryOutOfBounds { .. } => "out of bounds memory access",
            Self::TableOutOfBounds { .. } => "out of bounds table access",
            Self::DivisionByZero => "integer divide by zero",
            Self::InvalidConversionToInt => "invalid conversion to integer",
            Self::IntegerOverflow => "integer overflow",
            Self::CallStackOverflow => "call stack exhausted",
            Self::UndefinedElement { .. } => "undefined element",
            Self::UninitializedElement { .. } => "uninitialized element",
            Self::IndirectCallTypeMismatch { .. } => "indirect call type mismatch",
        }
    }
}

impl LinkingError {
    /// Get the message of the linking error
    pub fn message(&self) -> &'static str {
        match self {
            Self::UnknownImport { .. } => "unknown import",
            Self::IncompatibleImportType { .. } => "incompatible import type",
        }
    }
}

impl From<LinkingError> for Error {
    fn from(value: LinkingError) -> Self {
        Self::Linker(value)
    }
}

impl From<Trap> for Error {
    fn from(value: Trap) -> Self {
        Self::Trap(value)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "parser")]
            Self::ParseError(err) => write!(f, "error parsing module: {:?}", err),

            #[cfg(feature = "std")]
            Self::Io(err) => write!(f, "I/O error: {}", err),

            Self::Trap(trap) => write!(f, "trap: {}", trap.message()),
            Self::Linker(err) => write!(f, "linking error: {}", err.message()),
            Self::CallStackEmpty => write!(f, "call stack empty"),
            Self::InvalidLabelType => write!(f, "invalid label type"),
            Self::Other(message) => write!(f, "unknown error: {}", message),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {}", feature),
            Self::FuncDidNotReturn => write!(f, "function did not return"),
            Self::LabelStackUnderflow => write!(f, "label stack underflow"),
            Self::StackUnderflow => write!(f, "stack underflow"),
            Self::InvalidStore => write!(f, "invalid store"),
        }
    }
}

#[cfg(any(feature = "std", all(not(feature = "std"), nightly)))]
impl crate::std::error::Error for Error {}

#[cfg(feature = "parser")]
impl From<tinywasm_parser::ParseError> for Error {
    fn from(value: tinywasm_parser::ParseError) -> Self {
        Self::ParseError(value)
    }
}

/// A specialized [`Result`] type for tinywasm operations
pub type Result<T, E = Error> = crate::std::result::Result<T, E>;
