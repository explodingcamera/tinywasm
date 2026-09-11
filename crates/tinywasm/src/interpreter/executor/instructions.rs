use super::*;

/// Converts raw `f32` bits to signed `i32` bits with Wasm traps.
pub(super) fn acc_trunc_f32_s(value: u32) -> core::result::Result<u32, Trap> {
    let value = f32::from_bits(value);
    let (min, max) = float_min_max!(f32, i32);
    if value.is_nan() {
        return Err(Trap::InvalidConversionToInt);
    }
    if value <= min || value >= max {
        return Err(Trap::IntegerOverflow);
    }
    Ok((value as i32) as u32)
}

/// Converts raw `f32` bits to unsigned `i32` bits with Wasm traps.
pub(super) fn acc_trunc_f32_u(value: u32) -> core::result::Result<u32, Trap> {
    let value = f32::from_bits(value);
    let (min, max) = float_min_max!(f32, u32);
    if value.is_nan() {
        return Err(Trap::InvalidConversionToInt);
    }
    if value <= min || value >= max {
        return Err(Trap::IntegerOverflow);
    }
    Ok(value as u32)
}

/// Truncates an `f64` accumulator value to a signed `i64`.
pub(super) fn acc_trunc_f64_s(value: u64) -> core::result::Result<u64, Trap> {
    let value = f64::from_bits(value);
    let (min, max) = float_min_max!(f64, i64);
    if value.is_nan() {
        return Err(Trap::InvalidConversionToInt);
    }
    if value <= min || value >= max {
        return Err(Trap::IntegerOverflow);
    }
    Ok((value as i64) as u64)
}

/// Truncates an `f64` accumulator value to an unsigned `i64`.
pub(super) fn acc_trunc_f64_u(value: u64) -> core::result::Result<u64, Trap> {
    let value = f64::from_bits(value);
    let (min, max) = float_min_max!(f64, u64);
    if value.is_nan() {
        return Err(Trap::InvalidConversionToInt);
    }
    if value <= min || value >= max {
        return Err(Trap::IntegerOverflow);
    }
    Ok(value as u64)
}

/// Applies a compact 32-bit unary accumulator operation.
pub(super) fn exec_unary32(op: UnaryOp32, value: u32) -> Result<u32, Trap> {
    Ok(match op {
        UnaryOp32::I32Eqz => u32::from(value == 0),
        UnaryOp32::I32Clz => value.leading_zeros(),
        UnaryOp32::I32Ctz => value.trailing_zeros(),
        UnaryOp32::I32Popcnt => value.count_ones(),
        UnaryOp32::I32Extend8S => i32::from(value as i8) as u32,
        UnaryOp32::I32Extend16S => i32::from(value as i16) as u32,
        UnaryOp32::I32TruncF32S => acc_trunc_f32_s(value)?,
        UnaryOp32::I32TruncF32U => acc_trunc_f32_u(value)?,
        UnaryOp32::I32TruncSatF32S => f32::from_bits(value).trunc() as i32 as u32,
        UnaryOp32::I32TruncSatF32U => f32::from_bits(value).trunc() as u32,
        UnaryOp32::F32ConvertI32S => (value as i32 as f32).to_bits(),
        UnaryOp32::F32ConvertI32U => (value as f32).to_bits(),
        UnaryOp32::F32Abs => f32::from_bits(value).abs().to_bits(),
        UnaryOp32::F32Neg => (-f32::from_bits(value)).to_bits(),
        UnaryOp32::F32Ceil => f32::from_bits(value).ceil().to_bits(),
        UnaryOp32::F32Floor => f32::from_bits(value).floor().to_bits(),
        UnaryOp32::F32Trunc => f32::from_bits(value).trunc().to_bits(),
        UnaryOp32::F32Nearest => f32::from_bits(value).tw_nearest().to_bits(),
        UnaryOp32::F32Sqrt => f32::from_bits(value).sqrt().to_bits(),
    })
}

/// Applies a compact 64-bit unary accumulator operation.
pub(super) fn exec_unary64(op: UnaryOp64, value: u64) -> Result<u64, Trap> {
    Ok(match op {
        UnaryOp64::I64Clz => u64::from(value.leading_zeros()),
        UnaryOp64::I64Ctz => u64::from(value.trailing_zeros()),
        UnaryOp64::I64Popcnt => u64::from(value.count_ones()),
        UnaryOp64::I64Extend8S => i64::from(value as i8) as u64,
        UnaryOp64::I64Extend16S => i64::from(value as i16) as u64,
        UnaryOp64::I64Extend32S => i64::from(value as i32) as u64,
        UnaryOp64::I64TruncF64S => acc_trunc_f64_s(value)?,
        UnaryOp64::I64TruncF64U => acc_trunc_f64_u(value)?,
        UnaryOp64::I64TruncSatF64S => f64::from_bits(value).trunc() as i64 as u64,
        UnaryOp64::I64TruncSatF64U => f64::from_bits(value).trunc() as u64,
        UnaryOp64::F64ConvertI64S => (value as i64 as f64).to_bits(),
        UnaryOp64::F64ConvertI64U => (value as f64).to_bits(),
        UnaryOp64::F64Abs => f64::from_bits(value).abs().to_bits(),
        UnaryOp64::F64Neg => (-f64::from_bits(value)).to_bits(),
        UnaryOp64::F64Ceil => f64::from_bits(value).ceil().to_bits(),
        UnaryOp64::F64Floor => f64::from_bits(value).floor().to_bits(),
        UnaryOp64::F64Trunc => f64::from_bits(value).trunc().to_bits(),
        UnaryOp64::F64Nearest => f64::from_bits(value).tw_nearest().to_bits(),
        UnaryOp64::F64Sqrt => f64::from_bits(value).sqrt().to_bits(),
    })
}

fn trunc_f64_to_i32(value: f64, unsigned: bool) -> Result<u32, Trap> {
    if value.is_nan() {
        return Err(Trap::InvalidConversionToInt);
    }
    if unsigned {
        let (min, max) = float_min_max!(f64, u32);
        if value <= min || value >= max {
            return Err(Trap::IntegerOverflow);
        }
        Ok(value as u32)
    } else {
        let (min, max) = float_min_max!(f64, i32);
        if value <= min || value >= max {
            return Err(Trap::IntegerOverflow);
        }
        Ok((value as i32) as u32)
    }
}

fn trunc_f32_to_i64(value: f32, unsigned: bool) -> Result<u64, Trap> {
    if value.is_nan() {
        return Err(Trap::InvalidConversionToInt);
    }
    if unsigned {
        let (min, max) = float_min_max!(f32, u64);
        if value <= min || value >= max {
            return Err(Trap::IntegerOverflow);
        }
        Ok(value as u64)
    } else {
        let (min, max) = float_min_max!(f32, i64);
        if value <= min || value >= max {
            return Err(Trap::IntegerOverflow);
        }
        Ok((value as i64) as u64)
    }
}

/// Applies a compact conversion from a 32-bit stack value to `acc64`.
pub(super) fn convert_stack32_to_acc64(op: ConvertOp32To64, value: u32) -> Result<u64, Trap> {
    Ok(match op {
        ConvertOp32To64::I64ExtendI32S => i64::from(value as i32) as u64,
        ConvertOp32To64::I64ExtendI32U => u64::from(value),
        ConvertOp32To64::I64TruncF32S => trunc_f32_to_i64(f32::from_bits(value), false)?,
        ConvertOp32To64::I64TruncF32U => trunc_f32_to_i64(f32::from_bits(value), true)?,
        ConvertOp32To64::F64ConvertI32S => (value as i32 as f64).to_bits(),
        ConvertOp32To64::F64ConvertI32U => (value as f64).to_bits(),
        ConvertOp32To64::F64PromoteF32 => (f32::from_bits(value) as f64).to_bits(),
        ConvertOp32To64::I64TruncSatF32S => f32::from_bits(value).trunc() as i64 as u64,
        ConvertOp32To64::I64TruncSatF32U => f32::from_bits(value).trunc() as u64,
    })
}

/// Applies a compact conversion from a 64-bit stack value to `acc32`.
pub(super) fn convert_stack64_to_acc32(op: ConvertOp64To32, value: u64) -> Result<u32, Trap> {
    Ok(match op {
        ConvertOp64To32::I64Eqz => u32::from(value == 0),
        ConvertOp64To32::I32WrapI64 => value as u32,
        ConvertOp64To32::I32TruncF64S => trunc_f64_to_i32(f64::from_bits(value), false)?,
        ConvertOp64To32::I32TruncF64U => trunc_f64_to_i32(f64::from_bits(value), true)?,
        ConvertOp64To32::F32ConvertI64S => (value as i64 as f32).to_bits(),
        ConvertOp64To32::F32ConvertI64U => (value as f32).to_bits(),
        ConvertOp64To32::F32DemoteF64 => (f64::from_bits(value) as f32).to_bits(),
        ConvertOp64To32::I32TruncSatF64S => f64::from_bits(value).trunc() as i32 as u32,
        ConvertOp64To32::I32TruncSatF64U => f64::from_bits(value).trunc() as u32,
    })
}

/// Applies a checked 32-bit integer operation to stack operands.
pub(super) fn exec_int_binop32(op: IntBinOp, lhs: u32, rhs: u32) -> Result<u32, Trap> {
    match op {
        IntBinOp::DivS => Ok((lhs as i32).tw_checked_div(rhs as i32)? as u32),
        IntBinOp::DivU => lhs.checked_div(rhs).ok_or(Trap::DivisionByZero),
        IntBinOp::RemS => Ok((lhs as i32).tw_checked_wrapping_rem(rhs as i32)? as u32),
        IntBinOp::RemU => lhs.tw_checked_wrapping_rem(rhs),
    }
}

/// Applies a checked 64-bit integer operation to stack operands.
pub(super) fn exec_int_binop64(op: IntBinOp, lhs: u64, rhs: u64) -> Result<u64, Trap> {
    match op {
        IntBinOp::DivS => Ok((lhs as i64).tw_checked_div(rhs as i64)? as u64),
        IntBinOp::DivU => lhs.checked_div(rhs).ok_or(Trap::DivisionByZero),
        IntBinOp::RemS => Ok((lhs as i64).tw_checked_wrapping_rem(rhs as i64)? as u64),
        IntBinOp::RemU => lhs.tw_checked_wrapping_rem(rhs),
    }
}

macro_rules! exec_op {
    ($executor:ident; accumulator $dst:ident = unary $src:ident, |$value:ident| $expr:expr) => {{
        let $value = $src;
        $dst = $expr;
    }};
    ($executor:ident; accumulator $dst:ident = unary_fallible $src:ident, |$value:ident| $expr:expr) => {{
        let $value = $src;
        $dst = $expr?;
    }};
    ($executor:ident; accumulator $dst:ident = binary $lhs_source:expr, $rhs_source:expr, |$lhs:ident, $rhs:ident| $expr:expr) => {{
        let $lhs = $lhs_source;
        let $rhs = $rhs_source;
        $dst = $expr;
    }};
    ($executor:ident; accumulator $dst:ident = binary $lhs_source:expr, $rhs_source:expr,
        #[$inline:meta] $operation:expr, $operation_ty:ty, |$op:ident, $lhs:ident, $rhs:ident| $expr:expr
    ) => {{
        #[$inline]
        fn exec_acc_binary($op: $operation_ty, $lhs: u64, $rhs: u64) -> u64 {
            $expr
        }
        $dst = exec_acc_binary($operation, $lhs_source as u64, $rhs_source as u64) as _;
    }};
    ($executor:ident; accumulator $dst:ident = stack_binary $ty:ty,
        #[$inline:meta] $operation:expr, $operation_ty:ty, |$op:ident, $lhs:ident, $rhs:ident| $expr:expr
    ) => {{
        #[$inline]
        fn exec_acc_binary($op: $operation_ty, $lhs: u64, $rhs: u64) -> u64 {
            $expr
        }
        let rhs = <$ty>::stack_pop(&mut $executor.store.value_stack) as u64;
        let lhs = <$ty>::stack_pop(&mut $executor.store.value_stack) as u64;
        $dst = exec_acc_binary($operation, lhs, rhs) as _;
    }};
    ($executor:ident; binary_fallible $ty:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
        fn exec_binary_fallible(value_stack: &mut ValueStack) -> Result<(), Trap> {
            let $rhs = <$ty>::stack_pop(value_stack);
            let $lhs = <$ty>::stack_pop(value_stack);
            <$ty>::stack_push(value_stack, $expr?)
        }
        exec_binary_fallible(&mut $executor.store.value_stack)?;
    }};
    ($executor:ident; unary $from:ty => $to:ty, |$v:ident| $expr:expr) => {{
        fn exec_unary(value_stack: &mut ValueStack) -> Result<(), Trap> {
            let $v = <$from>::stack_pop(value_stack);
            <$to>::stack_push(value_stack, $expr)
        }
        exec_unary(&mut $executor.store.value_stack)?;
    }};
    ($executor:ident; binary $from:ty => $to:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
        exec_op!($executor; binary $from, $from => $to, |$lhs, $rhs| $expr)
    }};
    ($executor:ident; binary $lhs_ty:ty, $rhs_ty:ty => $res:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
        fn exec_binary(value_stack: &mut ValueStack) -> Result<(), Trap> {
            let $rhs = <$rhs_ty>::stack_pop(value_stack);
            let $lhs = <$lhs_ty>::stack_pop(value_stack);
            <$res>::stack_push(value_stack, $expr)
        }
        exec_binary(&mut $executor.store.value_stack)?;
    }};
    ($executor:ident; ternary $from:ty => $to:ty, |$a:ident, $b:ident, $c:ident| $expr:expr) => {{
        fn exec_ternary(value_stack: &mut ValueStack) -> Result<(), Trap> {
            let $c = <$from>::stack_pop(value_stack);
            let $b = <$from>::stack_pop(value_stack);
            let $a = <$from>::stack_pop(value_stack);
            <$to>::stack_push(value_stack, $expr)
        }
        exec_ternary(&mut $executor.store.value_stack)?;
    }};
}

// This list is the single source for stable instruction stepping and nightly tail handlers.
#[rustfmt::skip]
macro_rules! instruction_handlers {
    ($emit:ident) => {
        $emit! { executor, instr_ptr, acc32, acc64, acc_ref, dispatch_next, dispatch_flow;
            AccConst32(value) => acc32 = *value as u32,
            AccLocalGet32(local) => acc32 = Value32::local_get(&executor.store.value_stack, &executor.cf, *local),
            AccLocalGetPush32(local) => {
                acc32 = Value32::local_get(&executor.store.value_stack, &executor.cf, *local);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccLocalGetPush32Push64(local) => {
                acc32 = Value32::local_get(&executor.store.value_stack, &executor.cf, *local);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            PushAcc32 => Value32::stack_push(&mut executor.store.value_stack, acc32)?,
            AccUnaryStack32(op) => acc32 = instructions::exec_unary32(*op, Value32::stack_pop(&mut executor.store.value_stack))?,
            AccUnaryStackPush32(op) => {
                acc32 = instructions::exec_unary32(*op, Value32::stack_pop(&mut executor.store.value_stack))?;
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccMemorySize32(memory) => acc32 = executor.memory_size(*memory) as u32,
            AccTableSize32(table) => acc32 = executor.table_size(*table) as u32,
            AccI32Eqz => exec_op!(executor; accumulator acc32 = unary acc32, |v| u32::from(v == 0)),
            AccI32Clz => exec_op!(executor; accumulator acc32 = unary acc32, |v| v.leading_zeros()),
            AccI32Ctz => exec_op!(executor; accumulator acc32 = unary acc32, |v| v.trailing_zeros()),
            AccI32Popcnt => exec_op!(executor; accumulator acc32 = unary acc32, |v| v.count_ones()),
            AccI32Extend8S => exec_op!(executor; accumulator acc32 = unary acc32, |v| i32::from(v as i8) as u32),
            AccI32Extend16S => exec_op!(executor; accumulator acc32 = unary acc32, |v| i32::from(v as i16) as u32),
            AccI32TruncF32S => exec_op!(executor; accumulator acc32 = unary_fallible acc32, |v| instructions::acc_trunc_f32_s(v)),
            AccI32TruncF32U => exec_op!(executor; accumulator acc32 = unary_fallible acc32, |v| instructions::acc_trunc_f32_u(v)),
            AccI32TruncSatF32S => exec_op!(executor; accumulator acc32 = unary acc32, |v| f32::from_bits(v).trunc() as i32 as u32),
            AccI32TruncSatF32U => exec_op!(executor; accumulator acc32 = unary acc32, |v| f32::from_bits(v).trunc() as u32),
            AccF32ConvertI32S => exec_op!(executor; accumulator acc32 = unary acc32, |v| (v as i32 as f32).to_bits()),
            AccF32ConvertI32U => exec_op!(executor; accumulator acc32 = unary acc32, |v| (v as f32).to_bits()),
            AccF32Abs => exec_op!(executor; accumulator acc32 = unary acc32, |v| f32::from_bits(v).abs().to_bits()),
            AccF32Neg => exec_op!(executor; accumulator acc32 = unary acc32, |v| (-f32::from_bits(v)).to_bits()),
            AccF32Ceil => exec_op!(executor; accumulator acc32 = unary acc32, |v| f32::from_bits(v).ceil().to_bits()),
            AccF32Floor => exec_op!(executor; accumulator acc32 = unary acc32, |v| f32::from_bits(v).floor().to_bits()),
            AccF32Trunc => exec_op!(executor; accumulator acc32 = unary acc32, |v| f32::from_bits(v).trunc().to_bits()),
            AccF32Nearest => exec_op!(executor; accumulator acc32 = unary acc32, |v| f32::from_bits(v).tw_nearest().to_bits()),
            AccF32Sqrt => exec_op!(executor; accumulator acc32 = unary acc32, |v| f32::from_bits(v).sqrt().to_bits()),
            AccLoad32(index) => acc32 = executor.exec_acc_load::<u32, 4>(index.resolve(&executor.func.data), acc32, identity)?,
            AccLoadTee32(arg) => {
                let (memory_arg_idx, local) = (arg.memory_arg_idx, arg.local);
                acc32 = executor.exec_acc_load::<u32, 4>(memory_arg_idx.resolve(&executor.func.data), acc32, identity)?;
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, local, acc32);
            },
            AccLoadTeePush32(arg) => {
                let (memory_arg_idx, local) = (arg.memory_arg_idx, arg.local);
                acc32 = executor.exec_acc_load::<u32, 4>(memory_arg_idx.resolve(&executor.func.data), acc32, identity)?;
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, local, acc32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccLoad8S32(index) => acc32 = executor.exec_acc_load::<i8, 1>(index.resolve(&executor.func.data), acc32, |v| i32::from(v) as u32)?,
            AccLoad8U32(index) => acc32 = executor.exec_acc_load::<u8, 1>(index.resolve(&executor.func.data), acc32, u32::from)?,
            AccLoad16S32(index) => acc32 = executor.exec_acc_load::<i16, 2>(index.resolve(&executor.func.data), acc32, |v| i32::from(v) as u32)?,
            AccLoad16U32(index) => acc32 = executor.exec_acc_load::<u16, 2>(index.resolve(&executor.func.data), acc32, u32::from)?,
            AccLoadStack32(packed) => {
                let memory = packed.index.resolve(&executor.func.data);
                match packed.op {
                    LoadOp32::Full => acc32 = executor.exec_acc32_load_stack::<u32, 4>(memory, identity)?,
                    LoadOp32::I8S => acc32 = executor.exec_acc32_load_stack::<i8, 1>(memory, |v| i32::from(v) as u32)?,
                    LoadOp32::I8U => acc32 = executor.exec_acc32_load_stack::<u8, 1>(memory, u32::from)?,
                    LoadOp32::I16S => acc32 = executor.exec_acc32_load_stack::<i16, 2>(memory, |v| i32::from(v) as u32)?,
                    LoadOp32::I16U => acc32 = executor.exec_acc32_load_stack::<u16, 2>(memory, u32::from)?,
                }
            },
            AccLoadPush32(packed) => {
                let memory = packed.index.resolve(&executor.func.data);
                acc32 = match packed.op {
                    LoadOp32::Full => executor.exec_acc_load::<u32, 4>(memory, acc32, identity)?,
                    LoadOp32::I8S => executor.exec_acc_load::<i8, 1>(memory, acc32, |v| i32::from(v) as u32)?,
                    LoadOp32::I8U => executor.exec_acc_load::<u8, 1>(memory, acc32, u32::from)?,
                    LoadOp32::I16S => executor.exec_acc_load::<i16, 2>(memory, acc32, |v| i32::from(v) as u32)?,
                    LoadOp32::I16U => executor.exec_acc_load::<u16, 2>(memory, acc32, u32::from)?,
                };
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccLoadStackPush32(packed) => {
                let memory = packed.index.resolve(&executor.func.data);
                acc32 = match packed.op {
                    LoadOp32::Full => executor.exec_acc32_load_stack::<u32, 4>(memory, identity)?,
                    LoadOp32::I8S => executor.exec_acc32_load_stack::<i8, 1>(memory, |v| i32::from(v) as u32)?,
                    LoadOp32::I8U => executor.exec_acc32_load_stack::<u8, 1>(memory, u32::from)?,
                    LoadOp32::I16S => executor.exec_acc32_load_stack::<i16, 2>(memory, |v| i32::from(v) as u32)?,
                    LoadOp32::I16U => executor.exec_acc32_load_stack::<u16, 2>(memory, u32::from)?,
                };
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccStore32(index) => executor.exec_mem_store_value(
                executor.mem_addr(index.resolve(&executor.func.data).memory()),
                index.resolve(&executor.func.data).offset(),
                acc32,
            )?,
            AccStore8_32(index) => executor.exec_mem_store_value(
                executor.mem_addr(index.resolve(&executor.func.data).memory()),
                index.resolve(&executor.func.data).offset(),
                acc32 as u8,
            )?,
            AccStore16_32(index) => executor.exec_mem_store_value(
                executor.mem_addr(index.resolve(&executor.func.data).memory()),
                index.resolve(&executor.func.data).offset(),
                acc32 as u16,
            )?,
            AccI32AddStack => exec_op!(executor; accumulator acc32 = binary Value32::stack_pop(&mut executor.store.value_stack), acc32, |a, b| a.wrapping_add(b)),
            AccI32AddLocal(local) => exec_op!(executor; accumulator acc32 = binary acc32, Value32::local_get(&executor.store.value_stack, &executor.cf, *local), |a, b| a.wrapping_add(b)),
            AccI32AddConst(value) => exec_op!(executor; accumulator acc32 = binary acc32, *value as u32, |a, b| a.wrapping_add(b)),
            AccI32AddLocalConst(arg) => exec_op!(executor; accumulator acc32 = binary
                Value32::local_get(&executor.store.value_stack, &executor.cf, arg.local), arg.value as u32,
                |a, b| a.wrapping_add(b)),
            AccI32AddLocalConstPush(arg) => {
                acc32 = Value32::local_get(&executor.store.value_stack, &executor.cf, arg.local)
                    .wrapping_add(arg.value as u32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccI32AddLocalConstPush32Push64(arg) => {
                acc32 = Value32::local_get(&executor.store.value_stack, &executor.cf, arg.local)
                    .wrapping_add(arg.value as u32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccI32AddLocalConstTee(index) => {
                let operand = index.resolve(&executor.func.data);
                exec_op!(executor; accumulator acc32 = binary
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a()), operand.c(),
                    |a, b| a.wrapping_add(b));
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.b(), acc32);
            },
            AccI32AddLocalConstTeePush(index) => {
                let operand = index.resolve(&executor.func.data);
                acc32 = Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a())
                    .wrapping_add(operand.c());
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.b(), acc32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccI32AddLocalConstTeePush32Push64(index) => {
                let operand = index.resolve(&executor.func.data);
                acc32 = Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a())
                    .wrapping_add(operand.c());
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.b(), acc32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccBinOpLocalConst32(packed) => {
                let v = packed.index.resolve(&executor.func.data);
                exec_op!(executor; accumulator acc32 = binary
                    Value32::local_get(&executor.store.value_stack, &executor.cf, v.a()), v.b(),
                    |a, b| packed.op.exec(a, b));
            },
            AccBinOpLocalConstTee32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                exec_op!(executor; accumulator acc32 = binary
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a()), operand.c(),
                    |a, b| packed.op.exec(a, b));
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.b(), acc32);
            },
            AccBinOpLocalConstPush32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                acc32 = packed.op.exec(
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a()),
                    operand.b(),
                );
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccBinOpLocalConstTeePush32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                acc32 = packed.op.exec(
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a()),
                    operand.c(),
                );
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.b(), acc32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccI32CmpLocalConst(packed) => {
                let v = packed.index.resolve(&executor.func.data);
                exec_op!(executor; accumulator acc32 = binary
                    Value32::local_get(&executor.store.value_stack, &executor.cf, v.a()), v.b(),
                    |a, b| u32::from(packed.op.cmp(a as i32, b as i32)));
            },
            AccF32CmpLocalConst(packed) => {
                let v = packed.index.resolve(&executor.func.data);
                exec_op!(executor; accumulator acc32 = binary
                    Value32::local_get(&executor.store.value_stack, &executor.cf, v.a()), v.b(),
                    |a, b| u32::from(packed.op.cmp(f32::from_bits(a), f32::from_bits(b))));
            },
            AccBinOpLocalLocal32(op, lhs, rhs) => exec_op!(executor; accumulator acc32 = binary
                Value32::local_get(&executor.store.value_stack, &executor.cf, *lhs),
                Value32::local_get(&executor.store.value_stack, &executor.cf, *rhs),
                |a, b| op.exec(a, b)),
            AccBinOpLocalLocalPush32(op, lhs, rhs) => {
                acc32 = op.exec(
                    Value32::local_get(&executor.store.value_stack, &executor.cf, *lhs),
                    Value32::local_get(&executor.store.value_stack, &executor.cf, *rhs),
                );
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccBinOpLocalLocalTee32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                exec_op!(executor; accumulator acc32 = binary
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a()),
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.b()),
                    |a, b| packed.op.exec(a, b));
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.c(), acc32);
            },
            AccBinOpLocalLocalTeePush32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                acc32 = packed.op.exec(
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a()),
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.b()),
                );
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.c(), acc32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccBinOpNestedLocalLocal32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                let (outer, inner) = packed.op;
                let rhs = inner.exec(
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a()),
                    Value32::local_get(&executor.store.value_stack, &executor.cf, operand.b()),
                );
                acc32 = outer.exec(acc32, rhs);
            },
            AccBinOpNestedLocalConst32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                let (outer, inner) = packed.op;
                let rhs = inner.exec(Value32::local_get(&executor.store.value_stack, &executor.cf, operand.a()), operand.b());
                acc32 = outer.exec(acc32, rhs);
            },
            AccI32CmpLocalLocal(op, lhs, rhs) => exec_op!(executor; accumulator acc32 = binary
                Value32::local_get(&executor.store.value_stack, &executor.cf, *lhs),
                Value32::local_get(&executor.store.value_stack, &executor.cf, *rhs),
                |a, b| u32::from(op.cmp(a as i32, b as i32))),
            AccF32CmpLocalLocal(op, lhs, rhs) => exec_op!(executor; accumulator acc32 = binary
                Value32::local_get(&executor.store.value_stack, &executor.cf, *lhs),
                Value32::local_get(&executor.store.value_stack, &executor.cf, *rhs),
                |a, b| u32::from(op.cmp(f32::from_bits(a), f32::from_bits(b)))),
            AccBinOpStack32(op) => exec_op!(executor; accumulator acc32 = binary Value32::stack_pop(&mut executor.store.value_stack), acc32, #[inline(never)] *op, BinOp, |op, a, b| u64::from(op.exec(a as u32, b as u32))),
            AccBinOpStackPush32(op) => {
                acc32 = op.exec(Value32::stack_pop(&mut executor.store.value_stack), acc32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccBinOpStackStack32(op) => exec_op!(executor; accumulator acc32 = stack_binary Value32, #[inline(never)] *op, BinOp, |op, a, b| u64::from(op.exec(a as u32, b as u32))),
            AccBinOpStackStackPush32(op) => {
                let rhs = Value32::stack_pop(&mut executor.store.value_stack);
                let lhs = Value32::stack_pop(&mut executor.store.value_stack);
                acc32 = op.exec(lhs, rhs);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccI32CmpStack(op) => exec_op!(executor; accumulator acc32 = binary Value32::stack_pop(&mut executor.store.value_stack), acc32, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(a as i32, b as i32))),
            AccI32CmpStackStack(op) => exec_op!(executor; accumulator acc32 = stack_binary Value32, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(a as i32, b as i32))),
            AccI32CmpStackStackPush32(op) => {
                let rhs = Value32::stack_pop(&mut executor.store.value_stack);
                let lhs = Value32::stack_pop(&mut executor.store.value_stack);
                acc32 = u32::from(op.cmp(lhs as i32, rhs as i32));
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccF32CmpStack(op) => exec_op!(executor; accumulator acc32 = binary Value32::stack_pop(&mut executor.store.value_stack), acc32, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(f32::from_bits(a as u32), f32::from_bits(b as u32)))),
            AccF32CmpStackStack(op) => exec_op!(executor; accumulator acc32 = stack_binary Value32, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(f32::from_bits(a as u32), f32::from_bits(b as u32)))),
            AccIntBinOpStack32(op) => {
                let lhs = Value32::stack_pop(&mut executor.store.value_stack);
                acc32 = instructions::exec_int_binop32(*op, lhs, acc32)?;
            },
            AccIntBinOpStackStack32(op) => {
                let rhs = Value32::stack_pop(&mut executor.store.value_stack);
                let lhs = Value32::stack_pop(&mut executor.store.value_stack);
                acc32 = instructions::exec_int_binop32(*op, lhs, rhs)?;
            },
            AccBinOpLocal32(op, local) => exec_op!(executor; accumulator acc32 = binary acc32, Value32::local_get(&executor.store.value_stack, &executor.cf, *local), #[inline(never)] *op, BinOp, |op, a, b| u64::from(op.exec(a as u32, b as u32))),
            AccBinOpLocalPush32(op, local) => {
                acc32 = op.exec(acc32, Value32::local_get(&executor.store.value_stack, &executor.cf, *local));
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccI32CmpLocal(op, local) => exec_op!(executor; accumulator acc32 = binary acc32, Value32::local_get(&executor.store.value_stack, &executor.cf, *local), #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(a as i32, b as i32))),
            AccI32CmpLocalPush32(op, local) => {
                acc32 = u32::from(op.cmp(
                    acc32 as i32,
                    Value32::local_get(&executor.store.value_stack, &executor.cf, *local) as i32,
                ));
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccF32CmpLocal(op, local) => exec_op!(executor; accumulator acc32 = binary acc32, Value32::local_get(&executor.store.value_stack, &executor.cf, *local), #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(f32::from_bits(a as u32), f32::from_bits(b as u32)))),
            AccBinOpConst32(op, value) => exec_op!(executor; accumulator acc32 = binary acc32, *value as u32, #[inline(never)] *op, BinOp, |op, a, b| u64::from(op.exec(a as u32, b as u32))),
            AccBinOpConstPush32(op, value) => {
                acc32 = op.exec(acc32, *value as u32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccBinOpConstTee32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                exec_op!(executor; accumulator acc32 = binary acc32, operand.b(), #[inline(never)] packed.op, BinOp, |op, a, b| u64::from(op.exec(a as u32, b as u32)));
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.a(), acc32);
            },
            AccBinOpConstTeePush32(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                acc32 = packed.op.exec(acc32, operand.b());
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, operand.a(), acc32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccI32CmpConst(op, value) => exec_op!(executor; accumulator acc32 = binary acc32, *value as u32, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(a as i32, b as i32))),
            AccI32CmpConstPush32(op, value) => {
                acc32 = u32::from(op.cmp(acc32 as i32, *value));
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccF32CmpConst(op, value) => exec_op!(executor; accumulator acc32 = binary acc32, *value as u32, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(f32::from_bits(a as u32), f32::from_bits(b as u32)))),
            AccLocalSet32(local) => {
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc32);
            },
            AccLocalTee32(local) => {
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc32);
            },
            AccLocalTeePush32(local) => {
                Value32::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc32);
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccGlobalGet32(global) => acc32 = executor.exec_global_get_value(*global),
            AccGlobalSet32(global) => executor.exec_global_set_value(*global, acc32),
            AccSelect32 => {
                let rhs = Value32::stack_pop(&mut executor.store.value_stack);
                let lhs = Value32::stack_pop(&mut executor.store.value_stack);
                acc32 = if acc32 != 0 { lhs } else { rhs };
            },
            AccSelectPush32 => {
                let rhs = Value32::stack_pop(&mut executor.store.value_stack);
                let lhs = Value32::stack_pop(&mut executor.store.value_stack);
                acc32 = if acc32 != 0 { lhs } else { rhs };
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            JumpIfAccZero32(ip) => if acc32 == 0 { dispatch_next!(*ip as usize) },
            JumpIfAccNonZero32(ip) => if acc32 != 0 { dispatch_next!(*ip as usize) },

            AccConst64(index) => acc64 = index.resolve(&executor.func.data).value() as u64,
            AccLocalGet64(local) => acc64 = Value64::local_get(&executor.store.value_stack, &executor.cf, *local),
            PushAcc64 => Value64::stack_push(&mut executor.store.value_stack, acc64)?,
            AccUnaryStack64(op) => acc64 = instructions::exec_unary64(*op, Value64::stack_pop(&mut executor.store.value_stack))?,
            AccMemorySize64(memory) => acc64 = executor.memory_size(*memory),
            AccTableSize64(table) => acc64 = executor.table_size(*table),
            AccI64Clz => exec_op!(executor; accumulator acc64 = unary acc64, |v| u64::from(v.leading_zeros())),
            AccI64Ctz => exec_op!(executor; accumulator acc64 = unary acc64, |v| u64::from(v.trailing_zeros())),
            AccI64Popcnt => exec_op!(executor; accumulator acc64 = unary acc64, |v| u64::from(v.count_ones())),
            AccI64Extend8S => exec_op!(executor; accumulator acc64 = unary acc64, |v| i64::from(v as i8) as u64),
            AccI64Extend16S => exec_op!(executor; accumulator acc64 = unary acc64, |v| i64::from(v as i16) as u64),
            AccI64Extend32S => exec_op!(executor; accumulator acc64 = unary acc64, |v| i64::from(v as i32) as u64),
            AccI64TruncF64S => exec_op!(executor; accumulator acc64 = unary_fallible acc64, |v| instructions::acc_trunc_f64_s(v)),
            AccI64TruncF64U => exec_op!(executor; accumulator acc64 = unary_fallible acc64, |v| instructions::acc_trunc_f64_u(v)),
            AccI64TruncSatF64S => exec_op!(executor; accumulator acc64 = unary acc64, |v| f64::from_bits(v).trunc() as i64 as u64),
            AccI64TruncSatF64U => exec_op!(executor; accumulator acc64 = unary acc64, |v| f64::from_bits(v).trunc() as u64),
            AccF64ConvertI64S => exec_op!(executor; accumulator acc64 = unary acc64, |v| (v as i64 as f64).to_bits()),
            AccF64ConvertI64U => exec_op!(executor; accumulator acc64 = unary acc64, |v| (v as f64).to_bits()),
            AccF64Abs => exec_op!(executor; accumulator acc64 = unary acc64, |v| f64::from_bits(v).abs().to_bits()),
            AccF64Neg => exec_op!(executor; accumulator acc64 = unary acc64, |v| (-f64::from_bits(v)).to_bits()),
            AccF64Ceil => exec_op!(executor; accumulator acc64 = unary acc64, |v| f64::from_bits(v).ceil().to_bits()),
            AccF64Floor => exec_op!(executor; accumulator acc64 = unary acc64, |v| f64::from_bits(v).floor().to_bits()),
            AccF64Trunc => exec_op!(executor; accumulator acc64 = unary acc64, |v| f64::from_bits(v).trunc().to_bits()),
            AccF64Nearest => exec_op!(executor; accumulator acc64 = unary acc64, |v| f64::from_bits(v).tw_nearest().to_bits()),
            AccF64Sqrt => exec_op!(executor; accumulator acc64 = unary acc64, |v| f64::from_bits(v).sqrt().to_bits()),
            AccI64Eqz => exec_op!(executor; accumulator acc32 = unary acc64, |v| u32::from(v == 0)),
            AccI32WrapI64 => exec_op!(executor; accumulator acc32 = unary acc64, |v| v as u32),
            AccI32TruncF64S => acc32 = instructions::convert_stack64_to_acc32(ConvertOp64To32::I32TruncF64S, acc64)?,
            AccI32TruncF64U => acc32 = instructions::convert_stack64_to_acc32(ConvertOp64To32::I32TruncF64U, acc64)?,
            AccI32TruncSatF64S => acc32 = instructions::convert_stack64_to_acc32(ConvertOp64To32::I32TruncSatF64S, acc64)?,
            AccI32TruncSatF64U => acc32 = instructions::convert_stack64_to_acc32(ConvertOp64To32::I32TruncSatF64U, acc64)?,
            AccF32ConvertI64S => exec_op!(executor; accumulator acc32 = unary acc64, |v| (v as i64 as f32).to_bits()),
            AccF32ConvertI64U => exec_op!(executor; accumulator acc32 = unary acc64, |v| (v as f32).to_bits()),
            AccF32DemoteF64 => exec_op!(executor; accumulator acc32 = unary acc64, |v| (f64::from_bits(v) as f32).to_bits()),
            AccI64ExtendI32S => exec_op!(executor; accumulator acc64 = unary acc32, |v| i64::from(v as i32) as u64),
            AccI64ExtendI32U => exec_op!(executor; accumulator acc64 = unary acc32, |v| u64::from(v)),
            AccI64TruncF32S => acc64 = instructions::convert_stack32_to_acc64(ConvertOp32To64::I64TruncF32S, acc32)?,
            AccI64TruncF32U => acc64 = instructions::convert_stack32_to_acc64(ConvertOp32To64::I64TruncF32U, acc32)?,
            AccI64TruncSatF32S => acc64 = instructions::convert_stack32_to_acc64(ConvertOp32To64::I64TruncSatF32S, acc32)?,
            AccI64TruncSatF32U => acc64 = instructions::convert_stack32_to_acc64(ConvertOp32To64::I64TruncSatF32U, acc32)?,
            AccF64ConvertI32S => exec_op!(executor; accumulator acc64 = unary acc32, |v| (v as i32 as f64).to_bits()),
            AccF64ConvertI32U => exec_op!(executor; accumulator acc64 = unary acc32, |v| (v as f64).to_bits()),
            AccF64PromoteF32 => exec_op!(executor; accumulator acc64 = unary acc32, |v| (f32::from_bits(v) as f64).to_bits()),
            AccConvertStack32To64(op) => acc64 = instructions::convert_stack32_to_acc64(*op, Value32::stack_pop(&mut executor.store.value_stack))?,
            AccConvertStack32To64Push64(op) => {
                acc64 = instructions::convert_stack32_to_acc64(*op, Value32::stack_pop(&mut executor.store.value_stack))?;
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccConvertStack64To32(op) => acc32 = instructions::convert_stack64_to_acc32(*op, Value64::stack_pop(&mut executor.store.value_stack))?,
            AccConvertStack64To32Push32(op) => {
                acc32 = instructions::convert_stack64_to_acc32(*op, Value64::stack_pop(&mut executor.store.value_stack))?;
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            AccLoad64Addr32(index) => acc64 = executor.exec_acc64_load::<u64, 8, false>(index.resolve(&executor.func.data), acc32, acc64, identity)?,
            AccLoad64Addr32Push64(index) => {
                acc64 = executor.exec_acc64_load::<u64, 8, false>(
                    index.resolve(&executor.func.data),
                    acc32,
                    acc64,
                    identity,
                )?;
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccLoad8S64Addr32(index) => acc64 = executor.exec_acc64_load::<i8, 1, false>(index.resolve(&executor.func.data), acc32, acc64, |v| i64::from(v) as u64)?,
            AccLoad8U64Addr32(index) => acc64 = executor.exec_acc64_load::<u8, 1, false>(index.resolve(&executor.func.data), acc32, acc64, u64::from)?,
            AccLoad16S64Addr32(index) => acc64 = executor.exec_acc64_load::<i16, 2, false>(index.resolve(&executor.func.data), acc32, acc64, |v| i64::from(v) as u64)?,
            AccLoad16U64Addr32(index) => acc64 = executor.exec_acc64_load::<u16, 2, false>(index.resolve(&executor.func.data), acc32, acc64, u64::from)?,
            AccLoad32S64Addr32(index) => acc64 = executor.exec_acc64_load::<i32, 4, false>(index.resolve(&executor.func.data), acc32, acc64, |v| i64::from(v) as u64)?,
            AccLoad32U64Addr32(index) => acc64 = executor.exec_acc64_load::<u32, 4, false>(index.resolve(&executor.func.data), acc32, acc64, u64::from)?,
            AccLoad64Addr64(index) => acc64 = executor.exec_acc64_load::<u64, 8, true>(index.resolve(&executor.func.data), acc32, acc64, identity)?,
            AccLoad8S64Addr64(index) => acc64 = executor.exec_acc64_load::<i8, 1, true>(index.resolve(&executor.func.data), acc32, acc64, |v| i64::from(v) as u64)?,
            AccLoad8U64Addr64(index) => acc64 = executor.exec_acc64_load::<u8, 1, true>(index.resolve(&executor.func.data), acc32, acc64, u64::from)?,
            AccLoad16S64Addr64(index) => acc64 = executor.exec_acc64_load::<i16, 2, true>(index.resolve(&executor.func.data), acc32, acc64, |v| i64::from(v) as u64)?,
            AccLoad16U64Addr64(index) => acc64 = executor.exec_acc64_load::<u16, 2, true>(index.resolve(&executor.func.data), acc32, acc64, u64::from)?,
            AccLoad32S64Addr64(index) => acc64 = executor.exec_acc64_load::<i32, 4, true>(index.resolve(&executor.func.data), acc32, acc64, |v| i64::from(v) as u64)?,
            AccLoad32U64Addr64(index) => acc64 = executor.exec_acc64_load::<u32, 4, true>(index.resolve(&executor.func.data), acc32, acc64, u64::from)?,
            AccLoadStack64(packed) => {
                let memory = packed.index.resolve(&executor.func.data);
                match packed.op {
                    LoadOp64::Full => acc64 = executor.exec_acc64_load_stack::<u64, 8>(memory, identity)?,
                    LoadOp64::I8S => acc64 = executor.exec_acc64_load_stack::<i8, 1>(memory, |v| i64::from(v) as u64)?,
                    LoadOp64::I8U => acc64 = executor.exec_acc64_load_stack::<u8, 1>(memory, u64::from)?,
                    LoadOp64::I16S => acc64 = executor.exec_acc64_load_stack::<i16, 2>(memory, |v| i64::from(v) as u64)?,
                    LoadOp64::I16U => acc64 = executor.exec_acc64_load_stack::<u16, 2>(memory, u64::from)?,
                    LoadOp64::I32S => acc64 = executor.exec_acc64_load_stack::<i32, 4>(memory, |v| i64::from(v) as u64)?,
                    LoadOp64::I32U => acc64 = executor.exec_acc64_load_stack::<u32, 4>(memory, u64::from)?,
                }
            },
            AccLoadStackPush64(packed) => {
                let memory = packed.index.resolve(&executor.func.data);
                acc64 = match packed.op {
                    LoadOp64::Full => executor.exec_acc64_load_stack::<u64, 8>(memory, identity)?,
                    LoadOp64::I8S => executor.exec_acc64_load_stack::<i8, 1>(memory, |v| i64::from(v) as u64)?,
                    LoadOp64::I8U => executor.exec_acc64_load_stack::<u8, 1>(memory, u64::from)?,
                    LoadOp64::I16S => executor.exec_acc64_load_stack::<i16, 2>(memory, |v| i64::from(v) as u64)?,
                    LoadOp64::I16U => executor.exec_acc64_load_stack::<u16, 2>(memory, u64::from)?,
                    LoadOp64::I32S => executor.exec_acc64_load_stack::<i32, 4>(memory, |v| i64::from(v) as u64)?,
                    LoadOp64::I32U => executor.exec_acc64_load_stack::<u32, 4>(memory, u64::from)?,
                };
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccLoad32Addr64(index) => acc32 = executor.exec_acc32_load_addr64::<u32, 4>(index.resolve(&executor.func.data), acc64, identity)?,
            AccLoad8S32Addr64(index) => acc32 = executor.exec_acc32_load_addr64::<i8, 1>(index.resolve(&executor.func.data), acc64, |v| i32::from(v) as u32)?,
            AccLoad8U32Addr64(index) => acc32 = executor.exec_acc32_load_addr64::<u8, 1>(index.resolve(&executor.func.data), acc64, u32::from)?,
            AccLoad16S32Addr64(index) => acc32 = executor.exec_acc32_load_addr64::<i16, 2>(index.resolve(&executor.func.data), acc64, |v| i32::from(v) as u32)?,
            AccLoad16U32Addr64(index) => acc32 = executor.exec_acc32_load_addr64::<u16, 2>(index.resolve(&executor.func.data), acc64, u32::from)?,
            AccStore64(index) => { let m = index.resolve(&executor.func.data); executor.exec_mem_store_value(executor.mem_addr(m.memory()), m.offset(), acc64)?; },
            AccStore8_64(index) => { let m = index.resolve(&executor.func.data); executor.exec_mem_store_value(executor.mem_addr(m.memory()), m.offset(), acc64 as u8)?; },
            AccStore16_64(index) => { let m = index.resolve(&executor.func.data); executor.exec_mem_store_value(executor.mem_addr(m.memory()), m.offset(), acc64 as u16)?; },
            AccStore32_64(index) => { let m = index.resolve(&executor.func.data); executor.exec_mem_store_value(executor.mem_addr(m.memory()), m.offset(), acc64 as u32)?; },
            AccBinOpStack64(op) => exec_op!(executor; accumulator acc64 = binary Value64::stack_pop(&mut executor.store.value_stack), acc64, #[inline(never)] *op, BinOp, |op, a, b| op.exec(a, b)),
            AccBinOpStackTee64(op, local) => {
                exec_op!(executor; accumulator acc64 = binary Value64::stack_pop(&mut executor.store.value_stack), acc64, #[inline(never)] *op, BinOp, |op, a, b| op.exec(a, b));
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc64);
            },
            AccBinOpStackStack64(op) => exec_op!(executor; accumulator acc64 = stack_binary Value64, #[inline(never)] *op, BinOp, |op, a, b| op.exec(a, b)),
            AccBinOpStackStackPush64(op) => {
                let rhs = Value64::stack_pop(&mut executor.store.value_stack);
                let lhs = Value64::stack_pop(&mut executor.store.value_stack);
                acc64 = op.exec(lhs, rhs);
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccBinOpStackStackTee64(op, local) => {
                let rhs = Value64::stack_pop(&mut executor.store.value_stack);
                let lhs = Value64::stack_pop(&mut executor.store.value_stack);
                acc64 = op.exec(lhs, rhs);
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc64);
            },
            AccBinOpStackStackTeePush64(op, local) => {
                let rhs = Value64::stack_pop(&mut executor.store.value_stack);
                let lhs = Value64::stack_pop(&mut executor.store.value_stack);
                acc64 = op.exec(lhs, rhs);
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc64);
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccIntBinOpStack64(op) => {
                let lhs = Value64::stack_pop(&mut executor.store.value_stack);
                acc64 = instructions::exec_int_binop64(*op, lhs, acc64)?;
            },
            AccIntBinOpStackStack64(op) => {
                let rhs = Value64::stack_pop(&mut executor.store.value_stack);
                let lhs = Value64::stack_pop(&mut executor.store.value_stack);
                acc64 = instructions::exec_int_binop64(*op, lhs, rhs)?;
            },
            AccBinOpLocal64(op, local) => exec_op!(executor; accumulator acc64 = binary acc64, Value64::local_get(&executor.store.value_stack, &executor.cf, *local), #[inline(never)] *op, BinOp, |op, a, b| op.exec(a, b)),
            AccBinOpLocalPush64(op, local) => {
                acc64 = op.exec(acc64, Value64::local_get(&executor.store.value_stack, &executor.cf, *local));
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccBinOpLocalTee64(op, local, destination) => {
                exec_op!(executor; accumulator acc64 = binary acc64, Value64::local_get(&executor.store.value_stack, &executor.cf, *local), #[inline(never)] *op, BinOp, |op, a, b| op.exec(a, b));
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, *destination, acc64);
            },
            AccBinOpLocalTeePush64(op, local, destination) => {
                acc64 = op.exec(acc64, Value64::local_get(&executor.store.value_stack, &executor.cf, *local));
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, *destination, acc64);
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccBinOpConst64(packed) => exec_op!(executor; accumulator acc64 = binary acc64, packed.index.resolve(&executor.func.data).value() as u64, #[inline(never)] packed.op, BinOp, |op, a, b| op.exec(a, b)),
            AccBinOpConstPush64(packed) => {
                acc64 = packed.op.exec(acc64, packed.index.resolve(&executor.func.data).value() as u64);
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccBinOpConstTee64(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                exec_op!(executor; accumulator acc64 = binary acc64, operand.b(), #[inline(never)] packed.op, BinOp, |op, a, b| op.exec(a, b));
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, operand.a(), acc64);
            },
            AccBinOpConstTeePush64(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                acc64 = packed.op.exec(acc64, operand.b());
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, operand.a(), acc64);
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccBinOpLocalConst64(packed) => { let v = packed.index.resolve(&executor.func.data); exec_op!(executor; accumulator acc64 = binary Value64::local_get(&executor.store.value_stack, &executor.cf, v.a()), v.b(), #[inline(never)] packed.op, BinOp, |op, a, b| op.exec(a, b)); },
            AccBinOpLocalConstPush64(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                acc64 = packed.op.exec(
                    Value64::local_get(&executor.store.value_stack, &executor.cf, operand.a()),
                    operand.b(),
                );
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccBinOpLocalLocal64(op, lhs, rhs) => exec_op!(executor; accumulator acc64 = binary Value64::local_get(&executor.store.value_stack, &executor.cf, *lhs), Value64::local_get(&executor.store.value_stack, &executor.cf, *rhs), #[inline(never)] *op, BinOp, |op, a, b| op.exec(a, b)),
            AccBinOpLocalLocalPush64(op, lhs, rhs) => {
                acc64 = op.exec(
                    Value64::local_get(&executor.store.value_stack, &executor.cf, *lhs),
                    Value64::local_get(&executor.store.value_stack, &executor.cf, *rhs),
                );
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccBinOpNestedLocalLocal64(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                let (outer, inner) = packed.op;
                let rhs = inner.exec(
                    Value64::local_get(&executor.store.value_stack, &executor.cf, operand.a()),
                    Value64::local_get(&executor.store.value_stack, &executor.cf, operand.b()),
                );
                acc64 = outer.exec(acc64, rhs);
            },
            AccBinOpNestedLocalLocalTee64(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                let (outer, inner) = packed.op;
                let rhs = inner.exec(
                    Value64::local_get(&executor.store.value_stack, &executor.cf, operand.a()),
                    Value64::local_get(&executor.store.value_stack, &executor.cf, operand.b()),
                );
                acc64 = outer.exec(acc64, rhs);
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, operand.c(), acc64);
            },
            AccBinOpNestedLocalConst64(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                let (outer, inner) = packed.op;
                let rhs = inner.exec(Value64::local_get(&executor.store.value_stack, &executor.cf, operand.a()), operand.b());
                acc64 = outer.exec(acc64, rhs);
            },
            AccBinOpNestedLocalConstPush64(packed) => {
                let operand = packed.index.resolve(&executor.func.data);
                let (outer, inner) = packed.op;
                let rhs = inner.exec(Value64::local_get(&executor.store.value_stack, &executor.cf, operand.a()), operand.b());
                acc64 = outer.exec(acc64, rhs);
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccI64CmpStack(op) => exec_op!(executor; accumulator acc32 = binary Value64::stack_pop(&mut executor.store.value_stack), acc64, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(a as i64, b as i64))),
            AccI64CmpStackStack(op) => exec_op!(executor; accumulator acc32 = stack_binary Value64, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(a as i64, b as i64))),
            AccI64CmpLocal(op, local) => exec_op!(executor; accumulator acc32 = binary acc64, Value64::local_get(&executor.store.value_stack, &executor.cf, *local), #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(a as i64, b as i64))),
            AccI64CmpConst(packed) => exec_op!(executor; accumulator acc32 = binary acc64, packed.index.resolve(&executor.func.data).value() as u64, #[inline(always)] packed.op, CmpOp, |op, a, b| u64::from(op.cmp(a as i64, b as i64))),
            AccI64CmpLocalConst(packed) => { let v = packed.index.resolve(&executor.func.data); exec_op!(executor; accumulator acc32 = binary Value64::local_get(&executor.store.value_stack, &executor.cf, v.a()), v.b(), #[inline(always)] packed.op, CmpOp, |op, a, b| u64::from(op.cmp(a as i64, b as i64))); },
            AccI64CmpLocalLocal(op, lhs, rhs) => exec_op!(executor; accumulator acc32 = binary Value64::local_get(&executor.store.value_stack, &executor.cf, *lhs), Value64::local_get(&executor.store.value_stack, &executor.cf, *rhs), #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(a as i64, b as i64))),
            AccF64CmpStack(op) => exec_op!(executor; accumulator acc32 = binary Value64::stack_pop(&mut executor.store.value_stack), acc64, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(f64::from_bits(a), f64::from_bits(b)))),
            AccF64CmpStackStack(op) => exec_op!(executor; accumulator acc32 = stack_binary Value64, #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(f64::from_bits(a), f64::from_bits(b)))),
            AccF64CmpLocal(op, local) => exec_op!(executor; accumulator acc32 = binary acc64, Value64::local_get(&executor.store.value_stack, &executor.cf, *local), #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(f64::from_bits(a), f64::from_bits(b)))),
            AccF64CmpConst(packed) => exec_op!(executor; accumulator acc32 = binary acc64, packed.index.resolve(&executor.func.data).value() as u64, #[inline(always)] packed.op, CmpOp, |op, a, b| u64::from(op.cmp(f64::from_bits(a), f64::from_bits(b)))),
            AccF64CmpLocalConst(packed) => { let v = packed.index.resolve(&executor.func.data); exec_op!(executor; accumulator acc32 = binary Value64::local_get(&executor.store.value_stack, &executor.cf, v.a()), v.b(), #[inline(always)] packed.op, CmpOp, |op, a, b| u64::from(op.cmp(f64::from_bits(a), f64::from_bits(b)))); },
            AccF64CmpLocalLocal(op, lhs, rhs) => exec_op!(executor; accumulator acc32 = binary Value64::local_get(&executor.store.value_stack, &executor.cf, *lhs), Value64::local_get(&executor.store.value_stack, &executor.cf, *rhs), #[inline(always)] *op, CmpOp, |op, a, b| u64::from(op.cmp(f64::from_bits(a), f64::from_bits(b)))),
            AccLocalSet64(local) => Value64::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc64),
            AccLocalTee64(local) => Value64::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc64),
            AccLocalTeePush64(local) => {
                Value64::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc64);
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            AccGlobalGet64(global) => acc64 = executor.exec_global_get_value(*global),
            AccGlobalSet64(global) => executor.exec_global_set_value(*global, acc64),
            AccSelect64 => { let rhs = Value64::stack_pop(&mut executor.store.value_stack); let lhs = Value64::stack_pop(&mut executor.store.value_stack); acc64 = if acc32 != 0 { lhs } else { rhs }; },
            JumpIfAccZero64(ip) => if acc64 == 0 { dispatch_next!(*ip as usize) },
            JumpIfAccNonZero64(ip) => if acc64 != 0 { dispatch_next!(*ip as usize) },
            AccRefNull => acc_ref = ValueRef::NULL,
            AccRefFunc(func_idx) => acc_ref = ValueRef::from_category_addr(executor.module.resolve_func_addr(*func_idx)),
            AccRefLocalGet(local) => acc_ref = ValueRef::local_get(&executor.store.value_stack, &executor.cf, *local),
            PushAccRef => {
                let reference = core::mem::replace(&mut acc_ref, ValueRef::NULL);
                ValueRef::stack_push(&mut executor.store.value_stack, reference)?;
            },
            ClearAccRef => acc_ref = ValueRef::NULL,
            AccRefLocalSet(local) => {
                let reference = core::mem::replace(&mut acc_ref, ValueRef::NULL);
                ValueRef::local_set(&mut executor.store.value_stack, &executor.cf, *local, reference);
            },
            AccRefLocalTee(local) => ValueRef::local_set(&mut executor.store.value_stack, &executor.cf, *local, acc_ref),
            AccRefGlobalGet(global) => {
                let global = executor.module.resolve_global_addr(*global);
                acc_ref = ValueRef::global_get(&executor.store.state.globals, global);
            },
            AccRefGlobalSet(global) => {
                let global = executor.module.resolve_global_addr(*global);
                let reference = core::mem::replace(&mut acc_ref, ValueRef::NULL);
                ValueRef::global_set(&mut executor.store.state.globals, global, reference);
            },
            Unreachable => { return cold!(Err(Trap::Unreachable.into())); },
            Drop32 => { _ = Value32::stack_pop(&mut executor.store.value_stack)},
            Drop64 => { _ = Value64::stack_pop(&mut executor.store.value_stack)},
            Drop128 => { _ = Value128::stack_pop(&mut executor.store.value_stack)},
            Select32 => Value32::stack_select(&mut executor.store.value_stack),
            Select64 => Value64::stack_select(&mut executor.store.value_stack),
            Select128 => Value128::stack_select(&mut executor.store.value_stack),
            SelectStore32(idx) => executor.exec_select_store::<Value32, 4>(idx.resolve(&executor.func.data))?,
            SelectStore64(idx) => executor.exec_select_store::<Value64, 8>(idx.resolve(&executor.func.data))?,
            SelectMulti(counts) => executor.store.value_stack.select_multi(*counts),
            Call(v) => dispatch_flow!(executor.exec_call_direct(*v, instr_ptr + 1)?),
            CallSelf => { executor.exec_call_self(instr_ptr + 1)?; dispatch_next!(0); },
            CallIndirect(idx) => dispatch_flow!(executor.exec_call_indirect::<false>(*idx, instr_ptr + 1)?),
            CallRef(ty) => dispatch_flow!(executor.exec_call_ref::<false>(*ty, instr_ptr + 1)?),
            ReturnCall(v) => dispatch_flow!(executor.exec_return_call_direct(*v)?),
            ReturnCallSelf => { executor.exec_return_call_self()?; dispatch_next!(0); },
            ReturnCallIndirect(idx) => dispatch_flow!(executor.exec_call_indirect::<true>(*idx, instr_ptr + 1)?),
            ReturnCallRef(ty) => dispatch_flow!(executor.exec_call_ref::<true>(*ty, instr_ptr + 1)?),
            Throw(tag) => dispatch_flow!(executor.exec_throw(*tag, instr_ptr, acc_ref)?),
            ThrowRef => dispatch_flow!(executor.exec_throw_ref(instr_ptr)?),
            Jump(ip) => dispatch_next!(*ip as usize),
            JumpIfZero32(ip) => if i32::stack_pop(&mut executor.store.value_stack) == 0 { dispatch_next!(*ip as usize) },
            JumpIfNonZero32(ip) => if i32::stack_pop(&mut executor.store.value_stack) != 0 { dispatch_next!(*ip as usize) },
            JumpIfZero64(ip) => if i64::stack_pop(&mut executor.store.value_stack) == 0 { dispatch_next!(*ip as usize) },
            JumpIfNonZero64(ip) => if i64::stack_pop(&mut executor.store.value_stack) != 0 { dispatch_next!(*ip as usize) },
            JumpIfRefNull(ip) => { let ip = *ip; if executor.exec_jump_if_ref::<true>() { dispatch_next!(ip as usize) } },
            JumpIfRefNonNull(ip) => { let ip = *ip; if executor.exec_jump_if_ref::<false>() { dispatch_next!(ip as usize) } },
            BrOnCast(idx) => if let Some(ip) = executor.exec_br_on_cast::<false>(*idx) { dispatch_next!(ip) },
            BrOnCastFail(idx) => if let Some(ip) = executor.exec_br_on_cast::<true>(*idx) { dispatch_next!(ip) },
            JumpIfLocalZero32(arg) => if executor.exec_jump_if_local::<Value32, true>(arg.local) { dispatch_next!(arg.target_ip as usize) },
            JumpIfLocalNonZero32(arg) => if executor.exec_jump_if_local::<Value32, false>(arg.local) { dispatch_next!(arg.target_ip as usize) },
            JumpIfLocalZero64(arg) => if executor.exec_jump_if_local::<Value64, true>(arg.local) { dispatch_next!(arg.target_ip as usize) },
            JumpIfLocalNonZero64(arg) => if executor.exec_jump_if_local::<Value64, false>(arg.local) { dispatch_next!(arg.target_ip as usize) },
            JumpCmpStackConst32(packed) => if let Some(ip) = executor.exec_jump_cmp_stack_const32(*packed) { dispatch_next!(ip) },
            JumpCmpStackConst64(packed) => if let Some(ip) = executor.exec_jump_cmp_stack_const64(*packed) { dispatch_next!(ip) },
            JumpCmpStackLocal32(packed) => if let Some(ip) = executor.exec_jump_cmp_stack_local32(*packed) { dispatch_next!(ip) },
            JumpCmpStackLocal64(packed) => if let Some(ip) = executor.exec_jump_cmp_stack_local64(*packed) { dispatch_next!(ip) },
            BinOpLocalConstJump32(packed) => if let Some(ip) = executor.exec_binop_local_const_jump(*packed) { dispatch_next!(ip) },
            BinOpLocalConstJumpCmpLocal32(packed) => if let Some(ip) = executor.exec_binop_local_const_jump_cmp_local(*packed) { dispatch_next!(ip) },
            BinOpStackConstTeeLocalJump32(packed) => if let Some(ip) = executor.exec_binop_stack_const_tee_local_jump(*packed)? { dispatch_next!(ip) },
            BinOpGlobalConstJump32(packed) => if let Some(ip) = executor.exec_binop_global_const_jump(*packed) { dispatch_next!(ip) },
            IncLocalJump32(idx) => if let Some(ip) = executor.exec_inc_local_jump(*idx) { dispatch_next!(ip) },
            IncStackTeeLocalJump32(idx) => if let Some(ip) = executor.exec_inc_stack_tee_local_jump(*idx)? { dispatch_next!(ip) },
            IncGlobalJump32(idx) => if let Some(ip) = executor.exec_inc_global_jump(*idx) { dispatch_next!(ip) },
            IncLocalJumpCmpLocal32(packed) => if let Some(ip) = executor.exec_inc_local_jump_cmp_local(*packed) { dispatch_next!(ip) },
            JumpCmpLocalConst32(packed) => if let Some(ip) = executor.exec_jump_cmp_local_const32(*packed) { dispatch_next!(ip) },
            JumpCmpLocalConst64(packed) => if let Some(ip) = executor.exec_jump_cmp_local_const64(*packed) { dispatch_next!(ip) },
            JumpCmpLocalLocal32(packed) => if let Some(ip) = executor.exec_jump_cmp_local_local32(*packed) { dispatch_next!(ip) },
            JumpCmpLocalLocal64(packed) => if let Some(ip) = executor.exec_jump_cmp_local_local64(*packed) { dispatch_next!(ip) },
            DropKeep32 { base, keep } => executor.store.value_stack.stack_32.truncate_keep((executor.cf.stack_base().s32 + u32::from(*base)) as usize, *keep as usize),
            DropKeep64 { base, keep } => executor.store.value_stack.stack_64.truncate_keep((executor.cf.stack_base().s64 + u32::from(*base)) as usize, *keep as usize),
            DropKeep128 { base, keep } => executor.store.value_stack.stack_128.truncate_keep((executor.cf.stack_base().s128 + u32::from(*base)) as usize, *keep as usize),
            BranchTable(idx) => dispatch_next!(executor.exec_branch_table(*idx)),
            Return => dispatch_flow!(executor.exec_return()),
            ReturnVoid => dispatch_flow!(executor.exec_return_void()),
            Return32 => dispatch_flow!(executor.exec_return_32()),
            Return64 => dispatch_flow!(executor.exec_return_64()),
            ReturnAcc32 => dispatch_flow!(executor.exec_return_acc32(acc32)?),
            ReturnAcc64 => dispatch_flow!(executor.exec_return_acc64(acc64)?),
            ReturnAccRef => dispatch_flow!(executor.exec_return_acc_ref(&mut acc_ref)?),
            Return128 => dispatch_flow!(executor.exec_return_128()),
            LocalGet32(local_index) => Value32::local_push(&mut executor.store.value_stack, &executor.cf, *local_index)?,
            LocalGetPushAcc32(local_index) => {
                Value32::local_push(&mut executor.store.value_stack, &executor.cf, *local_index)?;
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            LocalGetPushAcc32PushAcc64(local_index) => {
                Value32::local_push(&mut executor.store.value_stack, &executor.cf, *local_index)?;
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
                Value64::stack_push(&mut executor.store.value_stack, acc64)?;
            },
            LocalGet64(local_index) => Value64::local_push(&mut executor.store.value_stack, &executor.cf, *local_index)?,
            LocalGet128(local_index) => Value128::local_push(&mut executor.store.value_stack, &executor.cf, *local_index)?,
            LocalSet32(local_index) => executor.exec_local_set_pop::<Value32>(*local_index),
            LocalSet64(local_index) => executor.exec_local_set_pop::<Value64>(*local_index),
            LocalSet128(local_index) => executor.exec_local_set_pop::<Value128>(*local_index),
            LocalCopy32(from, to) => Value32::local_copy(&mut executor.store.value_stack, &executor.cf, *from, *to),
            LocalCopy64(from, to) => Value64::local_copy(&mut executor.store.value_stack, &executor.cf, *from, *to),
            LocalCopy128(from, to) => Value128::local_copy(&mut executor.store.value_stack, &executor.cf, *from, *to),
            AddConst32(c) => { i32::stack_update(&mut executor.store.value_stack, |value| value.wrapping_add(*c)); },
            AndConst32(c) => { i32::stack_update(&mut executor.store.value_stack, |value| value & *c); },
            XorConst32(c) => { i32::stack_update(&mut executor.store.value_stack, |value| value ^ *c); },
            ShrUConst32(c) => { u32::stack_update(&mut executor.store.value_stack, |value| value.wrapping_shr(*c as u32)); },
            AddConst64(idx) => { let rhs = idx.resolve(&executor.func.data).value(); i64::stack_update(&mut executor.store.value_stack, |value| value.wrapping_add(rhs)); },
            BinOpStackConst32(op, rhs) => { u32::stack_update(&mut executor.store.value_stack, |lhs| op.exec(lhs, *rhs as u32)); },
            BinOpStackConst64(packed) => { let rhs = packed.index.resolve(&executor.func.data).value() as u64; u64::stack_update(&mut executor.store.value_stack, |lhs| packed.op.exec(lhs, rhs)); },
            IncLocal32(arg) => { i32::local_update(&mut executor.store.value_stack, &executor.cf, arg.local, |value| value.wrapping_add(arg.value)); },
            IncLocal64(packed) => { let rhs = packed.index.resolve(&executor.func.data).value(); i64::local_update(&mut executor.store.value_stack, &executor.cf, packed.op, |value| value.wrapping_add(rhs)); },
            I32Add3 => exec_op!(executor; ternary i32 => i32, |a, b, c| a.wrapping_add(b).wrapping_add(c)),
            I64Add3 => exec_op!(executor; ternary i64 => i64, |a, b, c| a.wrapping_add(b).wrapping_add(c)),
            MulAccLocal32(acc) => executor.exec_mul_acc_local::<i32>(*acc, i32::wrapping_mul, i32::wrapping_add),
            MulAccLocal64(acc) => executor.exec_mul_acc_local::<i64>(*acc, i64::wrapping_mul, i64::wrapping_add),
            FMulAccLocal32(acc) => executor.exec_mul_acc_local::<f32>(*acc, #[inline(always)] |a, b| a * b, #[inline(always)] |a, b| a + b),
            FMulAccLocal64(acc) => executor.exec_mul_acc_local::<f64>(*acc, #[inline(always)] |a, b| a * b, #[inline(always)] |a, b| a + b),
            BinOpLocalLocal32(op, a, b) => executor.exec_binop_local_local::<Value32, true>(*a, *b, None, *op)?,
            BinOpLocalLocal64(op, a, b) => executor.exec_binop_local_local::<Value64, true>(*a, *b, None, *op)?,
            BinOpLocalLocal128(op, a, b) => executor.exec_binop_local_local::<Value128, true>(*a, *b, None, *op)?,
            CmpLocalLocal32(op, a, b) => executor.exec_cmp_local_local::<i32>(*a, *b, *op)?,
            CmpLocalLocal64(op, a, b) => executor.exec_cmp_local_local::<i64>(*a, *b, *op)?,
            AddLocalLocalSet32(arg) => executor.exec_binop_local_local::<Value32, false>(arg.left, arg.right, Some(arg.dst), BinOp::IAdd)?,
            AddLocalLocalTee32(arg) => executor.exec_binop_local_local::<Value32, true>(arg.left, arg.right, Some(arg.dst), BinOp::IAdd)?,
            BinOpLocalLocalSet32(packed) => executor.exec_binop_local_local_indexed::<Value32, false>(packed.index, packed.op)?,
            BinOpLocalLocalSet64(packed) => executor.exec_binop_local_local_indexed::<Value64, false>(packed.index, packed.op)?,
            BinOpLocalLocalSet128(packed) => executor.exec_binop_local_local_indexed::<Value128, false>(packed.index, packed.op)?,
            BinOpLocalLocalTee32(packed) => executor.exec_binop_local_local_indexed::<Value32, true>(packed.index, packed.op)?,
            BinOpLocalLocalTee64(packed) => executor.exec_binop_local_local_indexed::<Value64, true>(packed.index, packed.op)?,
            BinOpLocalLocalTee128(packed) => executor.exec_binop_local_local_indexed::<Value128, true>(packed.index, packed.op)?,
            AddLocalConst32(arg) => executor.exec_binop_local_const::<Value32, true>(arg.local, arg.value as u32, None, BinOp::IAdd)?,
            SubLocalConst32(arg) => executor.exec_binop_local_const::<Value32, true>(arg.local, arg.value as u32, None, BinOp::ISub)?,
            MulLocalConst32(arg) => executor.exec_binop_local_const::<Value32, true>(arg.local, arg.value as u32, None, BinOp::IMul)?,
            AndLocalConst32(arg) => executor.exec_binop_local_const::<Value32, true>(arg.local, arg.value as u32, None, BinOp::IAnd)?,
            ShrULocalConst32(arg) => executor.exec_binop_local_const::<Value32, true>(arg.local, arg.value as u32, None, BinOp::IShrU)?,
            BinOpLocalConst32(packed) => { let v = packed.index.resolve(&executor.func.data); executor.exec_binop_local_const::<Value32, true>(v.a(), v.b(), None, packed.op)?; },
            BinOpLocalConst64(packed) => { let v = packed.index.resolve(&executor.func.data); executor.exec_binop_local_const::<Value64, true>(v.a(), v.b(), None, packed.op)?; },
            BinOpLocalConst128(packed) => { let v = packed.index.resolve(&executor.func.data); let rhs = Value128(v.b().resolve(&executor.func.data).value()); executor.exec_binop_local_const::<Value128, true>(v.a(), rhs, None, packed.op)?; },
            BinOpLocalConstSet32(packed) => { let v = packed.index.resolve(&executor.func.data); executor.exec_binop_local_const::<Value32, false>(v.a(), v.c(), Some(v.b()), packed.op)?; },
            BinOpLocalConstTee32(packed) => { let v = packed.index.resolve(&executor.func.data); executor.exec_binop_local_const::<Value32, true>(v.a(), v.c(), Some(v.b()), packed.op)?; },
            BinOpLocalConstSet64(packed) => { let v = packed.index.resolve(&executor.func.data); executor.exec_binop_local_const::<Value64, false>(v.a(), v.c(), Some(v.b()), packed.op)?; },
            BinOpLocalConstTee64(packed) => { let v = packed.index.resolve(&executor.func.data); executor.exec_binop_local_const::<Value64, true>(v.a(), v.c(), Some(v.b()), packed.op)?; },
            BinOpLocalConstSet128(packed) => { let v = packed.index.resolve(&executor.func.data); let rhs = Value128(v.c().resolve(&executor.func.data).value()); executor.exec_binop_local_const::<Value128, false>(v.a(), rhs, Some(v.b()), packed.op)?; },
            BinOpLocalConstTee128(packed) => { let v = packed.index.resolve(&executor.func.data); let rhs = Value128(v.c().resolve(&executor.func.data).value()); executor.exec_binop_local_const::<Value128, true>(v.a(), rhs, Some(v.b()), packed.op)?; },
            BinOpStackLocal32(op, local) => executor.exec_binop_stack_local::<Value32, true>(*local, None, *op)?,
            BinOpStackLocalSet32(op, local, dst) => executor.exec_binop_stack_local::<Value32, false>(*local, Some(*dst), *op)?,
            BinOpStackLocalTee32(op, local, dst) => executor.exec_binop_stack_local::<Value32, true>(*local, Some(*dst), *op)?,
            BinOpStackLocal128(op, local) => executor.exec_binop_stack_local::<Value128, true>(*local, None, *op)?,
            BinOpStackGlobal32(op, global_index) => executor.exec_binop_stack_global::<Value32>(*global_index, *op)?,
            BinOpStackGlobal64(op, global_index) => executor.exec_binop_stack_global::<Value64>(*global_index, *op)?,
            BinOpGlobalConst32(packed) => { let v = packed.index.resolve(&executor.func.data); executor.exec_binop_global_const::<Value32>(v.a(), v.b(), packed.op)?; },
            BinOpGlobalConst64(packed) => { let v = packed.index.resolve(&executor.func.data); executor.exec_binop_global_const::<Value64>(v.a(), v.b(), packed.op)?; },
            BinOpGlobalConst128(packed) => { let v = packed.index.resolve(&executor.func.data); let rhs = Value128(v.b().resolve(&executor.func.data).value()); executor.exec_binop_global_const::<Value128>(v.a(), rhs, packed.op)?; },
            SetLocalConst32(arg) => i32::local_set(&mut executor.store.value_stack, &executor.cf, arg.local, arg.value),
            SetLocalConst64(packed) => { let v = packed.index.resolve(&executor.func.data).value(); i64::local_set(&mut executor.store.value_stack, &executor.cf, packed.op, v); },
            SetLocalConst128(packed) => Value128::local_set(&mut executor.store.value_stack, &executor.cf, packed.op, Value128(packed.index.resolve(&executor.func.data).value())),
            IncMemoryLocal32(arg) => executor.exec_inc_memory_local::<i32, 4>(arg.memory_arg_idx, arg.local1, #[inline(always)] |v| v.wrapping_add(1))?,
            IncMemoryLocal64(arg) => executor.exec_inc_memory_local::<i64, 8>(arg.memory_arg_idx, arg.local1, #[inline(always)] |v| v.wrapping_add(1))?,
            StoreLocalLocal32(arg) => executor.exec_store_local_local::<u32, 4>(arg.memory_arg_idx, arg.local1, arg.local2)?,
            StoreLocalLocal64(arg) => executor.exec_store_local_local::<i64, 8>(arg.memory_arg_idx, arg.local1, arg.local2)?,
            StoreLocalLocal128(arg) => executor.exec_store_local_local::<Value128, 16>(arg.memory_arg_idx, arg.local1, arg.local2)?,
            LoadLocal32(arg) => executor.exec_load_local::<i32, 4, _, false, false>(arg.memory_arg_idx, arg.local1, 0, identity)?,
            LoadLocal64(arg) => executor.exec_load_local::<i64, 8, _, false, false>(arg.memory_arg_idx, arg.local1, 0, identity)?,
            LoadLocal8S32(arg) => executor.exec_load_local::<i8, 1, _, false, false>(arg.memory_arg_idx, arg.local1, 0, i32::from)?,
            LoadLocal8U32(arg) => executor.exec_load_local::<u8, 1, _, false, false>(arg.memory_arg_idx, arg.local1, 0, i32::from)?,
            LoadLocal16S32(arg) => executor.exec_load_local::<i16, 2, _, false, false>(arg.memory_arg_idx, arg.local1, 0, i32::from)?,
            LoadLocal16U32(arg) => executor.exec_load_local::<u16, 2, _, false, false>(arg.memory_arg_idx, arg.local1, 0, i32::from)?,
            LoadLocalTee32(arg) => executor.exec_load_local::<i32, 4, _, true, true>(arg.memory_arg_idx, arg.local1, arg.local2, identity)?,
            LoadLocalSet32(arg) => executor.exec_load_local::<i32, 4, _, true, false>(arg.memory_arg_idx, arg.local1, arg.local2, identity)?,
            LoadLocalTee8S32(arg) => executor.exec_load_local::<i8, 1, _, true, true>(arg.memory_arg_idx, arg.local1, arg.local2, i32::from)?,
            LoadLocalTee8U32(arg) => executor.exec_load_local::<u8, 1, _, true, true>(arg.memory_arg_idx, arg.local1, arg.local2, i32::from)?,
            LoadLocalTee16S32(arg) => executor.exec_load_local::<i16, 2, _, true, true>(arg.memory_arg_idx, arg.local1, arg.local2, i32::from)?,
            LoadLocalTee16U32(arg) => executor.exec_load_local::<u16, 2, _, true, true>(arg.memory_arg_idx, arg.local1, arg.local2, i32::from)?,
            LoadLocalSet8S32(arg) => executor.exec_load_local::<i8, 1, _, true, false>(arg.memory_arg_idx, arg.local1, arg.local2, i32::from)?,
            LoadLocalSet8U32(arg) => executor.exec_load_local::<u8, 1, _, true, false>(arg.memory_arg_idx, arg.local1, arg.local2, i32::from)?,
            LoadLocalSet16S32(arg) => executor.exec_load_local::<i16, 2, _, true, false>(arg.memory_arg_idx, arg.local1, arg.local2, i32::from)?,
            LoadLocalSet16U32(arg) => executor.exec_load_local::<u16, 2, _, true, false>(arg.memory_arg_idx, arg.local1, arg.local2, i32::from)?,
            LoadLocalTee128(arg) => executor.exec_load_local::<Value128, 16, _, true, true>(arg.memory_arg_idx, arg.local1, arg.local2, identity)?,
            LoadLocalSet128(arg) => executor.exec_load_local::<Value128, 16, _, true, false>(arg.memory_arg_idx, arg.local1, arg.local2, identity)?,
            AndConstTee32(arg) => executor.exec_binop_const_tee::<Value32>(arg.value as u32, arg.local, BinOp::IAnd)?,
            SubConstTee32(arg) => executor.exec_binop_const_tee::<Value32>(arg.value as u32, arg.local, BinOp::ISub)?,
            AndConstTee64(packed) => executor.exec_binop_const_tee::<Value64>(packed.index.resolve(&executor.func.data).value() as u64, packed.op, BinOp::IAnd)?,
            SubConstTee64(packed) => executor.exec_binop_const_tee::<Value64>(packed.index.resolve(&executor.func.data).value() as u64, packed.op, BinOp::ISub)?,
            LocalTee32(local_index) => executor.exec_local_tee::<Value32>(*local_index),
            LocalTee64(local_index) => executor.exec_local_tee::<Value64>(*local_index),
            LocalTee128(local_index) => executor.exec_local_tee::<Value128>(*local_index),
            GlobalGet32(global_index) => executor.exec_global_get::<Value32>(*global_index)?,
            GlobalGet64(global_index) => executor.exec_global_get::<Value64>(*global_index)?,
            GlobalGet128(global_index) => executor.exec_global_get::<Value128>(*global_index)?,
            GlobalSet32(global_index) => executor.exec_global_set::<Value32>(*global_index),
            GlobalSet64(global_index) => executor.exec_global_set::<Value64>(*global_index),
            GlobalSet128(global_index) => executor.exec_global_set::<Value128>(*global_index),
            GlobalTee32(global_index) => executor.exec_global_tee::<Value32>(*global_index),
            GlobalTee64(global_index) => executor.exec_global_tee::<Value64>(*global_index),
            GlobalTee128(global_index) => executor.exec_global_tee::<Value128>(*global_index),
            Const32(val) => i32::stack_push(&mut executor.store.value_stack, *val)?,
            ConstPushAcc32(val) => {
                i32::stack_push(&mut executor.store.value_stack, *val)?;
                Value32::stack_push(&mut executor.store.value_stack, acc32)?;
            },
            Const64Imm(val) => i64::stack_push(&mut executor.store.value_stack, i64::from(*val))?,
            Const64(idx) => i64::stack_push(&mut executor.store.value_stack, idx.resolve(&executor.func.data).value())?,
            Const128Imm(val) => Value128::stack_push(&mut executor.store.value_stack, Value128(u128::from(*val).to_le_bytes()))?,
            I64Eqz => exec_op!(executor; unary i64 => i32, |v| i32::from(v == 0)),
            I32Eqz => exec_op!(executor; unary i32 => i32, |v| i32::from(v == 0)),
            I32Eq => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a == b)),
            I64Eq => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a == b)),
            F32Eq => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a == b)),
            F64Eq => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a == b)),
            I32Ne => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a != b)),
            I64Ne => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a != b)),
            F32Ne => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a != b)),
            F64Ne => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a != b)),
            I32LtS => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a < b)),
            I64LtS => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a < b)),
            I32LtU => exec_op!(executor; binary u32 => i32, |a, b| i32::from(a < b)),
            I64LtU => exec_op!(executor; binary u64 => i32, |a, b| i32::from(a < b)),
            F32Lt => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a < b)),
            F64Lt => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a < b)),
            I32LeS => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a <= b)),
            I64LeS => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a <= b)),
            I32LeU => exec_op!(executor; binary u32 => i32, |a, b| i32::from(a <= b)),
            I64LeU => exec_op!(executor; binary u64 => i32, |a, b| i32::from(a <= b)),
            F32Le => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a <= b)),
            F64Le => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a <= b)),
            I32GeS => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a >= b)),
            I64GeS => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a >= b)),
            I32GeU => exec_op!(executor; binary u32 => i32, |a, b| i32::from(a >= b)),
            I64GeU => exec_op!(executor; binary u64 => i32, |a, b| i32::from(a >= b)),
            F32Ge => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a >= b)),
            F64Ge => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a >= b)),
            I32GtS => exec_op!(executor; binary i32 => i32, |a, b| i32::from(a > b)),
            I64GtS => exec_op!(executor; binary i64 => i32, |a, b| i32::from(a > b)),
            I32GtU => exec_op!(executor; binary u32 => i32, |a, b| i32::from(a > b)),
            I64GtU => exec_op!(executor; binary u64 => i32, |a, b| i32::from(a > b)),
            F32Gt => exec_op!(executor; binary f32 => i32, |a, b| i32::from(a > b)),
            F64Gt => exec_op!(executor; binary f64 => i32, |a, b| i32::from(a > b)),
            I32Add => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_add(b)),
            I64Add => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_add(b)),
            F32Add => exec_op!(executor; binary f32 => f32, |a, b| a + b),
            F64Add => exec_op!(executor; binary f64 => f64, |a, b| a + b),
            I32Sub => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_sub(b)),
            I64Sub => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_sub(b)),
            F32Sub => exec_op!(executor; binary f32 => f32, |a, b| a - b),
            F64Sub => exec_op!(executor; binary f64 => f64, |a, b| a - b),
            F32Div => exec_op!(executor; binary f32 => f32, |a, b| a / b),
            F64Div => exec_op!(executor; binary f64 => f64, |a, b| a / b),
            I32Mul => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_mul(b)),
            I64Mul => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_mul(b)),
            F32Mul => exec_op!(executor; binary f32 => f32, |a, b| a * b),
            F64Mul => exec_op!(executor; binary f64 => f64, |a, b| a * b),
            I32DivS => exec_op!(executor; binary_fallible i32, |a, b| a.tw_checked_div(b)),
            I64DivS => exec_op!(executor; binary_fallible i64, |a, b| a.tw_checked_div(b)),
            I32DivU => exec_op!(executor; binary_fallible u32, |a, b| a.checked_div(b).ok_or(Trap::DivisionByZero)),
            I64DivU => exec_op!(executor; binary_fallible u64, |a, b| a.checked_div(b).ok_or(Trap::DivisionByZero)),
            I32RemS => exec_op!(executor; binary_fallible i32, |a, b| a.tw_checked_wrapping_rem(b)),
            I64RemS => exec_op!(executor; binary_fallible i64, |a, b| a.tw_checked_wrapping_rem(b)),
            I32RemU => exec_op!(executor; binary_fallible u32, |a, b| a.tw_checked_wrapping_rem(b)),
            I64RemU => exec_op!(executor; binary_fallible u64, |a, b| a.tw_checked_wrapping_rem(b)),
            I32And => exec_op!(executor; binary i32 => i32, |a, b| a & b),
            I64And => exec_op!(executor; binary i64 => i64, |a, b| a & b),
            I32Or => exec_op!(executor; binary i32 => i32, |a, b| a | b),
            I64Or => exec_op!(executor; binary i64 => i64, |a, b| a | b),
            I32Xor => exec_op!(executor; binary i32 => i32, |a, b| a ^ b),
            I64Xor => exec_op!(executor; binary i64 => i64, |a, b| a ^ b),
            I32Shl => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_shl(b as u32)),
            I64Shl => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_shl(b as u32)),
            I32ShrS => exec_op!(executor; binary i32 => i32, |a, b| a.wrapping_shr(b as u32)),
            I64ShrS => exec_op!(executor; binary i64 => i64, |a, b| a.wrapping_shr(b as u32)),
            I32ShrU => exec_op!(executor; binary u32 => u32, |a, b| a.wrapping_shr(b)),
            I64ShrU => exec_op!(executor; binary u64 => u64, |a, b| a.wrapping_shr(b as u32)),
            I32Rotl => exec_op!(executor; binary i32 => i32, |a, b| a.rotate_left(b as u32)),
            I64Rotl => exec_op!(executor; binary i64 => i64, |a, b| a.rotate_left(b as u32)),
            I32Rotr => exec_op!(executor; binary i32 => i32, |a, b| a.rotate_right(b as u32)),
            I64Rotr => exec_op!(executor; binary i64 => i64, |a, b| a.rotate_right(b as u32)),
            I64Add128 => executor.exec_i64_add128()?,
            I64Sub128 => executor.exec_i64_sub128()?,
            I64MulWideS => executor.exec_i64_mul_wide_s()?,
            I64MulWideU => executor.exec_i64_mul_wide_u()?,
            I32Clz => exec_op!(executor; unary i32 => i32, |v| v.leading_zeros() as i32),
            I64Clz => exec_op!(executor; unary i64 => i64, |v| i64::from(v.leading_zeros())),
            I32Ctz => exec_op!(executor; unary i32 => i32, |v| v.trailing_zeros() as i32),
            I64Ctz => exec_op!(executor; unary i64 => i64, |v| i64::from(v.trailing_zeros())),
            I32Popcnt => exec_op!(executor; unary i32 => i32, |v| v.count_ones() as i32),
            I64Popcnt => exec_op!(executor; unary i64 => i64, |v| i64::from(v.count_ones())),

            // Reference types
            RefFunc(func_idx) => ValueRef::stack_push(&mut executor.store.value_stack, ValueRef::from_category_addr(executor.module.resolve_func_addr(*func_idx)))?,
            RefNull(_) => ValueRef::stack_push(&mut executor.store.value_stack, ValueRef::NULL)?,
            RefIsNull => executor.exec_ref_is_null()?,
            AccRefIsNull => {
                let reference = core::mem::replace(&mut acc_ref, ValueRef::NULL);
                acc32 = u32::from(reference.is_null());
            },
            AccRefIsNullStack => acc32 = u32::from(ValueRef::stack_pop(&mut executor.store.value_stack).is_null()),
            RefAsNonNull => executor.exec_ref_as_non_null()?,
            RefI31 => exec_op!(executor; unary i32 => ValueRef, |v| ValueRef::from_i31(v)),
            I31GetS => executor.exec_i31_get(true)?,
            AccI31GetS => acc32 = executor.i31_get(true)?,
            I31GetU => executor.exec_i31_get(false)?,
            AccI31GetU => acc32 = executor.i31_get(false)?,
            RefEq => exec_op!(executor; binary ValueRef => i32, |a, b| i32::from(a == b)),
            AccRefEq => {
                let rhs = ValueRef::stack_pop(&mut executor.store.value_stack);
                let lhs = ValueRef::stack_pop(&mut executor.store.value_stack);
                acc32 = u32::from(lhs == rhs);
            },
            RefTest(ty) => executor.exec_ref_test(*ty)?,
            AccRefTest(ty) => acc32 = u32::from(executor.ref_test(*ty)),
            RefCast(ty) => executor.exec_ref_cast(*ty)?,

            // GC objects
            StructNew(ty) => executor.exec_struct_new(*ty, false, acc_ref)?,
            StructNewDefault(ty) => executor.exec_struct_new(*ty, true, acc_ref)?,
            StructGet(idx) => executor.exec_struct_get(*idx, None)?,
            StructGetS(idx) => executor.exec_struct_get(*idx, Some(true))?,
            StructGetU(idx) => executor.exec_struct_get(*idx, Some(false))?,
            StructSet(idx) => executor.exec_struct_set(*idx)?,
            ArrayNew(ty) => executor.exec_array_new(*ty, false, acc_ref)?,
            ArrayNewDefault(ty) => executor.exec_array_new(*ty, true, acc_ref)?,
            ArrayNewFixed(idx) => executor.exec_array_new_fixed(*idx, acc_ref)?,
            ArrayNewData(idx) => executor.exec_array_new_data(*idx, acc_ref)?,
            ArrayNewElem(idx) => executor.exec_array_new_elem(*idx, acc_ref)?,
            ArrayGet(ty) => executor.exec_array_get(*ty, None)?,
            ArrayGetS(ty) => executor.exec_array_get(*ty, Some(true))?,
            ArrayGetU(ty) => executor.exec_array_get(*ty, Some(false))?,
            ArraySet(ty) => executor.exec_array_set(*ty)?,
            ArrayLen => executor.exec_array_len()?,
            AccArrayLen => acc32 = executor.array_len()?,
            ArrayFill(ty) => executor.exec_array_fill(*ty)?,
            ArrayCopy(idx) => executor.exec_array_copy(*idx)?,
            ArrayInitData(idx) => executor.exec_array_init_data(*idx)?,
            ArrayInitElem(idx) => executor.exec_array_init_elem(*idx)?,
            MemorySize(addr) => executor.exec_memory_size(*addr)?,
            MemoryGrow(addr) => executor.exec_memory_grow(*addr)?,
            AccMemoryGrow32(addr) => acc32 = executor.memory_grow(*addr, i64::from(acc32 as i32))? as u32,
            AccMemoryGrowStack32(addr) => {
                let delta = Value32::stack_pop(&mut executor.store.value_stack);
                acc32 = executor.memory_grow(*addr, i64::from(delta as i32))? as u32;
            },
            AccMemoryGrow64(addr) => acc64 = executor.memory_grow(*addr, acc64 as i64)? as u64,
            AccMemoryGrowStack64(addr) => {
                let delta = Value64::stack_pop(&mut executor.store.value_stack);
                acc64 = executor.memory_grow(*addr, delta as i64)? as u64;
            },

            // Bulk memory operations
            MemoryCopy(idx) => executor.exec_memory_copy(*idx)?,
            MemoryFill(addr) => executor.exec_memory_fill(*addr)?,
            MemoryFillConst(idx) => executor.exec_memory_fill_const(*idx)?,
            MemoryInit(idx) => executor.exec_memory_init(*idx)?,
            DataDrop(data_index) => executor.store.state.get_data_mut(executor.module.resolve_data_addr(*data_index)).drop(),
            ElemDrop(elem_index) => executor.store.state.get_elem_mut(executor.module.resolve_elem_addr(*elem_index)).drop(),

            // Table instructions
            TableGet(table_idx) => executor.exec_table_get(*table_idx)?,
            TableSet(table_idx) => executor.exec_table_set(*table_idx)?,
            TableSize(table_idx) => executor.exec_table_size(*table_idx)?,
            TableInit(idx) => executor.exec_table_init(*idx)?,
            TableGrow(table_idx) => executor.exec_table_grow(*table_idx)?,
            AccTableGrow32(table_idx) => {
                let delta = Executor::table_operand(u64::from(acc32))?;
                acc32 = executor.table_grow(*table_idx, delta)? as u32;
            },
            AccTableGrowStack32(table_idx) => {
                let delta = Value32::stack_pop(&mut executor.store.value_stack);
                let delta = Executor::table_operand(u64::from(delta))?;
                acc32 = executor.table_grow(*table_idx, delta)? as u32;
            },
            AccTableGrow64(table_idx) => {
                let delta = Executor::table_operand(acc64)?;
                acc64 = executor.table_grow(*table_idx, delta)?;
            },
            AccTableGrowStack64(table_idx) => {
                let delta = Value64::stack_pop(&mut executor.store.value_stack);
                let delta = Executor::table_operand(delta)?;
                acc64 = executor.table_grow(*table_idx, delta)?;
            },
            TableFill(table_idx) => executor.exec_table_fill(*table_idx)?,
            TableCopy(idx) => executor.exec_table_copy(*idx)?,

            // Core memory load/store operations
            I32Store(idx) => executor.exec_mem_store::<i32, i32, 4>(idx.resolve(&executor.func.data), identity)?,
            I64Store(idx) => executor.exec_mem_store::<i64, i64, 8>(idx.resolve(&executor.func.data), identity)?,
            F32Store(idx) => executor.exec_mem_store::<f32, f32, 4>(idx.resolve(&executor.func.data), identity)?,
            F64Store(idx) => executor.exec_mem_store::<f64, f64, 8>(idx.resolve(&executor.func.data), identity)?,
            FMaStoreF32(m) => executor.exec_fma_store::<f32, 4>(*m)?,
            FMaStoreF64(m) => executor.exec_fma_store::<f64, 8>(*m)?,
            I32Store8(idx) => executor.exec_mem_store::<i32, i8, 1>(idx.resolve(&executor.func.data), #[inline(always)] |v| v as i8)?,
            I32Store16(idx) => executor.exec_mem_store::<i32, i16, 2>(idx.resolve(&executor.func.data), #[inline(always)] |v| v as i16)?,
            I64Store8(idx) => executor.exec_mem_store::<i64, i8, 1>(idx.resolve(&executor.func.data), #[inline(always)] |v| v as i8)?,
            I64Store16(idx) => executor.exec_mem_store::<i64, i16, 2>(idx.resolve(&executor.func.data), #[inline(always)] |v| v as i16)?,
            I64Store32(idx) => executor.exec_mem_store::<i64, i32, 4>(idx.resolve(&executor.func.data), #[inline(always)] |v| v as i32)?,
            I32Load(idx) => executor.exec_mem_load::<i32, 4, _>(idx.resolve(&executor.func.data), identity)?,
            I64Load(idx) => executor.exec_mem_load::<i64, 8, _>(idx.resolve(&executor.func.data), identity)?,
            F32Load(idx) => executor.exec_mem_load::<f32, 4, _>(idx.resolve(&executor.func.data), identity)?,
            F64Load(idx) => executor.exec_mem_load::<f64, 8, _>(idx.resolve(&executor.func.data), identity)?,
            I32Load8S(idx) => executor.exec_mem_load::<i8, 1, _>(idx.resolve(&executor.func.data), i32::from)?,
            I32Load8U(idx) => executor.exec_mem_load::<u8, 1, _>(idx.resolve(&executor.func.data), i32::from)?,
            I32Load16S(idx) => executor.exec_mem_load::<i16, 2, _>(idx.resolve(&executor.func.data), i32::from)?,
            I32Load16U(idx) => executor.exec_mem_load::<u16, 2, _>(idx.resolve(&executor.func.data), i32::from)?,
            I64Load8S(idx) => executor.exec_mem_load::<i8, 1, _>(idx.resolve(&executor.func.data), i64::from)?,
            I64Load8U(idx) => executor.exec_mem_load::<u8, 1, _>(idx.resolve(&executor.func.data), i64::from)?,
            I64Load16S(idx) => executor.exec_mem_load::<i16, 2, _>(idx.resolve(&executor.func.data), i64::from)?,
            I64Load16U(idx) => executor.exec_mem_load::<u16, 2, _>(idx.resolve(&executor.func.data), i64::from)?,
            I64Load32S(idx) => executor.exec_mem_load::<i32, 4, _>(idx.resolve(&executor.func.data), i64::from)?,
            I64Load32U(idx) => executor.exec_mem_load::<u32, 4, _>(idx.resolve(&executor.func.data), i64::from)?,

            // Numeric conversion operations
            F32ConvertI32S => exec_op!(executor; unary i32 => f32, |v| v as f32),
            F32ConvertI64S => exec_op!(executor; unary i64 => f32, |v| v as f32),
            F64ConvertI32S => exec_op!(executor; unary i32 => f64, |v| f64::from(v)),
            F64ConvertI64S => exec_op!(executor; unary i64 => f64, |v| v as f64),
            F32ConvertI32U => exec_op!(executor; unary u32 => f32, |v| v as f32),
            F32ConvertI64U => exec_op!(executor; unary u64 => f32, |v| v as f32),
            F64ConvertI32U => exec_op!(executor; unary u32 => f64, |v| f64::from(v)),
            F64ConvertI64U => exec_op!(executor; unary u64 => f64, |v| v as f64),

            // Sign-extension operations
            I32Extend8S => exec_op!(executor; unary i32 => i32, |v| i32::from(v as i8)),
            I32Extend16S => exec_op!(executor; unary i32 => i32, |v| i32::from(v as i16)),
            I64Extend8S => exec_op!(executor; unary i64 => i64, |v| i64::from(v as i8)),
            I64Extend16S => exec_op!(executor; unary i64 => i64, |v| i64::from(v as i16)),
            I64Extend32S => exec_op!(executor; unary i64 => i64, |v| i64::from(v as i32)),
            I64ExtendI32U => exec_op!(executor; unary u32 => i64, |v| i64::from(v)),
            I64ExtendI32S => exec_op!(executor; unary i32 => i64, |v| i64::from(v)),
            I32WrapI64 => exec_op!(executor; unary i64 => i32, |v| v as i32),
            F32DemoteF64 => exec_op!(executor; unary f64 => f32, |v| v as f32),
            F64PromoteF32 => exec_op!(executor; unary f32 => f64, |v| f64::from(v)),
            F32Abs => exec_op!(executor; unary f32 => f32, |v| v.abs()),
            F64Abs => exec_op!(executor; unary f64 => f64, |v| v.abs()),
            F32Neg => exec_op!(executor; unary f32 => f32, |v| -v),
            F64Neg => exec_op!(executor; unary f64 => f64, |v| -v),
            F32Ceil => exec_op!(executor; unary f32 => f32, |v| v.ceil()),
            F64Ceil => exec_op!(executor; unary f64 => f64, |v| v.ceil()),
            F32Floor => exec_op!(executor; unary f32 => f32, |v| v.floor()),
            F64Floor => exec_op!(executor; unary f64 => f64, |v| v.floor()),
            F32Trunc => exec_op!(executor; unary f32 => f32, |v| v.trunc()),
            F64Trunc => exec_op!(executor; unary f64 => f64, |v| v.trunc()),
            F32Nearest => exec_op!(executor; unary f32 => f32, |v| v.tw_nearest()),
            F64Nearest => exec_op!(executor; unary f64 => f64, |v| v.tw_nearest()),
            F32Sqrt => exec_op!(executor; unary f32 => f32, |v| v.sqrt()),
            F64Sqrt => exec_op!(executor; unary f64 => f64, |v| v.sqrt()),
            F32Min => exec_op!(executor; binary f32 => f32, |a, b| a.tw_minimum(b)),
            F64Min => exec_op!(executor; binary f64 => f64, |a, b| a.tw_minimum(b)),
            F32Max => exec_op!(executor; binary f32 => f32, |a, b| a.tw_maximum(b)),
            F64Max => exec_op!(executor; binary f64 => f64, |a, b| a.tw_maximum(b)),
            F32Copysign => exec_op!(executor; binary f32 => f32, |a, b| a.copysign(b)),
            F64Copysign => exec_op!(executor; binary f64 => f64, |a, b| a.copysign(b)),
            I32TruncF32S => checked_conv_float!(f32, i32, executor),
            I32TruncF64S => checked_conv_float!(f64, i32, executor),
            I32TruncF32U => checked_conv_float!(f32, u32, i32, executor),
            I32TruncF64U => checked_conv_float!(f64, u32, i32, executor),
            I64TruncF32S => checked_conv_float!(f32, i64, executor),
            I64TruncF64S => checked_conv_float!(f64, i64, executor),
            I64TruncF32U => checked_conv_float!(f32, u64, i64, executor),
            I64TruncF64U => checked_conv_float!(f64, u64, i64, executor),

            // Non-trapping float-to-int conversions
            I32TruncSatF32S => exec_op!(executor; unary f32 => i32, |v| v.trunc() as i32),
            I32TruncSatF32U => exec_op!(executor; unary f32 => u32, |v| v.trunc() as u32),
            I32TruncSatF64S => exec_op!(executor; unary f64 => i32, |v| v.trunc() as i32),
            I32TruncSatF64U => exec_op!(executor; unary f64 => u32, |v| v.trunc() as u32),
            I64TruncSatF32S => exec_op!(executor; unary f32 => i64, |v| v.trunc() as i64),
            I64TruncSatF32U => exec_op!(executor; unary f32 => u64, |v| v.trunc() as u64),
            I64TruncSatF64S => exec_op!(executor; unary f64 => i64, |v| v.trunc() as i64),
            I64TruncSatF64U => exec_op!(executor; unary f64 => u64, |v| v.trunc() as u64),

            // SIMD extension
            V128Not => exec_op!(executor; unary Value128 => Value128, |v| v.v128_not()),
            V128And => exec_op!(executor; binary Value128 => Value128, |a, b| a.v128_and(b)),
            V128AndNot => exec_op!(executor; binary Value128 => Value128, |a, b| a.v128_andnot(b)),
            V128Or => exec_op!(executor; binary Value128 => Value128, |a, b| a.v128_or(b)),
            V128Xor => exec_op!(executor; binary Value128 => Value128, |a, b| a.v128_xor(b)),
            V128Bitselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::v128_bitselect(a, b, c)),
            V128AnyTrue => exec_op!(executor; unary Value128 => i32, |v| v.v128_any_true() as i32),
            I8x16Swizzle => exec_op!(executor; binary Value128 => Value128, |a, s| a.i8x16_swizzle(s)),
            I8x16RelaxedSwizzle => exec_op!(executor; binary Value128 => Value128, |a, s| a.i8x16_relaxed_swizzle(s)),
            V128Load(idx) => executor.exec_mem_load::<Value128, 16, _>(idx.resolve(&executor.func.data), identity)?,
            V128Load8x8S(idx) => executor.exec_mem_load::<u64, 8, Value128>(idx.resolve(&executor.func.data), |v| Value128::v128_load8x8_s(v.to_le_bytes()))?,
            V128Load8x8U(idx) => executor.exec_mem_load::<u64, 8, Value128>(idx.resolve(&executor.func.data), |v| Value128::v128_load8x8_u(v.to_le_bytes()))?,
            V128Load16x4S(idx) => executor.exec_mem_load::<u64, 8, Value128>(idx.resolve(&executor.func.data), |v| Value128::v128_load16x4_s(v.to_le_bytes()))?,
            V128Load16x4U(idx) => executor.exec_mem_load::<u64, 8, Value128>(idx.resolve(&executor.func.data), |v| Value128::v128_load16x4_u(v.to_le_bytes()))?,
            V128Load32x2S(idx) => executor.exec_mem_load::<u64, 8, Value128>(idx.resolve(&executor.func.data), |v| Value128::v128_load32x2_s(v.to_le_bytes()))?,
            V128Load32x2U(idx) => executor.exec_mem_load::<u64, 8, Value128>(idx.resolve(&executor.func.data), |v| Value128::v128_load32x2_u(v.to_le_bytes()))?,
            V128Load8Splat(idx) => executor.exec_mem_load::<i8, 1, Value128>(idx.resolve(&executor.func.data), Value128::splat_i8)?,
            V128Load16Splat(idx) => executor.exec_mem_load::<i16, 2, Value128>(idx.resolve(&executor.func.data), Value128::splat_i16)?,
            V128Load32Splat(idx) => executor.exec_mem_load::<i32, 4, Value128>(idx.resolve(&executor.func.data), Value128::splat_i32)?,
            V128Load64Splat(idx) => executor.exec_mem_load::<i64, 8, Value128>(idx.resolve(&executor.func.data), Value128::splat_i64)?,
            V128Store(idx) => executor.exec_mem_store::<Value128, Value128, 16>(idx.resolve(&executor.func.data), identity)?,
            V128Store8Lane(arg) => executor.exec_mem_store_lane::<i8, 1>(*arg)?,
            V128Store16Lane(arg) => executor.exec_mem_store_lane::<i16, 2>(*arg)?,
            V128Store32Lane(arg) => executor.exec_mem_store_lane::<i32, 4>(*arg)?,
            V128Store64Lane(arg) => executor.exec_mem_store_lane::<i64, 8>(*arg)?,
            V128Load32Zero(idx) => executor.exec_mem_load::<i32, 4, Value128>(idx.resolve(&executor.func.data), |v| Value128::from_i32x4([v, 0, 0, 0]))?,
            V128Load64Zero(idx) => executor.exec_mem_load::<i64, 8, Value128>(idx.resolve(&executor.func.data), |v| Value128::from_i64x2([v, 0]))?,
            Const128(arg) => Value128::stack_push(&mut executor.store.value_stack, Value128(arg.resolve(&executor.func.data).value()))?,
            I8x16ExtractLaneS(lane) => executor.exec_simd_extract_lane::<i32>(*lane, |v, lane| v.extract_lane_i8(lane) as i32)?,
            I8x16ExtractLaneU(lane) => executor.exec_simd_extract_lane::<i32>(*lane, |v, lane| v.extract_lane_u8(lane) as i32)?,
            I16x8ExtractLaneS(lane) => executor.exec_simd_extract_lane::<i32>(*lane, |v, lane| v.extract_lane_i16(lane) as i32)?,
            I16x8ExtractLaneU(lane) => executor.exec_simd_extract_lane::<i32>(*lane, |v, lane| v.extract_lane_u16(lane) as i32)?,
            I32x4ExtractLane(lane) => executor.exec_simd_extract_lane::<i32>(*lane, Value128::extract_lane_i32)?,
            I64x2ExtractLane(lane) => executor.exec_simd_extract_lane::<i64>(*lane, Value128::extract_lane_i64)?,
            F32x4ExtractLane(lane) => executor.exec_simd_extract_lane::<f32>(*lane, Value128::extract_lane_f32)?,
            F64x2ExtractLane(lane) => executor.exec_simd_extract_lane::<f64>(*lane, Value128::extract_lane_f64)?,
            V128Load8Lane(arg) => executor.exec_mem_load_lane::<i8, 1>(*arg)?,
            V128Load16Lane(arg) => executor.exec_mem_load_lane::<i16, 2>(*arg)?,
            V128Load32Lane(arg) => executor.exec_mem_load_lane::<i32, 4>(*arg)?,
            V128Load64Lane(arg) => executor.exec_mem_load_lane::<i64, 8>(*arg)?,
            I8x16ReplaceLane(lane) => executor.exec_simd_replace_lane::<i32>(*lane, |value, vector, lane| vector.i8x16_replace_lane(lane, value as i8))?,
            I16x8ReplaceLane(lane) => executor.exec_simd_replace_lane::<i32>(*lane, |value, vector, lane| vector.i16x8_replace_lane(lane, value as i16))?,
            I32x4ReplaceLane(lane) => executor.exec_simd_replace_lane::<i32>(*lane, |value, vector, lane| vector.i32x4_replace_lane(lane, value))?,
            I64x2ReplaceLane(lane) => executor.exec_simd_replace_lane::<i64>(*lane, |value, vector, lane| vector.i64x2_replace_lane(lane, value))?,
            F32x4ReplaceLane(lane) => executor.exec_simd_replace_lane::<f32>(*lane, |value, vector, lane| vector.f32x4_replace_lane(lane, value))?,
            F64x2ReplaceLane(lane) => executor.exec_simd_replace_lane::<f64>(*lane, |value, vector, lane| vector.f64x2_replace_lane(lane, value))?,
            I8x16Splat => exec_op!(executor; unary i32 => Value128, |v| Value128::splat_i8(v as i8)),
            I16x8Splat => exec_op!(executor; unary i32 => Value128, |v| Value128::splat_i16(v as i16)),
            I32x4Splat => exec_op!(executor; unary i32 => Value128, |v| Value128::splat_i32(v)),
            I64x2Splat => exec_op!(executor; unary i64 => Value128, |v| Value128::splat_i64(v)),
            F32x4Splat => exec_op!(executor; unary f32 => Value128, |v| Value128::splat_f32(v)),
            F64x2Splat => exec_op!(executor; unary f64 => Value128, |v| Value128::splat_f64(v)),
            I8x16Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_eq(b)),
            I16x8Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_eq(b)),
            I32x4Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_eq(b)),
            I64x2Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_eq(b)),
            F32x4Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_eq(b)),
            F64x2Eq => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_eq(b)),
            I8x16Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_ne(b)),
            I16x8Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_ne(b)),
            I32x4Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_ne(b)),
            I64x2Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_ne(b)),
            F32x4Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_ne(b)),
            F64x2Ne => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_ne(b)),
            I8x16LtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_lt_s(b)),
            I16x8LtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_lt_s(b)),
            I32x4LtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_lt_s(b)),
            I64x2LtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_lt_s(b)),
            I8x16LtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_lt_u(b)),
            I16x8LtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_lt_u(b)),
            I32x4LtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_lt_u(b)),
            F32x4Lt => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_lt(b)),
            F64x2Lt => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_lt(b)),
            F32x4Gt => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_gt(b)),
            F64x2Gt => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_gt(b)),
            I8x16GtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_gt_s(b)),
            I16x8GtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_gt_s(b)),
            I32x4GtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_gt_s(b)),
            I64x2GtS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_gt_s(b)),
            I64x2LeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_le_s(b)),
            F32x4Le => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_le(b)),
            F64x2Le => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_le(b)),
            I8x16GtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_gt_u(b)),
            I16x8GtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_gt_u(b)),
            I32x4GtU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_gt_u(b)),
            F32x4Ge => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_ge(b)),
            F64x2Ge => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_ge(b)),
            I8x16LeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_le_s(b)),
            I16x8LeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_le_s(b)),
            I32x4LeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_le_s(b)),
            I8x16LeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_le_u(b)),
            I16x8LeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_le_u(b)),
            I32x4LeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_le_u(b)),
            I8x16GeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_ge_s(b)),
            I16x8GeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_ge_s(b)),
            I32x4GeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_ge_s(b)),
            I64x2GeS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_ge_s(b)),
            I8x16GeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_ge_u(b)),
            I16x8GeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_ge_u(b)),
            I32x4GeU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_ge_u(b)),
            I8x16Abs => exec_op!(executor; unary Value128 => Value128, |a| a.i8x16_abs()),
            I16x8Abs => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_abs()),
            I32x4Abs => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_abs()),
            I64x2Abs => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_abs()),
            I8x16Neg => exec_op!(executor; unary Value128 => Value128, |a| a.i8x16_neg()),
            I16x8Neg => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_neg()),
            I32x4Neg => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_neg()),
            I64x2Neg => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_neg()),
            I8x16AllTrue => exec_op!(executor; unary Value128 => i32, |v| v.i8x16_all_true() as i32),
            I16x8AllTrue => exec_op!(executor; unary Value128 => i32, |v| v.i16x8_all_true() as i32),
            I32x4AllTrue => exec_op!(executor; unary Value128 => i32, |v| v.i32x4_all_true() as i32),
            I64x2AllTrue => exec_op!(executor; unary Value128 => i32, |v| v.i64x2_all_true() as i32),
            I8x16Bitmask => exec_op!(executor; unary Value128 => i32, |v| v.i8x16_bitmask() as i32),
            I16x8Bitmask => exec_op!(executor; unary Value128 => i32, |v| v.i16x8_bitmask() as i32),
            I32x4Bitmask => exec_op!(executor; unary Value128 => i32, |v| v.i32x4_bitmask() as i32),
            I64x2Bitmask => exec_op!(executor; unary Value128 => i32, |v| v.i64x2_bitmask() as i32),
            I8x16Shl => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i8x16_shl(a as u32)),
            I16x8Shl => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i16x8_shl(a as u32)),
            I32x4Shl => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i32x4_shl(a as u32)),
            I64x2Shl => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i64x2_shl(a as u32)),
            I8x16ShrS => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i8x16_shr_s(a as u32)),
            I16x8ShrS => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i16x8_shr_s(a as u32)),
            I32x4ShrS => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i32x4_shr_s(a as u32)),
            I64x2ShrS => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i64x2_shr_s(a as u32)),
            I8x16ShrU => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i8x16_shr_u(a as u32)),
            I16x8ShrU => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i16x8_shr_u(a as u32)),
            I32x4ShrU => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i32x4_shr_u(a as u32)),
            I64x2ShrU => exec_op!(executor; binary i32, Value128 => Value128, |a, b| b.i64x2_shr_u(a as u32)),
            I8x16Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_add(b)),
            I16x8Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_add(b)),
            I32x4Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_add(b)),
            I64x2Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_add(b)),
            I8x16Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_sub(b)),
            I16x8Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_sub(b)),
            I32x4Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_sub(b)),
            I64x2Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_sub(b)),
            I8x16MinS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_min_s(b)),
            I16x8MinS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_min_s(b)),
            I32x4MinS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_min_s(b)),
            I8x16MinU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_min_u(b)),
            I16x8MinU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_min_u(b)),
            I32x4MinU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_min_u(b)),
            I8x16MaxS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_max_s(b)),
            I16x8MaxS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_max_s(b)),
            I32x4MaxS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_max_s(b)),
            I8x16MaxU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_max_u(b)),
            I16x8MaxU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_max_u(b)),
            I32x4MaxU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_max_u(b)),
            I64x2Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_mul(b)),
            I16x8Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_mul(b)),
            I32x4Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_mul(b)),
            I8x16NarrowI16x8S => exec_op!(executor; binary Value128 => Value128, |a, b| Value128::i8x16_narrow_i16x8_s(a, b)),
            I8x16NarrowI16x8U => exec_op!(executor; binary Value128 => Value128, |a, b| Value128::i8x16_narrow_i16x8_u(a, b)),
            I16x8NarrowI32x4S => exec_op!(executor; binary Value128 => Value128, |a, b| Value128::i16x8_narrow_i32x4_s(a, b)),
            I16x8NarrowI32x4U => exec_op!(executor; binary Value128 => Value128, |a, b| Value128::i16x8_narrow_i32x4_u(a, b)),
            I8x16AddSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_add_sat_s(b)),
            I16x8AddSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_add_sat_s(b)),
            I8x16AddSatU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_add_sat_u(b)),
            I16x8AddSatU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_add_sat_u(b)),
            I8x16SubSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_sub_sat_s(b)),
            I16x8SubSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_sub_sat_s(b)),
            I8x16SubSatU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_sub_sat_u(b)),
            I16x8SubSatU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_sub_sat_u(b)),
            I8x16AvgrU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i8x16_avgr_u(b)),
            I16x8AvgrU => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_avgr_u(b)),
            I16x8ExtAddPairwiseI8x16S => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extadd_pairwise_i8x16_s()),
            I16x8ExtAddPairwiseI8x16U => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extadd_pairwise_i8x16_u()),
            I32x4ExtAddPairwiseI16x8S => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extadd_pairwise_i16x8_s()),
            I32x4ExtAddPairwiseI16x8U => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extadd_pairwise_i16x8_u()),
            I16x8ExtMulLowI8x16S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_extmul_low_i8x16_s(b)),
            I16x8ExtMulLowI8x16U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_extmul_low_i8x16_u(b)),
            I16x8ExtMulHighI8x16S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_extmul_high_i8x16_s(b)),
            I16x8ExtMulHighI8x16U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_extmul_high_i8x16_u(b)),
            I32x4ExtMulLowI16x8S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_extmul_low_i16x8_s(b)),
            I32x4ExtMulLowI16x8U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_extmul_low_i16x8_u(b)),
            I32x4ExtMulHighI16x8S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_extmul_high_i16x8_s(b)),
            I32x4ExtMulHighI16x8U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_extmul_high_i16x8_u(b)),
            I64x2ExtMulLowI32x4S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_extmul_low_i32x4_s(b)),
            I64x2ExtMulLowI32x4U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_extmul_low_i32x4_u(b)),
            I64x2ExtMulHighI32x4S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_extmul_high_i32x4_s(b)),
            I64x2ExtMulHighI32x4U => exec_op!(executor; binary Value128 => Value128, |a, b| a.i64x2_extmul_high_i32x4_u(b)),
            I16x8ExtendLowI8x16S => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extend_low_i8x16_s()),
            I16x8ExtendLowI8x16U => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extend_low_i8x16_u()),
            I16x8ExtendHighI8x16S => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extend_high_i8x16_s()),
            I16x8ExtendHighI8x16U => exec_op!(executor; unary Value128 => Value128, |a| a.i16x8_extend_high_i8x16_u()),
            I32x4ExtendLowI16x8S => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extend_low_i16x8_s()),
            I32x4ExtendLowI16x8U => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extend_low_i16x8_u()),
            I32x4ExtendHighI16x8S => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extend_high_i16x8_s()),
            I32x4ExtendHighI16x8U => exec_op!(executor; unary Value128 => Value128, |a| a.i32x4_extend_high_i16x8_u()),
            I64x2ExtendLowI32x4S => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_extend_low_i32x4_s()),
            I64x2ExtendLowI32x4U => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_extend_low_i32x4_u()),
            I64x2ExtendHighI32x4S => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_extend_high_i32x4_s()),
            I64x2ExtendHighI32x4U => exec_op!(executor; unary Value128 => Value128, |a| a.i64x2_extend_high_i32x4_u()),
            I8x16Popcnt => exec_op!(executor; unary Value128 => Value128, |v| v.i8x16_popcnt()),
            I8x16Shuffle(idx) => executor.exec_simd_shuffle(Value128(idx.resolve(&executor.func.data).value()))?,
            I16x8Q15MulrSatS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_q15mulr_sat_s(b)),
            I32x4DotI16x8S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i32x4_dot_i16x8_s(b)),
            I8x16RelaxedLaneselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::i8x16_relaxed_laneselect(a, b, c)),
            I16x8RelaxedLaneselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::i16x8_relaxed_laneselect(a, b, c)),
            I32x4RelaxedLaneselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::i32x4_relaxed_laneselect(a, b, c)),
            I64x2RelaxedLaneselect => exec_op!(executor; ternary Value128 => Value128, |a, b, c| Value128::i64x2_relaxed_laneselect(a, b, c)),
            I16x8RelaxedQ15mulrS => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_relaxed_q15mulr_s(b)),
            I16x8RelaxedDotI8x16I7x16S => exec_op!(executor; binary Value128 => Value128, |a, b| a.i16x8_relaxed_dot_i8x16_i7x16_s(b)),
            I32x4RelaxedDotI8x16I7x16AddS => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.i32x4_relaxed_dot_i8x16_i7x16_add_s(b, c)),
            F32x4Ceil => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_ceil()),
            F64x2Ceil => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_ceil()),
            F32x4Floor => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_floor()),
            F64x2Floor => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_floor()),
            F32x4Trunc => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_trunc()),
            F64x2Trunc => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_trunc()),
            F32x4Nearest => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_nearest()),
            F64x2Nearest => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_nearest()),
            F32x4Abs => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_abs()),
            F64x2Abs => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_abs()),
            F32x4Neg => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_neg()),
            F64x2Neg => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_neg()),
            F32x4Sqrt => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_sqrt()),
            F64x2Sqrt => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_sqrt()),
            F32x4Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_add(b)),
            F64x2Add => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_add(b)),
            F32x4Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_sub(b)),
            F64x2Sub => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_sub(b)),
            F32x4Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_mul(b)),
            F64x2Mul => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_mul(b)),
            F32x4Div => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_div(b)),
            F64x2Div => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_div(b)),
            F32x4Min => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_min(b)),
            F64x2Min => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_min(b)),
            F32x4Max => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_max(b)),
            F64x2Max => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_max(b)),
            F32x4PMin => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_pmin(b)),
            F32x4PMax => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_pmax(b)),
            F64x2PMin => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_pmin(b)),
            F64x2PMax => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_pmax(b)),
            F32x4RelaxedMadd => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.f32x4_relaxed_madd(b, c)),
            F32x4RelaxedNmadd => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.f32x4_relaxed_nmadd(b, c)),
            F64x2RelaxedMadd => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.f64x2_relaxed_madd(b, c)),
            F64x2RelaxedNmadd => exec_op!(executor; ternary Value128 => Value128, |a, b, c| a.f64x2_relaxed_nmadd(b, c)),
            F32x4RelaxedMin => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_relaxed_min(b)),
            F32x4RelaxedMax => exec_op!(executor; binary Value128 => Value128, |a, b| a.f32x4_relaxed_max(b)),
            F64x2RelaxedMin => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_relaxed_min(b)),
            F64x2RelaxedMax => exec_op!(executor; binary Value128 => Value128, |a, b| a.f64x2_relaxed_max(b)),
            I32x4TruncSatF32x4S => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_trunc_sat_f32x4_s()),
            I32x4TruncSatF32x4U => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_trunc_sat_f32x4_u()),
            F32x4ConvertI32x4S => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_convert_i32x4_s()),
            F32x4ConvertI32x4U => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_convert_i32x4_u()),
            F64x2ConvertLowI32x4S => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_convert_low_i32x4_s()),
            F64x2ConvertLowI32x4U => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_convert_low_i32x4_u()),
            F32x4DemoteF64x2Zero => exec_op!(executor; unary Value128 => Value128, |v| v.f32x4_demote_f64x2_zero()),
            F64x2PromoteLowF32x4 => exec_op!(executor; unary Value128 => Value128, |v| v.f64x2_promote_low_f32x4()),
            I32x4TruncSatF64x2SZero => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_trunc_sat_f64x2_s_zero()),
            I32x4TruncSatF64x2UZero => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_trunc_sat_f64x2_u_zero()),
            I32x4RelaxedTruncF32x4S => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f32x4_s()),
            I32x4RelaxedTruncF32x4U => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f32x4_u()),
            I32x4RelaxedTruncF64x2SZero => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f64x2_s_zero()),
            I32x4RelaxedTruncF64x2UZero => exec_op!(executor; unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f64x2_u_zero()),
        }
    };
}
