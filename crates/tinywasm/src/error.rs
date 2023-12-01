use alloc::string::{String, ToString};
use core::fmt::Display;

#[derive(Debug)]
pub enum Error {
    ParseError {
        message: String,
        offset: usize,
    },
    UnsupportedFeature(String),
    Other(String),

    #[cfg(feature = "std")]
    Io(crate::std::io::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ParseError { message, offset } => {
                write!(f, "error parsing module: {} at offset {}", message, offset)
            }
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {}", feature),
            Self::Other(message) => write!(f, "unknown error: {}", message),
            #[cfg(feature = "std")]
            Self::Io(err) => write!(f, "I/O error: {}", err),
        }
    }
}

impl crate::std::error::Error for Error {}

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
