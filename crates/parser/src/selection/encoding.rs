//! Operand decoding and encoding, without access to the instruction tail.
use crate::{Result, visit::FunctionDataBuilder};
use tinywasm_types::{Instruction, Instruction::*, *};

/// Reads a 64-bit literal without requiring a pool entry for immediate values.
pub(super) fn const64(data: &FunctionDataBuilder, instruction: Instruction) -> Option<i64> {
    match instruction {
        Const64(index) => Some(data.operand64(index).value()),
        Const64Imm(value) => Some(i64::from(value)),
        _ => None,
    }
}

/// Materializes a literal only when the selected encoding requires an operand index.
pub(super) fn pool64(data: &mut FunctionDataBuilder, instruction: Instruction) -> Result<Operand64Idx<i64>> {
    match instruction {
        Const64(index) => Ok(index),
        Const64Imm(value) => data.push64(Operand64::<i64>::new(i64::from(value))),
        _ => unreachable!("expected a 64-bit constant"),
    }
}

/// Materializes a vector literal only when the selected encoding requires an operand index.
pub(super) fn pool128(data: &mut FunctionDataBuilder, instruction: Instruction) -> Result<Operand128Idx<[u8; 16]>> {
    match instruction {
        Const128(index) => Ok(index),
        Const128Imm(value) => data.push128(Operand128::<[u8; 16]>::new(u128::from(value).to_le_bytes())),
        _ => unreachable!("expected a vector constant"),
    }
}

/// Decodes specialized and pooled local constants uniformly.
pub(super) fn local_const32(data: &FunctionDataBuilder, instruction: Instruction) -> Option<(BinOp, u16, i32)> {
    match instruction {
        AddLocalConst32(arg) => Some((BinOp::IAdd, arg.local, arg.value)),
        SubLocalConst32(arg) => Some((BinOp::ISub, arg.local, arg.value)),
        MulLocalConst32(arg) => Some((BinOp::IMul, arg.local, arg.value)),
        AndLocalConst32(arg) => Some((BinOp::IAnd, arg.local, arg.value)),
        ShrULocalConst32(arg) => Some((BinOp::IShrU, arg.local, arg.value)),
        BinOpLocalConst32(packed) => {
            let value = data.operand64(packed.index);
            Some((packed.op, value.a(), value.b() as i32))
        }
        _ => None,
    }
}

/// Decodes a pooled 64-bit local constant.
pub(super) fn local_const64(data: &FunctionDataBuilder, instruction: Instruction) -> Option<(BinOp, u16, i64)> {
    let BinOpLocalConst64(packed) = instruction else { return None };
    let value = data.operand128(packed.index);
    Some((packed.op, value.a(), value.b() as i64))
}

/// Decodes a pooled vector local constant.
pub(super) fn local_const128(
    data: &FunctionDataBuilder,
    instruction: Instruction,
) -> Option<(BinOp128, u16, Operand128Idx<[u8; 16]>)> {
    let BinOpLocalConst128(packed) = instruction else { return None };
    let value = data.operand64(packed.index);
    Some((packed.op, value.a(), value.b()))
}

/// Encodes the dedicated 32-bit local operations before falling back to a pooled operand.
pub(super) fn encode_local_const32(
    data: &mut FunctionDataBuilder,
    op: BinOp,
    local: u16,
    value: i32,
) -> Result<Instruction> {
    let arg = I32LocalArg { value, local };
    Ok(match op {
        BinOp::IAdd => AddLocalConst32(arg),
        BinOp::ISub => SubLocalConst32(arg),
        BinOp::IMul => MulLocalConst32(arg),
        BinOp::IAnd => AndLocalConst32(arg),
        BinOp::IShrU => ShrULocalConst32(arg),
        _ => BinOpLocalConst32(PackedOp::new(op, data.push64(Operand64::<(u16, u32)>::new(local, value as u32))?)),
    })
}

/// Encodes dedicated stack-immediate operations.
pub(super) fn stack_const32(op: BinOp, value: i32) -> Instruction {
    match op {
        BinOp::IAnd => AndConst32(value),
        BinOp::IXor => XorConst32(value),
        BinOp::IShrU => ShrUConst32(value),
        _ => BinOpStackConst32(op, value),
    }
}

macro_rules! local_const_set {
    ($($name:ident, $op:ty, $value:ty, [$($cast:tt)*], $pool:ident, $operand:ty, $set:ident, $tee:ident;)*) => {$(
        /// Encodes a local constant operation with a destination local.
        pub(super) fn $name(data: &mut FunctionDataBuilder, op: $op, src: u16, dst: u16, value: $value, tee: bool) -> Result<Instruction> {
            let index = data.$pool(<$operand>::new(src, dst, value $($cast)*))?;
            Ok(if tee { $tee(PackedOp::new(op, index)) } else { $set(PackedOp::new(op, index)) })
        }
    )*};
}
local_const_set! {
    local_const_set32, BinOp, i32, [as u32], push64, Operand64<(u16, u16, u32)>, BinOpLocalConstSet32, BinOpLocalConstTee32;
    local_const_set64, BinOp, i64, [as u64], push128, Operand128<(u16, u16, u64)>, BinOpLocalConstSet64, BinOpLocalConstTee64;
    local_const_set128, BinOp128, Operand128Idx<[u8; 16]>, [], push64, Operand64<(u16, u16, Operand128Idx<[u8; 16]>)>, BinOpLocalConstSet128, BinOpLocalConstTee128;
}

/// Decodes a preceding integer comparison, not the incoming instruction.
pub(super) fn cmp_op(instruction: Instruction) -> Option<CmpOp> {
    Some(match instruction {
        I32Eq | I64Eq => CmpOp::Eq,
        I32Ne | I64Ne => CmpOp::Ne,
        I32LtS | I64LtS => CmpOp::LtS,
        I32LtU | I64LtU => CmpOp::LtU,
        I32GtS | I64GtS => CmpOp::GtS,
        I32GtU | I64GtU => CmpOp::GtU,
        I32LeS | I64LeS => CmpOp::LeS,
        I32LeU | I64LeU => CmpOp::LeU,
        I32GeS | I64GeS => CmpOp::GeS,
        I32GeU | I64GeU => CmpOp::GeU,
        _ => return None,
    })
}

/// Applies branch polarity to a preceding integer comparison.
pub(super) fn branch_cmp(instruction: Instruction, on_zero: bool) -> Option<CmpOp> {
    cmp_op(instruction).map(|op| if on_zero { op.inverse() } else { op })
}

/// Encodes a local zero test of the requested width and polarity.
pub(super) fn local_jump(target: u32, local: u16, on_zero: bool, width64: bool) -> Instruction {
    let arg = TargetLocalArg { target_ip: target, local };
    match (on_zero, width64) {
        (true, false) => JumpIfLocalZero32(arg),
        (false, false) => JumpIfLocalNonZero32(arg),
        (true, true) => JumpIfLocalZero64(arg),
        (false, true) => JumpIfLocalNonZero64(arg),
    }
}

/// Allocates a private target operand for a local/local comparison.
pub(super) fn jump_cmp_local_local(
    data: &mut FunctionDataBuilder,
    target: u32,
    left: u16,
    right: u16,
    op: CmpOp,
    width64: bool,
) -> Result<Instruction> {
    let index = data.push_target64(Operand64::<(u32, u16, u16)>::new(target, left, right))?;
    Ok(if width64 {
        JumpCmpLocalLocal64(PackedOp::new(op, index))
    } else {
        JumpCmpLocalLocal32(PackedOp::new(op, index))
    })
}

/// Allocates a private target operand for a stack/local comparison.
pub(super) fn jump_cmp_stack_local(
    data: &mut FunctionDataBuilder,
    target: u32,
    local: u16,
    op: CmpOp,
    width64: bool,
) -> Result<Instruction> {
    let index = data.push_target64(Operand64::<(u32, u16)>::new(target, local))?;
    Ok(if width64 {
        JumpCmpStackLocal64(PackedOp::new(op, index))
    } else {
        JumpCmpStackLocal32(PackedOp::new(op, index))
    })
}

/// Encodes a local/immediate comparison, specializing zero equality tests.
pub(super) fn jump_cmp_local_const32(
    data: &mut FunctionDataBuilder,
    target: u32,
    local: u16,
    value: i32,
    op: CmpOp,
) -> Result<Instruction> {
    if value == 0 && matches!(op, CmpOp::Eq | CmpOp::Ne) {
        return Ok(local_jump(target, local, op == CmpOp::Eq, false));
    }
    Ok(JumpCmpLocalConst32(PackedOp::new(
        op,
        data.push_target128(Operand128::<(u32, i32, u16)>::new(target, value, local))?,
    )))
}

/// Encodes a 64-bit local comparison with a representable 32-bit immediate.
pub(super) fn jump_cmp_local_const64(
    data: &mut FunctionDataBuilder,
    target: u32,
    local: u16,
    value: i32,
    op: CmpOp,
) -> Result<Instruction> {
    if value == 0 && matches!(op, CmpOp::Eq | CmpOp::Ne) {
        return Ok(local_jump(target, local, op == CmpOp::Eq, true));
    }
    Ok(JumpCmpLocalConst64(PackedOp::new(
        op,
        data.push_target128(Operand128::<(u32, i32, u16)>::new(target, value, local))?,
    )))
}

/// Encodes a 32-bit stack/immediate comparison.
pub(super) fn jump_cmp_stack_const32(
    data: &mut FunctionDataBuilder,
    target: u32,
    value: i32,
    op: CmpOp,
) -> Result<Instruction> {
    if value == 0 {
        match op {
            CmpOp::Eq => return Ok(JumpIfZero32(target)),
            CmpOp::Ne => return Ok(JumpIfNonZero32(target)),
            _ => {}
        }
    }
    Ok(JumpCmpStackConst32(PackedOp::new(op, data.push_target64(Operand64::<(u32, i32)>::new(target, value))?)))
}

/// Encodes a 64-bit stack/immediate comparison.
pub(super) fn jump_cmp_stack_const64(
    data: &mut FunctionDataBuilder,
    target: u32,
    value: i64,
    op: CmpOp,
) -> Result<Instruction> {
    if value == 0 {
        match op {
            CmpOp::Eq => return Ok(JumpIfZero64(target)),
            CmpOp::Ne => return Ok(JumpIfNonZero64(target)),
            _ => {}
        }
    }
    Ok(JumpCmpStackConst64(PackedOp::new(op, data.push_target128(Operand128::<(u32, i64)>::new(target, value))?)))
}

/// Encodes a stack update, tee, and branch using a private target operand.
pub(super) fn stack_update_jump(
    data: &mut FunctionDataBuilder,
    target: u32,
    value: i32,
    local: u16,
    op: BinOp,
    on_zero: bool,
) -> Result<Instruction> {
    Ok(BinOpStackConstTeeLocalJump32(PackedOp::new(
        op,
        data.push_target128(Operand128::<LocalUpdateOperand>::new(target, value, local, on_zero))?,
    )))
}

/// Encodes local/global updates, specializing increments when possible.
pub(super) fn update_jump(
    data: &mut FunctionDataBuilder,
    target: u32,
    immediate: i32,
    address: u32,
    op: BinOp,
    on_zero: bool,
    global: bool,
) -> Result<Instruction> {
    if let Some(delta) = op.inc_delta(immediate) {
        Ok(if global {
            IncGlobalJump32(
                data.push_target128(Operand128::<GlobalUpdateOperand>::new(target, delta, address, on_zero))?,
            )
        } else {
            IncLocalJump32(data.push_target128(Operand128::<LocalUpdateOperand>::new(
                target,
                delta,
                address as u16,
                on_zero,
            ))?)
        })
    } else {
        Ok(if global {
            BinOpGlobalConstJump32(PackedOp::new(
                op,
                data.push_target128(Operand128::<GlobalUpdateOperand>::new(target, immediate, address, on_zero))?,
            ))
        } else {
            BinOpLocalConstJump32(PackedOp::new(
                op,
                data.push_target128(Operand128::<LocalUpdateOperand>::new(target, immediate, address as u16, on_zero))?,
            ))
        })
    }
}
