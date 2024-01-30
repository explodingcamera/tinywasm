use core::fmt::{Debug, Display};

use alloc::string::{String, ToString};
use wasmparser::Encoding;

#[derive(Debug)]
/// Errors that can occur when parsing a WebAssembly module
pub enum ParseError {
    /// An invalid type was encountered
    InvalidType,
    /// An unsupported section was encountered
    UnsupportedSection(String),
    /// A duplicate section was encountered
    DuplicateSection(String),
    /// An empty section was encountered
    EmptySection(String),
    /// An unsupported operator was encountered
    UnsupportedOperator(String),
    /// An error occurred while parsing the module
    ParseError {
        /// The error message
        message: String,
        /// The offset in the module where the error occurred
        offset: usize,
    },
    /// An invalid encoding was encountered
    InvalidEncoding(Encoding),
    /// An invalid local count was encountered
    InvalidLocalCount {
        /// The expected local count
        expected: u32,
        /// The actual local count
        actual: u32,
    },
    /// The end of the module was not reached
    EndNotReached,
    /// An unknown error occurred
    Other(String),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidType => write!(f, "invalid type"),
            Self::UnsupportedSection(section) => write!(f, "unsupported section: {}", section),
            Self::DuplicateSection(section) => write!(f, "duplicate section: {}", section),
            Self::EmptySection(section) => write!(f, "empty section: {}", section),
            Self::UnsupportedOperator(operator) => write!(f, "unsupported operator: {}", operator),
            Self::ParseError { message, offset } => {
                write!(f, "error parsing module: {} at offset {}", message, offset)
            }
            Self::InvalidEncoding(encoding) => write!(f, "invalid encoding: {:?}", encoding),
            Self::InvalidLocalCount { expected, actual } => {
                write!(f, "invalid local count: expected {}, actual {}", expected, actual)
            }
            Self::EndNotReached => write!(f, "end of module not reached"),
            Self::Other(message) => write!(f, "unknown error: {}", message),
        }
    }
}

#[cfg(any(feature = "std", all(not(feature = "std"), nightly)))]
impl crate::std::error::Error for ParseError {}

impl From<wasmparser::BinaryReaderError> for ParseError {
    fn from(value: wasmparser::BinaryReaderError) -> Self {
        Self::ParseError { message: value.message().to_string(), offset: value.offset() }
    }
}

pub(crate) type Result<T, E = ParseError> = core::result::Result<T, E>;
