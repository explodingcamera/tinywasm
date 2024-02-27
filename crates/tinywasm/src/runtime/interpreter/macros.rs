//! More generic macros for various instructions
//!
//! These macros are used to generate the actual instruction implementations.
//! In some basic tests this generated better assembly than using generic functions, even when inlined.
//! (Something to revisit in the future)

// Break to a block at the given index (relative to the current frame)
// If there is no block at the given index, return or call the parent function
//
// This is a bit hard to see from the spec, but it's vaild to use breaks to return
// from a function, so we need to check if the label stack is empty
macro_rules! break_to {
    ($cf:ident, $stack:ident, $break_to_relative:ident) => {{
        if $cf.break_to($break_to_relative, &mut $stack.values, &mut $stack.blocks).is_none() {
            if $stack.call_stack.is_empty() {
                return Ok(ExecResult::Return);
            } else {
                return Ok(ExecResult::Call);
            }
        }
    }};
}

/// Load a value from memory
macro_rules! mem_load {
    ($type:ty, $arg:ident, $stack:ident, $store:ident, $module:ident) => {{
        mem_load!($type, $type, $arg, $stack, $store, $module)
    }};

    ($load_type:ty, $target_type:ty, $arg:ident, $stack:ident, $store:ident, $module:ident) => {{
        let mem_idx = $module.resolve_mem_addr($arg.mem_addr);
        let mem = $store.get_mem(mem_idx as usize)?;
        let mem_ref = mem.borrow_mut();

        let addr: u64 = $stack.values.pop()?.into();
        let addr = $arg.offset.checked_add(addr).ok_or_else(|| {
            cold();
            Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset: $arg.offset as usize,
                len: core::mem::size_of::<$load_type>(),
                max: mem_ref.max_pages(),
            })
        })?;

        let addr: usize = addr.try_into().ok().ok_or_else(|| {
            cold();
            Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset: $arg.offset as usize,
                len: core::mem::size_of::<$load_type>(),
                max: mem_ref.max_pages(),
            })
        })?;

        const LEN: usize = core::mem::size_of::<$load_type>();
        let val = mem_ref.load_as::<LEN, $load_type>(addr)?;
        $stack.values.push((val as $target_type).into());
    }};
}

/// Store a value to memory
macro_rules! mem_store {
    ($type:ty, $arg:ident, $stack:ident, $store:ident, $module:ident) => {{
        log::debug!("mem_store!({}, {:?})", stringify!($type), $arg);

        mem_store!($type, $type, $arg, $stack, $store, $module)
    }};

    ($store_type:ty, $target_type:ty, $arg:ident, $stack:ident, $store:ident, $module:ident) => {{
        // likewise, there could be a lot of performance improvements here
        let mem_idx = $module.resolve_mem_addr($arg.mem_addr);
        let mem = $store.get_mem(mem_idx as usize)?;

        let val = $stack.values.pop_t::<$store_type>()?;
        let addr: u64 = $stack.values.pop()?.into();

        let val = val as $store_type;
        let val = val.to_le_bytes();

        mem.borrow_mut().store(($arg.offset + addr) as usize, val.len(), &val)?;
    }};
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
    ($from:ty, $intermediate:ty, $to:ty, $stack:ident) => {{
        let a = $stack.values.pop_t::<$from>()? as $intermediate;
        $stack.values.push((a as $to).into());
    }};
    ($from:ty, $to:ty, $stack:ident) => {{
        let a = $stack.values.pop_t::<$from>()?;
        $stack.values.push((a as $to).into());
    }};
}

/// Convert a value on the stack with error checking
macro_rules! checked_conv_float {
    // Direct conversion with error checking (two types)
    ($from:tt, $to:tt, $stack:ident) => {{
        checked_conv_float!($from, $to, $to, $stack)
    }};
    // Conversion with an intermediate unsigned type and error checking (three types)
    ($from:tt, $intermediate:tt, $to:tt, $stack:ident) => {{
        let (min, max) = float_min_max!($from, $intermediate);
        let a: $from = $stack.values.pop()?.into();

        if unlikely(a.is_nan()) {
            return Err(Error::Trap(crate::Trap::InvalidConversionToInt));
        }

        if unlikely(a <= min || a >= max) {
            return Err(Error::Trap(crate::Trap::IntegerOverflow));
        }

        $stack.values.push((a as $intermediate as $to).into());
    }};
}

/// Compare two values on the stack
macro_rules! comp {
    ($op:tt, $ty:ty, $stack:ident) => {{
        comp!($op, $ty, $ty, $stack)
    }};

    ($op:tt, $intermediate:ty, $to:ty, $stack:ident) => {{
        let b = $stack.values.pop_t::<$intermediate>()? as $to;
        let a = $stack.values.pop_t::<$intermediate>()? as $to;
        $stack.values.push(((a $op b) as i32).into());
    }};
}

/// Compare a value on the stack to zero
macro_rules! comp_zero {
    ($op:tt, $ty:ty, $stack:ident) => {{
        let a = $stack.values.pop_t::<$ty>()?;
        $stack.values.push(((a $op 0) as i32).into());
    }};
}

/// Apply an arithmetic method to two values on the stack
macro_rules! arithmetic {
    ($op:ident, $ty:ty, $stack:ident) => {
        arithmetic!($op, $ty, $ty, $stack)
    };

    // also allow operators such as +, -
    ($op:tt, $ty:ty, $stack:ident) => {{
        let b: $ty = $stack.values.pop_t()?;
        let a: $ty = $stack.values.pop_t()?;
        $stack.values.push((a $op b).into());
    }};

    ($op:ident, $intermediate:ty, $to:ty, $stack:ident) => {{
        let b = $stack.values.pop_t::<$to>()? as $intermediate;
        let a = $stack.values.pop_t::<$to>()? as $intermediate;
        $stack.values.push((a.$op(b) as $to).into());
    }};
}

/// Apply an arithmetic method to a single value on the stack
macro_rules! arithmetic_single {
    ($op:ident, $ty:ty, $stack:ident) => {{
        let a = $stack.values.pop_t::<$ty>()?;
        $stack.values.push((a.$op() as $ty).into());
    }};

    ($op:ident, $from:ty, $to:ty, $stack:ident) => {{
        let a = $stack.values.pop_t::<$from>()?;
        $stack.values.push((a.$op() as $to).into());
    }};
}

/// Apply an arithmetic operation to two values on the stack with error checking
macro_rules! checked_int_arithmetic {
    // Direct conversion with error checking (two types)
    ($from:tt, $to:tt, $stack:ident) => {{
        checked_int_arithmetic!($from, $to, $to, $stack)
    }};

    ($op:ident, $from:ty, $to:ty, $stack:ident) => {{
        let b = $stack.values.pop_t::<$from>()? as $to;
        let a = $stack.values.pop_t::<$from>()? as $to;

        if b == 0 {
            return Err(Error::Trap(crate::Trap::DivisionByZero));
        }

        let result = a.$op(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))?;
        // Cast back to original type if different
        $stack.values.push((result as $from).into());
    }};
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
pub(super) use mem_load;
pub(super) use mem_store;
