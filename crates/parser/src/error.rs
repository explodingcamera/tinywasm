use alloc::string::{String, ToString};
use wasmparser::Encoding;

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

impl From<wasmparser::BinaryReaderError> for ParseError {
    fn from(value: wasmparser::BinaryReaderError) -> Self {
        Self::ParseError {
            message: value.message().to_string(),
            offset: value.offset(),
        }
    }
}

pub type Result<T, E = ParseError> = core::result::Result<T, E>;
