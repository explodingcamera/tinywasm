use alloc::string::{String, ToString};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("error parsing module")]
    ParseError { message: String, offset: usize },

    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),

    #[error("unknown error: {0}")]
    Other(String),
}

impl Error {
    pub fn other<T>(message: &str) -> Result<T, Self> {
        Err(Self::Other(message.to_string()))
    }

    pub fn unsupported<T>(feature: &str) -> Result<T, Self> {
        Err(Self::UnsupportedFeature(feature.to_string()))
    }
}

impl From<wasmparser::BinaryReaderError> for Error {
    fn from(value: wasmparser::BinaryReaderError) -> Self {
        Self::ParseError {
            message: value.message().to_string(),
            offset: value.offset(),
        }
    }
}

pub type Result<T, E = Error> = crate::std::result::Result<T, E>;
