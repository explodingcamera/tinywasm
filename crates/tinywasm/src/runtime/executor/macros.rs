//! More generic macros for various instructions
//!
//! These macros are used to generate the actual instruction implementations.
//! In some basic tests this generated better assembly than using generic functions, even when inlined.
//! (Something to revisit in the future)

/// Load a value from memory
macro_rules! mem_load {
    ($type:ty, $arg:ident, $stack:ident, $store:ident, $module:ident) => {{
        mem_load!($type, $type, $arg, $stack, $store, $module)
    }};

    ($load_type:ty, $target_type:ty, $arg:ident, $stack:ident, $store:ident, $module:ident) => {{
        // TODO: there could be a lot of performance improvements here
        let mem_idx = $module.resolve_mem_addr($arg.mem_addr);
        let mem = $store.get_mem(mem_idx as usize)?;

        let addr = $stack.values.pop()?.raw_value();

        let val: [u8; core::mem::size_of::<$load_type>()] = {
            let mem = mem.borrow_mut();
            let val = mem.load(
                ($arg.offset + addr) as usize,
                $arg.align as usize,
                core::mem::size_of::<$load_type>(),
            )?;
            val.try_into().expect("slice with incorrect length")
        };

        let loaded_value = <$load_type>::from_le_bytes(val);
        $stack.values.push((loaded_value as $target_type).into());
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
        let addr = $stack.values.pop()?.raw_value();

        let val = val as $store_type;
        let val = val.to_le_bytes();

        mem.borrow_mut()
            .store(($arg.offset + addr) as usize, $arg.align as usize, &val)?;
    }};
}

/// Doing the actual conversion from float to int is a bit tricky, because
/// we need to check for overflow. This macro generates the min/max values
/// for a specific conversion, which are then used in the actual conversion.
/// Rust sadly doesn't have wrapping casts for floats (yet)
macro_rules! float_min_max {
    (f32, i32) => {
        (-2147483904.0_f32, 2147483648.0_f32)
    };
    (f64, i32) => {
        (-2147483649.0_f64, 2147483648.0_f64)
    };
    (f32, u32) => {
        (-1.0_f32, 4294967296.0_f32) // 2^32
    };
    (f64, u32) => {
        (-1.0_f64, 4294967296.0_f64) // 2^32
    };
    (f32, i64) => {
        (-9223373136366403584.0_f32, 9223372036854775808.0_f32) // 2^63 + 2^40 | 2^63
    };
    (f64, i64) => {
        (-9223372036854777856.0_f64, 9223372036854775808.0_f64) // 2^63 + 2^40 | 2^63
    };
    (f32, u64) => {
        (-1.0_f32, 18446744073709551616.0_f32) // 2^64
    };
    (f64, u64) => {
        (-1.0_f64, 18446744073709551616.0_f64) // 2^64
    };
    // other conversions are not allowed
    ($from:ty, $to:ty) => {
        compile_error!("invalid float conversion");
    };
}

/// Convert a value on the stack
macro_rules! conv {
    ($from:ty, $intermediate:ty, $to:ty, $stack:ident) => {{
        let a: $from = $stack.values.pop()?.into();
        $stack.values.push((a as $intermediate as $to).into());
    }};
    ($from:ty, $to:ty, $stack:ident) => {{
        let a: $from = $stack.values.pop()?.into();
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

        if a.is_nan() {
            return Err(Error::Trap(crate::Trap::InvalidConversionToInt));
        }

        if a <= min || a >= max {
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
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $intermediate = a.into();
        let b: $intermediate = b.into();

        // Cast to unsigned type before comparison
        let a = a as $to;
        let b = b as $to;
        $stack.values.push(((a $op b) as i32).into());
    }};
}

/// Compare a value on the stack to zero
macro_rules! comp_zero {
    ($op:tt, $ty:ty, $stack:ident) => {{
        let a: $ty = $stack.values.pop()?.into();
        $stack.values.push(((a $op 0) as i32).into());
    }};
}

/// Apply an arithmetic method to two values on the stack
macro_rules! arithmetic {
    ($op:ident, $ty:ty, $stack:ident) => {{
        arithmetic!($op, $ty, $ty, $stack)
    }};

    // also allow operators such as +, -
    ($op:tt, $ty:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $ty = a.into();
        let b: $ty = b.into();
        $stack.values.push((a $op b).into());
    }};

    ($op:ident, $intermediate:ty, $to:ty, $stack:ident) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $to = a.into();
        let b: $to = b.into();

        let a = a as $intermediate;
        let b = b as $intermediate;

        let result = a.$op(b);
        $stack.values.push((result as $to).into());
    }};
}

/// Apply an arithmetic method to a single value on the stack
macro_rules! arithmetic_single {
    ($op:ident, $ty:ty, $stack:ident) => {{
        let a: $ty = $stack.values.pop()?.into();
        let result = a.$op();
        $stack.values.push((result as $ty).into());
    }};
}

/// Apply an arithmetic operation to two values on the stack with error checking
macro_rules! checked_arithmetic {
    // Direct conversion with error checking (two types)
    ($from:tt, $to:tt, $stack:ident, $trap:expr) => {{
        checked_arithmetic!($from, $to, $to, $stack, $trap)
    }};

    ($op:ident, $from:ty, $to:ty, $stack:ident, $trap:expr) => {{
        let [a, b] = $stack.values.pop_n_const::<2>()?;
        let a: $from = a.into();
        let b: $from = b.into();

        let a_casted: $to = a as $to;
        let b_casted: $to = b as $to;

        let result = a_casted.$op(b_casted).ok_or_else(|| Error::Trap($trap))?;

        // Cast back to original type if different
        $stack.values.push((result as $from).into());
    }};
}

pub(super) use arithmetic;
pub(super) use arithmetic_single;
pub(super) use checked_arithmetic;
pub(super) use checked_conv_float;
pub(super) use comp;
pub(super) use comp_zero;
pub(super) use conv;
pub(super) use float_min_max;
pub(super) use mem_load;
pub(super) use mem_store;
