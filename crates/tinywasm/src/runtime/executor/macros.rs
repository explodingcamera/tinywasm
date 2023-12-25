//! More generic macros for various instructions
//!
//! These macros are used to generate the actual instruction implementations.

/// Convert the top value on the stack to a specific type
macro_rules! conv_1 {
    ($from:ty, $to:ty, $stack:ident) => {{
        let a: $from = $stack.values.pop()?.into();
        $stack.values.push((a as $to).into());
    }};
}

/// Convert the unsigned value on the top of the stack to a specific type
macro_rules! conv_2 {
    ($ty:ty, $uty:ty, $to:ty, $stack:ident) => {{
        let a: $ty = $stack.values.pop()?.into();
        $stack.values.push((a as $uty as $to).into());
    }};
}

/// Compare two values on the stack
macro_rules! comp {
    ($op:tt, $ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push(((a $op b) as i32).into());
    }};
}

/// Compare two values on the stack (cast to ty2 before comparison)
macro_rules! comp_cast {
    ($op:tt, $ty:ty, $ty2:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();

        // Cast to unsigned type before comparison
        let a_unsigned: $ty2 = a as $ty2;
        let b_unsigned: $ty2 = b as $ty2;
        $stack.values.push(((a_unsigned $op b_unsigned) as i32).into());
    }};
}

/// Compare a value on the stack to zero
macro_rules! comp_zero {
    ($op:tt, $ty:ty, $stack:ident) => {{
        let a: $ty = $stack.values.pop()?.into();
        $stack.values.push(((a $op 0) as i32).into());
    }};
}

/// Apply an arithmetic operation to two values on the stack
macro_rules! arithmetic_op {
    ($op:tt, $ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push((a $op b).into());
    }};
}

macro_rules! arithmetic_method {
    ($op:ident, $ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        let result = a.$op(b);
        $stack.values.push(result.into());
    }};
}

/// Apply an arithmetic operation to two values on the stack
macro_rules! checked_arithmetic_method {
    ($op:ident, $ty:ty, $stack:ident, $trap:expr) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        let result = a.$op(b).ok_or_else(|| Error::Trap($trap))?;
        debug!(
            "checked_arithmetic_method: {}, a: {}, b: {}, res: {}",
            stringify!($op),
            a,
            b,
            result
        );
        $stack.values.push(result.into());
    }};
}

/// Apply an arithmetic operation to two values on the stack (cast to ty2 before operation)
macro_rules! checked_arithmetic_method_cast {
    ($op:ident, $ty:ty, $ty2:ty, $stack:ident, $trap:expr) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();

        // Cast to unsigned type before operation
        let a_unsigned: $ty2 = a as $ty2;
        let b_unsigned: $ty2 = b as $ty2;

        let result = a_unsigned.$op(b_unsigned).ok_or_else(|| Error::Trap($trap))?;
        $stack.values.push((result as $ty).into());
    }};
}

pub(super) use arithmetic_method;
pub(super) use arithmetic_op;
pub(super) use checked_arithmetic_method;
pub(super) use checked_arithmetic_method_cast;
pub(super) use comp;
pub(super) use comp_cast;
pub(super) use comp_zero;
pub(super) use conv_1;
pub(super) use conv_2;
