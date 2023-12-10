use alloc::string::String;
use core::fmt::Display;

#[cfg(feature = "parser")]
use tinywasm_parser::ParseError;

#[derive(Debug)]
/// A WebAssembly trap
///
/// See <https://webassembly.github.io/spec/core/intro/overview.html#trap>
pub enum Trap {
    /// An unreachable instruction was executed
    Unreachable,
}

#[derive(Debug)]
/// A tinywasm error
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

    /// A function did not return a value
    FuncDidNotReturn,

    /// The stack is empty
    StackUnderflow,

    /// The block stack is empty
    BlockStackUnderflow,

    /// The call stack is empty
    CallStackEmpty,

    /// The store is not the one that the module instance was instantiated in
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
