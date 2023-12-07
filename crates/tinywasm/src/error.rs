use alloc::string::{String, ToString};
use core::fmt::Display;

#[cfg(feature = "parser")]
use tinywasm_parser::ParseError;

#[derive(Debug)]
pub enum Trap {
    Unreachable,
}

#[derive(Debug)]
pub enum Error {
    #[cfg(feature = "parser")]
    ParseError(ParseError),

    #[cfg(feature = "std")]
    Io(crate::std::io::Error),

    UnsupportedFeature(String),
    Other(String),

    Trap(Trap),

    FuncDidNotReturn,
    StackUnderflow,
    BlockStackUnderflow,
    CallStackEmpty,
    InvalidStore,
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "parser")]
            Self::ParseError(err) => write!(f, "error parsing module: {:?}", err),

            #[cfg(feature = "std")]
            Self::Io(err) => write!(f, "I/O error: {}", err),

            Self::Trap(trap) => write!(f, "trap: {:?}", trap),

            Self::Other(message) => write!(f, "unknown error: {}", message),
            Self::UnsupportedFeature(feature) => write!(f, "unsupported feature: {}", feature),
            Self::FuncDidNotReturn => write!(f, "function did not return"),
            Self::BlockStackUnderflow => write!(f, "block stack underflow"),
            Self::StackUnderflow => write!(f, "stack underflow"),
            Self::CallStackEmpty => write!(f, "call stack empty"),
            Self::InvalidStore => write!(f, "invalid store"),
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

#[cfg(feature = "parser")]
impl From<tinywasm_parser::ParseError> for Error {
    fn from(value: tinywasm_parser::ParseError) -> Self {
        Self::ParseError(value)
    }
}

pub type Result<T, E = Error> = crate::std::result::Result<T, E>;
