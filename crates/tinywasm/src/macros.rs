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

macro_rules! unwrap_or_unreachable {
    ($value:expr) => {{
        match $value {
            Some(value) => value,
            None => cold!(unreachable!()),
        }
    }};
    ($value:expr, $($arg:tt)+) => {{
        match $value {
            Some(value) => value,
            None => cold!(unreachable!($($arg)+)),
        }
    }};
}
