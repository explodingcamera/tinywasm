/// Stops on an interpreter invariant that validated code cannot break, such as a value-stack or
/// global access out of range.
///
/// With the nightly tail-call dispatch, release builds trap in place: a call here, even a cold one,
/// makes every instruction handler that touches the value stack save a stack frame. Debug builds
/// and the loop dispatch panic.
#[cfg(all(feature = "nightly-tail-calls", not(debug_assertions)))]
#[inline(always)]
pub(crate) fn invariant_violated(_what: &'static str) -> ! {
    core::intrinsics::abort()
}

/// Stops on an interpreter invariant that validated code cannot break, such as a value-stack or
/// global access out of range.
#[cfg(not(all(feature = "nightly-tail-calls", not(debug_assertions))))]
#[cold]
#[inline(never)]
#[track_caller]
pub(crate) fn invariant_violated(what: &'static str) -> ! {
    unreachable!("{what}, this is a bug")
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
