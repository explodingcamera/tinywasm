//! Operand decoding and encoding, without access to the instruction tail.
use crate::{Result, visit::FunctionDataBuilder};
use tinywasm_types::{Instruction, Instruction::*, *};

/// Returns an inline offset for a wasm32 memory-zero access.
pub(super) fn inline_memory_offset(data: &FunctionDataBuilder, memory: MemoryArg) -> Option<u16> {
    let arg = data.operand128(memory.memory_arg_idx);
    if memory.address64 || arg.memory() != 0 {
        return None;
    }
    u16::try_from(arg.offset()).ok()
}

/// Selects a full-width scalar store without materializing either local.
pub(super) fn scalar_store_locals(
    data: &mut FunctionDataBuilder,
    memory: MemoryArg,
    address: u16,
    value: u16,
    lane: ValueLane,
) -> Result<Option<Instruction>> {
    if let Some(offset) = inline_memory_offset(data, memory)
        && let (Ok(address), Ok(value)) = (u8::try_from(address), u8::try_from(value))
    {
        return Ok(Some(match lane {
            ValueLane::S32 => I32StoreLocalCompact(offset, address, value),
            ValueLane::S64 => I64StoreLocalCompact(offset, address, value),
            ValueLane::S128 => unreachable!(),
        }));
    }
    Ok(memory_locals(data, memory, address, value)?.map(|arg| match lane {
        ValueLane::S32 => I32StoreLocalLocal(arg),
        ValueLane::S64 => I64StoreLocalLocal(arg),
        ValueLane::S128 => unreachable!(),
    }))
}

/// Resolves and compacts a memory operand without allocating a pool entry.
pub(super) fn compact_memory(data: &FunctionDataBuilder, memory: MemoryArg) -> Option<Operand64<CompactMemoryOperand>> {
    Operand64::try_from_memory(data.operand128(memory.memory_arg_idx), memory.address64)
}

/// Encodes a memory operand with two small local indices.
pub(super) fn memory_locals(
    data: &mut FunctionDataBuilder,
    memory: MemoryArg,
    local1: u16,
    local2: u16,
) -> Result<Option<MemoryLocalArg>> {
    let (Ok(local1), Ok(local2)) = (u8::try_from(local1), u8::try_from(local2)) else { return Ok(None) };
    let Some(arg) = compact_memory(data, memory) else { return Ok(None) };
    let memory_arg_idx = data.push64(arg)?;
    Ok(Some(MemoryLocalArg { memory_arg_idx, local1, local2 }))
}

/// Reuses a producer's saved local when storing without materializing its address.
pub(super) fn stored_local_result(
    data: &mut FunctionDataBuilder,
    value: Instruction,
    address: u16,
    memory: MemoryArg,
    lane: ValueLane,
) -> Result<Option<(Instruction, MemoryLocalArg)>> {
    let Some((set, destination)) = stored_result(data, value, address, memory, lane) else { return Ok(None) };
    Ok(memory_locals(data, memory, address, destination)?.map(|arg| (set, arg)))
}

/// Returns the producer without its accumulator result and the local that retains it.
pub(super) fn stored_result(
    data: &FunctionDataBuilder,
    value: Instruction,
    address: u16,
    memory: MemoryArg,
    lane: ValueLane,
) -> Option<(Instruction, u16)> {
    if value.effects() != (InstructionEffects::Operands { inputs: ValueLanes::EMPTY, output: Some(lane) }) {
        return None;
    }
    let (Some(set), InstructionValueEffect::LocalResult(destination)) = (value.without_result(), value.value_effect())
    else {
        return None;
    };
    let destination = data.local_destination(destination);
    let address_lane = if memory.address64 { ValueLane::S64 } else { ValueLane::S32 };
    // The address was read before the producer, so an overwritten address cannot be reread.
    if lane == address_lane && address == destination {
        return None;
    }
    Some((set, destination))
}

macro_rules! fold_integer {
    ($name:ident, $compare:ident, $unsigned:ty, $signed:ty) => {
        /// Evaluates a nontrapping integer operation on constant operands.
        pub(super) fn $name(op: BinOp, left: $unsigned, right: $unsigned) -> Option<$unsigned> {
            Some(match op {
                BinOp::IAdd => left.wrapping_add(right),
                BinOp::ISub => left.wrapping_sub(right),
                BinOp::IMul => left.wrapping_mul(right),
                BinOp::IAnd => left & right,
                BinOp::IOr => left | right,
                BinOp::IXor => left ^ right,
                BinOp::IShl => left.wrapping_shl(right as u32),
                BinOp::IShrU => left.wrapping_shr(right as u32),
                BinOp::IShrS => (left as $signed).wrapping_shr(right as u32) as $unsigned,
                BinOp::IRotl => left.rotate_left(right as u32),
                BinOp::IRotr => left.rotate_right(right as u32),
                _ => return None,
            })
        }

        /// Evaluates an integer comparison on constant operands.
        pub(super) fn $compare(op: CmpOp, left: $unsigned, right: $unsigned) -> bool {
            match op {
                CmpOp::Eq => left == right,
                CmpOp::Ne => left != right,
                CmpOp::LtS => (left as $signed) < (right as $signed),
                CmpOp::LtU => left < right,
                CmpOp::GtS => (left as $signed) > (right as $signed),
                CmpOp::GtU => left > right,
                CmpOp::LeS => (left as $signed) <= (right as $signed),
                CmpOp::LeU => left <= right,
                CmpOp::GeS => (left as $signed) >= (right as $signed),
                CmpOp::GeU => left >= right,
            }
        }
    };
}

fold_integer!(fold32, compare32, u32, i32);
fold_integer!(fold64, compare64, u64, i64);

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

/// Decodes inline and pooled local/constant operands.
pub(super) fn local_const32(data: &FunctionDataBuilder, instruction: Instruction) -> Option<(BinOp, LocalAddr, u32)> {
    match instruction {
        I32AddLocalConst(arg) => Some((BinOp::IAdd, arg.local, arg.value as u32)),
        BinOpLocalConstCompact32(arg) => Some((arg.op, u16::from(arg.local), arg.value)),
        BinOpLocalConst32(packed) => {
            let arg = data.operand64(packed.index);
            Some((packed.op, arg.a(), arg.b()))
        }
        _ => None,
    }
}

/// Decodes a local-address scalar load, pooling inline operands for stronger fusions.
pub(super) fn local_load32(
    data: &mut FunctionDataBuilder,
    instruction: Instruction,
) -> Result<Option<(LoadKind32, MemoryLocalArg)>> {
    Ok(Some(match instruction {
        I32LoadLocal(arg) => (LoadKind32::U32, arg),
        I32LoadLocalCompact(arg) => {
            let Ok(local) = u8::try_from(arg.local) else { return Ok(None) };
            (
                LoadKind32::U32,
                MemoryLocalArg {
                    memory_arg_idx: data.push64(Operand64::<CompactMemoryOperand>::new(
                        u32::from(arg.offset),
                        0,
                        false,
                    ))?,
                    local1: local,
                    local2: 0,
                },
            )
        }
        I32Load8SLocal(arg) => (LoadKind32::I8, arg),
        I32Load8ULocal(arg) => (LoadKind32::U8, arg),
        I32Load16SLocal(arg) => (LoadKind32::I16, arg),
        I32Load16ULocal(arg) => (LoadKind32::U16, arg),
        _ => return Ok(None),
    }))
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

/// Encodes a float comparison with a constant, keeping branch polarity separate for NaNs.
pub(super) fn float_const_jump(
    data: &mut FunctionDataBuilder,
    target: u32,
    constant: Instruction,
    comparison: Instruction,
    on_zero: bool,
) -> Result<Option<Instruction>> {
    let op = match comparison {
        F32Eq | F64Eq => FloatCmpOp::Eq,
        F32Ne | F64Ne => FloatCmpOp::Ne,
        F32Lt | F64Lt => FloatCmpOp::Lt,
        F32Gt | F64Gt => FloatCmpOp::Gt,
        F32Le | F64Le => FloatCmpOp::Le,
        F32Ge | F64Ge => FloatCmpOp::Ge,
        _ => return Ok(None),
    };
    Ok(Some(match comparison {
        F32Eq | F32Ne | F32Lt | F32Gt | F32Le | F32Ge => {
            let Const32(value) = constant else { return Ok(None) };
            JumpFCmpAccConst32(PackedOp::new(
                (op, on_zero),
                data.push_target64(Operand64::<(u32, i32)>::new(target, value))?,
            ))
        }
        _ => {
            let Some(value) = const64(data, constant) else { return Ok(None) };
            JumpFCmpAccConst64(PackedOp::new(
                (op, on_zero),
                data.push_target128(Operand128::<(u32, i64)>::new(target, value))?,
            ))
        }
    }))
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

/// Allocates a private target operand for an accumulator/local comparison.
pub(super) fn jump_cmp_acc_local(
    data: &mut FunctionDataBuilder,
    target: u32,
    local: u16,
    op: CmpOp,
    width64: bool,
) -> Result<Instruction> {
    let index = data.push_target64(Operand64::<(u32, u16)>::new(target, local))?;
    Ok(if width64 { JumpCmpAccLocal64(PackedOp::new(op, index)) } else { JumpCmpAccLocal32(PackedOp::new(op, index)) })
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

/// Encodes a 64-bit local comparison with a full-width immediate.
pub(super) fn jump_cmp_local_const64(
    data: &mut FunctionDataBuilder,
    target: u32,
    local: u16,
    value: i64,
    op: CmpOp,
) -> Result<Instruction> {
    if value == 0 && matches!(op, CmpOp::Eq | CmpOp::Ne) {
        return Ok(local_jump(target, local, op == CmpOp::Eq, true));
    }
    Ok(JumpCmpLocalConst64(PackedOp::new(
        op,
        data.push_target128(Operand128::<(u32, i64, u16)>::new(target, value, local))?,
    )))
}

/// Encodes a 32-bit accumulator/immediate comparison.
pub(super) fn jump_cmp_acc_const32(
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
    Ok(JumpCmpAccConst32(PackedOp::new(op, data.push_target64(Operand64::<(u32, i32)>::new(target, value))?)))
}

/// Encodes a 64-bit accumulator/immediate comparison.
pub(super) fn jump_cmp_acc_const64(
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
    Ok(JumpCmpAccConst64(PackedOp::new(op, data.push_target128(Operand128::<(u32, i64)>::new(target, value))?)))
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
    let delta = op.inc_delta(immediate);
    let value = delta.unwrap_or(immediate);
    Ok(if global {
        let index = data.push_target128(Operand128::<GlobalUpdateOperand>::new(target, value, address, on_zero))?;
        if delta.is_some() {
            I32AddGlobalSetConstJump(index)
        } else {
            BinOpGlobalConstSetJump32(PackedOp::new(op, index))
        }
    } else {
        let index =
            data.push_target128(Operand128::<LocalUpdateOperand>::new(target, value, address as u16, on_zero))?;
        if delta.is_some() {
            I32AddLocalSetConstJump(index)
        } else {
            BinOpLocalConstSetJump32(PackedOp::new(op, index))
        }
    })
}
