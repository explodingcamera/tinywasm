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
    ($cf:ident, $stack:ident, $module:ident, $store:ident, $break_to_relative:ident) => {{
        if $cf.break_to(*$break_to_relative, &mut $stack.values, &mut $stack.blocks).is_none() {
            if $stack.call_stack.is_empty() {
                return Ok(());
            }

            call!($cf, $stack, $module, $store)
        }
    }};
}

/// Load a value from memory
macro_rules! mem_load {
    ($type:ty, $arg:expr, $stack:ident, $store:ident, $module:ident) => {{
        mem_load!($type, $type, $arg, $stack, $store, $module)
    }};

    ($load_type:ty, $target_type:ty, $arg:expr, $stack:ident, $store:ident, $module:ident) => {{
        #[inline(always)]
        fn mem_load_inner(
            store: &Store,
            module: &crate::ModuleInstance,
            stack: &mut crate::runtime::Stack,
            mem_addr: tinywasm_types::MemAddr,
            offset: u64,
        ) -> Result<()> {
            let mem = store.get_mem(module.resolve_mem_addr(mem_addr))?;
            let addr: usize = match offset.checked_add(stack.values.pop()?.into()).map(|a| a.try_into()) {
                Some(Ok(a)) => a,
                _ => {
                    cold();
                    return Err(Error::Trap(crate::Trap::MemoryOutOfBounds {
                        offset: offset as usize,
                        len: core::mem::size_of::<$load_type>(),
                        max: mem.borrow().max_pages(),
                    }));
                }
            };

            const LEN: usize = core::mem::size_of::<$load_type>();
            let val = mem.borrow().load_as::<LEN, $load_type>(addr)?;
            stack.values.push((val as $target_type).into());
            Ok(())
        }

        let (mem_addr, offset) = $arg;
        mem_load_inner($store, &$module, $stack, *mem_addr, *offset)?;
    }};
}

/// Store a value to memory
macro_rules! mem_store {
    ($type:ty, $arg:expr, $stack:ident, $store:ident, $module:ident) => {{
        mem_store!($type, $type, $arg, $stack, $store, $module)
    }};

    ($store_type:ty, $target_type:ty, $arg:expr, $stack:ident, $store:ident, $module:ident) => {{
        #[inline(always)]
        fn mem_store_inner(
            store: &Store,
            module: &crate::ModuleInstance,
            stack: &mut crate::runtime::Stack,
            mem_addr: tinywasm_types::MemAddr,
            offset: u64,
        ) -> Result<()> {
            let mem = store.get_mem(module.resolve_mem_addr(mem_addr))?;
            let val: $store_type = stack.values.pop()?.into();
            let val = val.to_le_bytes();
            let addr: u64 = stack.values.pop()?.into();
            mem.borrow_mut().store((offset + addr) as usize, val.len(), &val)?;
            Ok(())
        }

        let (mem_addr, offset) = $arg;
        mem_store_inner($store, &$module, $stack, *mem_addr, *offset)?;
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
    ($from:ty, $to:ty, $stack:ident) => {
        $stack.values.replace_top(|v| (<$from>::from(v) as $to).into())?
    };
}

/// Convert a value on the stack with error checking
macro_rules! checked_conv_float {
    // Direct conversion with error checking (two types)
    ($from:tt, $to:tt, $stack:ident) => {
        checked_conv_float!($from, $to, $to, $stack)
    };
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
    ($op:tt, $to:ty, $stack:ident) => {
        $stack.values.calculate(|a, b| {
            ((<$to>::from(a) $op <$to>::from(b)) as i32).into()
        })?
    };
}

/// Compare a value on the stack to zero
macro_rules! comp_zero {
    ($op:tt, $ty:ty, $stack:ident) => {
        $stack.values.replace_top(|v| {
            ((<$ty>::from(v) $op 0) as i32).into()
        })?
    };
}

/// Apply an arithmetic method to two values on the stack
macro_rules! arithmetic {
    ($op:ident, $to:ty, $stack:ident) => {
        $stack.values.calculate(|a, b| {
            (<$to>::from(a).$op(<$to>::from(b)) as $to).into()
        })?
    };

    // also allow operators such as +, -
    ($op:tt, $ty:ty, $stack:ident) => {
        $stack.values.calculate(|a, b| {
            ((<$ty>::from(a) $op <$ty>::from(b)) as $ty).into()
        })?
    };
}

/// Apply an arithmetic method to a single value on the stack
macro_rules! arithmetic_single {
    ($op:ident, $ty:ty, $stack:ident) => {
        arithmetic_single!($op, $ty, $ty, $stack)
    };

    ($op:ident, $from:ty, $to:ty, $stack:ident) => {
        $stack.values.replace_top(|v| (<$from>::from(v).$op() as $to).into())?
    };
}

/// Apply an arithmetic operation to two values on the stack with error checking
macro_rules! checked_int_arithmetic {
    ($op:ident, $to:ty, $stack:ident) => {
        $stack.values.calculate_trap(|a, b| {
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

macro_rules! call {
    ($cf:expr, $stack:expr, $module:expr, $store:expr) => {{
        let old = $cf.block_ptr;
        $cf = $stack.call_stack.pop()?;

        if old > $cf.block_ptr {
            $stack.blocks.truncate(old);
        }

        if $cf.module_addr != $module.id() {
            $module.swap_with($cf.module_addr, $store);
        }

        continue;
    }};
}

macro_rules! skip {
    ($code:expr) => {
        match $code {
            Ok(_) => continue,
            Err(e) => return Err(e),
        }
    };
}

pub(super) use arithmetic;
pub(super) use arithmetic_single;
pub(super) use break_to;
pub(super) use call;
pub(super) use checked_conv_float;
pub(super) use checked_int_arithmetic;
pub(super) use comp;
pub(super) use comp_zero;
pub(super) use conv;
pub(super) use float_min_max;
pub(super) use mem_load;
pub(super) use mem_store;
pub(super) use skip;
