// Break to a block at the given index (relative to the current frame)
// If there is no block at the given index, return or call the parent function
//
// This is a bit hard to see from the spec, but it's vaild to use breaks to return
// from a function, so we need to check if the label stack is empty
macro_rules! break_to {
    ($break_to_relative:expr, $self:expr) => {
        if $self.cf.break_to($break_to_relative, &mut $self.stack.values, &mut $self.stack.blocks).is_none() {
            return $self.exec_return();
        }
    };
}

/// Doing the actual conversion from float to int is a bit tricky, because
/// we need to check for overflow. This macro generates the min/max values
/// for a specific conversion, which are then used in the actual conversion.
/// Rust sadly doesn't have wrapping casts for floats yet, maybe never.
/// Alternatively, https://crates.io/crates/az could be used for this but
/// it's not worth the dependency. 
#[rustfmt::skip]
macro_rules! float_min_max {
    (f32, i32) => {(-2147483904.0_f32, 2147483648.0_f32)};
    (f64, i32) => {(-2147483649.0_f64, 2147483648.0_f64)};
    (f32, u32) => {(-1.0_f32, 4294967296.0_f32)}; // 2^32
    (f64, u32) => {(-1.0_f64, 4294967296.0_f64)}; // 2^32
    (f32, i64) => {(-9223373136366403584.0_f32, 9223372036854775808.0_f32)}; // 2^63 + 2^40 | 2^63
    (f64, i64) => {(-9223372036854777856.0_f64, 9223372036854775808.0_f64)}; // 2^63 + 2^40 | 2^63
    (f32, u64) => {(-1.0_f32, 18446744073709551616.0_f32)}; // 2^64
    (f64, u64) => {(-1.0_f64, 18446744073709551616.0_f64)}; // 2^64
    // other conversions are not allowed
    ($from:ty, $to:ty) => {compile_error!("invalid float conversion")};
}

/// Convert a value on the stack
macro_rules! conv {
    ($from:ty, $to:ty, $self:expr) => {
        $self.stack.values.replace_top(|v| (<$from>::from(v) as $to).into())?
    };
}

/// Convert a value on the stack with error checking
macro_rules! checked_conv_float {
    // Direct conversion with error checking (two types)
    ($from:tt, $to:tt, $self:expr) => {
        checked_conv_float!($from, $to, $to, $self)
    };
    // Conversion with an intermediate unsigned type and error checking (three types)
    ($from:tt, $intermediate:tt, $to:tt, $self:expr) => {
        $self.stack.values.replace_top_trap(|v| {
            let (min, max) = float_min_max!($from, $intermediate);
            let a: $from = v.into();
            if unlikely(a.is_nan()) {
                return Err(Error::Trap(crate::Trap::InvalidConversionToInt));
            }
            if unlikely(a <= min || a >= max) {
                return Err(Error::Trap(crate::Trap::IntegerOverflow));
            }
            Ok((a as $intermediate as $to).into())
        })?
    };
}

/// Compare two values on the stack
macro_rules! comp {
    ($op:tt, $to:ty, $self:ident) => {
        $self.stack.values.calculate(|a, b| {
            ((<$to>::from(a) $op <$to>::from(b)) as i32).into()
        })?
    };
}

/// Compare a value on the stack to zero
macro_rules! comp_zero {
    ($op:tt, $ty:ty, $self:expr) => {
        $self.stack.values.replace_top(|v| ((<$ty>::from(v) $op 0) as i32).into())?
    };
}

/// Apply an arithmetic method to two values on the stack
macro_rules! arithmetic {
    ($op:ident, $to:ty, $self:expr) => {
        $self.stack.values.calculate(|a, b| {
            (<$to>::from(a).$op(<$to>::from(b)) as $to).into()
        })?
    };

    // also allow operators such as +, -
    ($op:tt, $ty:ty, $self:expr) => {
        $self.stack.values.calculate(|a, b| {
            ((<$ty>::from(a) $op <$ty>::from(b)) as $ty).into()
        })?
    };
}

/// Apply an arithmetic method to a single value on the stack
macro_rules! arithmetic_single {
    ($op:ident, $ty:ty, $self:expr) => {
        arithmetic_single!($op, $ty, $ty, $self)
    };

    ($op:ident, $from:ty, $to:ty, $self:expr) => {
        $self.stack.values.replace_top(|v| (<$from>::from(v).$op() as $to).into())?
    };
}

/// Apply an arithmetic operation to two values on the stack with error checking
macro_rules! checked_int_arithmetic {
    ($op:ident, $to:ty, $self:expr) => {
        $self.stack.values.calculate_trap(|a, b| {
            let a: $to = a.into();
            let b: $to = b.into();

            if unlikely(b == 0) {
                return Err(Error::Trap(crate::Trap::DivisionByZero));
            }

            let result = a.$op(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))?;
            Ok((result).into())
        })?
    };
}

pub(super) use arithmetic;
pub(super) use arithmetic_single;
pub(super) use break_to;
pub(super) use checked_conv_float;
pub(super) use checked_int_arithmetic;
pub(super) use comp;
pub(super) use comp_zero;
pub(super) use conv;
pub(super) use float_min_max;
