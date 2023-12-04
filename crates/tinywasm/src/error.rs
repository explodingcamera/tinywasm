use alloc::string::{String, ToString};
use core::fmt::Display;
use tinywasm_parser::ParseError;

#[derive(Debug)]
pub enum Error {
    ParseError(ParseError),
    UnsupportedFeature(String),
    Other(String),

    FuncDidNotReturn,
    StackUnderflow,

    InvalidStore,

    #[cfg(feature = "std")]
    Io(crate::std::io::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FuncDidNotReturn => write!(f, "function did not return"),
            Self::StackUnderflow => write!(f, "stack underflow"),
            Self::ParseError(err) => write!(f, "error parsing module: {:?}", err),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {}", feature),
            Self::Other(message) => write!(f, "unknown error: {}", message),
            Self::InvalidStore => write!(f, "invalid store"),
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

impl From<tinywasm_parser::ParseError> for Error {
    fn from(value: tinywasm_parser::ParseError) -> Self {
        Self::ParseError(value)
    }
}

pub type Result<T, E = Error> = crate::std::result::Result<T, E>;
