// Format detailed diagnostics only when the debug feature is enabled.
macro_rules! error_message {
    ($compact:literal | $format:literal $(, $arg:expr)* $(,)?) => {{
        #[cfg(feature = "debug")]
        { alloc::format!($format $(, $arg)*) }
        #[cfg(not(feature = "debug"))]
        {
            let _ = || { $(let _ = &$arg;)* };
            alloc::string::String::from($compact)
        }
    }};
}

// Keep detailed panic formatting out of compact builds, without allocating a String.
macro_rules! unreachable_debug {
    ($compact:literal | $format:literal $(, $arg:expr)* $(,)?) => {{
        #[cfg(feature = "debug")]
        { core::unreachable!($format $(, $arg)*) }
        #[cfg(not(feature = "debug"))]
        {
            let _ = || { $(let _ = &$arg;)* };
            core::unreachable!($compact)
        }
    }};
}

/// Stops on an interpreter invariant that validated code cannot break, such as a value-stack or
/// global access out of range.
///
/// With immediate abort support, builds trap in place. This avoids forcing tail-call
/// handlers to save a stack frame for a panic call. Other builds panic instead.
#[cfg(tw_abort_immediate)]
#[inline(always)]
pub(crate) fn invariant_violated() -> ! {
    core::process::crash()
}

/// Stops on an interpreter invariant that validated code cannot break, such as a value-stack or
/// global access out of range.
#[cfg(not(tw_abort_immediate))]
#[cold]
#[inline(never)]
#[track_caller]
pub(crate) fn invariant_violated() -> ! {
    unreachable!("interpreter invariant violated")
}

/// Checks internal expectations with hot-path aborts or ordinary diagnostic panics.
pub(crate) trait UnwrapExt<T> {
    /// Returns the value or stops on an internal invariant failure.
    fn unwrap_invariant(self) -> T;

    /// Returns the value or panics with the message, without formatting the missing value or error.
    #[track_caller]
    fn expect_unreachable(self, message: &'static str) -> T;
}

impl<T> UnwrapExt<T> for Option<T> {
    #[inline(always)]
    #[track_caller]
    fn unwrap_invariant(self) -> T {
        match self {
            Some(value) => value,
            None => invariant_violated(),
        }
    }

    #[inline]
    #[track_caller]
    fn expect_unreachable(self, message: &'static str) -> T {
        match self {
            Some(value) => value,
            None => unreachable!("{message}"),
        }
    }
}

impl<T, E> UnwrapExt<T> for core::result::Result<T, E> {
    #[inline(always)]
    #[track_caller]
    fn unwrap_invariant(self) -> T {
        match self {
            Ok(value) => value,
            Err(_) => invariant_violated(),
        }
    }

    #[inline]
    #[track_caller]
    fn expect_unreachable(self, message: &'static str) -> T {
        match self {
            Ok(value) => value,
            Err(_) => unreachable!("{message}"),
        }
    }
}

macro_rules! cold {
    ($value:expr) => {{
        core::hint::cold_path();
        $value
    }};
}

// Mark the caller's error path cold. This makes a significant difference in interpreter benchmarks,
// while doing it inside map_err or inspect_err is unreliable:
// https://internals.rust-lang.org/t/err-automatic-hint-cold-path/24404
macro_rules! cold_err {
    ($result:expr) => {
        match $result {
            Ok(value) => Ok(value),
            Err(error) => cold!(Err(error)),
        }
    };
}
