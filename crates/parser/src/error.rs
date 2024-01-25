use core::fmt::{Debug, Display};

use alloc::string::{String, ToString};
use wasmparser::Encoding;

#[derive(Debug)]
pub enum ParseError {
    InvalidType,
    UnsupportedSection(String),
    DuplicateSection(String),
    EmptySection(String),
    UnsupportedOperator(String),
    ParseError { message: String, offset: usize },
    InvalidEncoding(Encoding),
    InvalidLocalCount { expected: u32, actual: u32 },
    EndNotReached,
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

pub type Result<T, E = ParseError> = core::result::Result<T, E>;
