use crate::visit::{BuilderRawOperand, FunctionDataBuilder};
use crate::{ParseError, Result};
use alloc::vec::Vec;
use tinywasm_types::{
    BranchTableArg, CastBranch, GlobalUpdate, Instruction, LocalConstCmp, LocalLocalCmp, LocalUpdate, LocalUpdateCmp,
    OperandIdx, OperandType, PackedOp, StackConst32, StackConst64, TargetLocal,
};

pub(super) trait TargetOperand: OperandType {
    fn set_target(&mut self, target: u32);
}

macro_rules! target_operands {
    ($($name:ty),+ $(,)?) => {$(
        impl TargetOperand for $name {
            fn set_target(&mut self, target: u32) { self.target = target; }
        }
    )+};
}

target_operands!(
    TargetLocal,
    CastBranch,
    StackConst32,
    StackConst64,
    LocalUpdate,
    GlobalUpdate,
    LocalUpdateCmp,
    LocalConstCmp,
    LocalLocalCmp,
    BranchTableArg,
);

pub(super) fn resolve_jump_target(instructions: &[Instruction], data: &FunctionDataBuilder, target: u32) -> u32 {
    let mut index = target as usize;
    let mut steps = 0;
    while let Some(instruction) = instructions.get(index)
        && let Some(next) = instruction_target(data, *instruction)
        && matches!(instruction, Instruction::Jump(_))
        && steps < instructions.len()
    {
        index = next as usize;
        steps += 1;
    }
    index as u32
}

pub(super) fn set_rewrite_target(
    instruction: &mut Instruction,
    data: &mut FunctionDataBuilder,
    target: u32,
) -> Result<()> {
    use Instruction::*;
    match instruction {
        JumpCmpStackLocal32(packed) | JumpCmpStackLocal64(packed) => {
            *packed = push_packed_target_copy(data, *packed, target)?
        }
        BrOnCast(index) | BrOnCastFail(index) => *index = push_target_copy(data, *index, target)?,
        JumpCmpStackConst32(packed) => *packed = push_packed_target_copy(data, *packed, target)?,
        JumpCmpStackConst64(packed) => *packed = push_packed_target_copy(data, *packed, target)?,
        BinOpLocalConstJump32(packed) | BinOpStackConstTeeLocalJump32(packed) => {
            *packed = push_packed_target_copy(data, *packed, target)?
        }
        BinOpLocalConstJumpCmpLocal32(packed) => *packed = push_packed_target_copy(data, *packed, target)?,
        BinOpGlobalConstJump32(packed) => *packed = push_packed_target_copy(data, *packed, target)?,
        IncLocalJump32(index) | IncStackTeeLocalJump32(index) => *index = push_target_copy(data, *index, target)?,
        IncGlobalJump32(index) => *index = push_target_copy(data, *index, target)?,
        IncLocalJumpCmpLocal32(packed) => *packed = push_packed_target_copy(data, *packed, target)?,
        JumpCmpLocalConst32(packed) | JumpCmpLocalConst64(packed) => {
            *packed = push_packed_target_copy(data, *packed, target)?
        }
        JumpCmpLocalLocal32(packed) | JumpCmpLocalLocal64(packed) => {
            *packed = push_packed_target_copy(data, *packed, target)?
        }
        BranchTable(index) => *index = push_target_copy(data, *index, target)?,
        _ => set_target(instruction, data, target),
    }
    Ok(())
}

fn push_target_copy<T: TargetOperand>(
    data: &mut FunctionDataBuilder,
    index: OperandIdx<T>,
    target: u32,
) -> Result<OperandIdx<T>>
where
    T::Raw: BuilderRawOperand,
{
    let mut value = data.operand(index);
    value.set_target(target);
    data.push_target_operand(value)
}

fn push_packed_target_copy<Op: Copy, T: TargetOperand>(
    data: &mut FunctionDataBuilder,
    packed: PackedOp<Op, T>,
    target: u32,
) -> Result<PackedOp<Op, T>>
where
    T::Raw: BuilderRawOperand,
{
    let mut value = data.operand(packed.index);
    value.set_target(target);
    Ok(PackedOp::new(packed.op, data.push_target_operand(value)?))
}

fn instruction_target(data: &FunctionDataBuilder, instruction: Instruction) -> Option<u32> {
    use Instruction::*;
    Some(match instruction {
        Jump(target)
        | JumpIfZero32(target)
        | JumpIfNonZero32(target)
        | JumpIfZero64(target)
        | JumpIfNonZero64(target)
        | JumpIfRefNull(target)
        | JumpIfRefNonNull(target) => target,
        JumpIfLocalZero32(arg) | JumpIfLocalNonZero32(arg) | JumpIfLocalZero64(arg) | JumpIfLocalNonZero64(arg) => {
            arg.target_ip
        }
        JumpCmpStackLocal32(packed) | JumpCmpStackLocal64(packed) => data.operand(packed.index).target,
        BrOnCast(index) | BrOnCastFail(index) => data.operand(index).target,
        JumpCmpStackConst32(packed) => data.operand(packed.index).target,
        JumpCmpStackConst64(packed) => data.operand(packed.index).target,
        BinOpLocalConstJump32(packed) | BinOpStackConstTeeLocalJump32(packed) => data.operand(packed.index).target,
        BinOpLocalConstJumpCmpLocal32(packed) => data.operand(packed.index).target,
        BinOpGlobalConstJump32(packed) => data.operand(packed.index).target,
        IncLocalJump32(index) | IncStackTeeLocalJump32(index) => data.operand(index).target,
        IncGlobalJump32(index) => data.operand(index).target,
        IncLocalJumpCmpLocal32(packed) => data.operand(packed.index).target,
        JumpCmpLocalConst32(packed) | JumpCmpLocalConst64(packed) => data.operand(packed.index).target,
        JumpCmpLocalLocal32(packed) | JumpCmpLocalLocal64(packed) => data.operand(packed.index).target,
        BranchTable(index) => data.operand(index).target,
        _ => return None,
    })
}

fn set_target(instruction: &mut Instruction, data: &mut FunctionDataBuilder, target: u32) {
    use Instruction::*;
    match instruction {
        Jump(value)
        | JumpIfZero32(value)
        | JumpIfNonZero32(value)
        | JumpIfZero64(value)
        | JumpIfNonZero64(value)
        | JumpIfRefNull(value)
        | JumpIfRefNonNull(value) => *value = target,
        JumpIfLocalZero32(arg) | JumpIfLocalNonZero32(arg) | JumpIfLocalZero64(arg) | JumpIfLocalNonZero64(arg) => {
            arg.target_ip = target
        }
        JumpCmpStackLocal32(packed) | JumpCmpStackLocal64(packed) => set_packed_operand_target(data, *packed, target),
        BrOnCast(index) | BrOnCastFail(index) => set_operand_target(data, *index, target),
        JumpCmpStackConst32(packed) => set_packed_operand_target(data, *packed, target),
        JumpCmpStackConst64(packed) => set_packed_operand_target(data, *packed, target),
        BinOpLocalConstJump32(packed) | BinOpStackConstTeeLocalJump32(packed) => {
            set_packed_operand_target(data, *packed, target)
        }
        BinOpLocalConstJumpCmpLocal32(packed) => set_packed_operand_target(data, *packed, target),
        BinOpGlobalConstJump32(packed) => set_packed_operand_target(data, *packed, target),
        IncLocalJump32(index) | IncStackTeeLocalJump32(index) => set_operand_target(data, *index, target),
        IncGlobalJump32(index) => set_operand_target(data, *index, target),
        IncLocalJumpCmpLocal32(packed) => set_packed_operand_target(data, *packed, target),
        JumpCmpLocalConst32(packed) | JumpCmpLocalConst64(packed) => set_packed_operand_target(data, *packed, target),
        JumpCmpLocalLocal32(packed) | JumpCmpLocalLocal64(packed) => set_packed_operand_target(data, *packed, target),
        BranchTable(index) => set_operand_target(data, *index, target),
        _ => {}
    }
}

fn set_operand_target<T: TargetOperand>(data: &mut FunctionDataBuilder, index: OperandIdx<T>, target: u32)
where
    T::Raw: BuilderRawOperand,
{
    let mut value = data.operand(index);
    value.set_target(target);
    data.set_operand(index, value);
}

fn set_packed_operand_target<Op: Copy, T: TargetOperand>(
    data: &mut FunctionDataBuilder,
    packed: PackedOp<Op, T>,
    target: u32,
) where
    T::Raw: BuilderRawOperand,
{
    let mut value = data.operand(packed.index);
    value.set_target(target);
    data.set_operand(packed.index, value);
}

fn branch_table_range(data: &FunctionDataBuilder, instruction: Instruction) -> Option<(u32, u32)> {
    let Instruction::BranchTable(index) = instruction else { return None };
    let operand = data.operand(index);
    Some((operand.start, operand.len))
}

pub(super) fn target_boundaries(instructions: &[Instruction], data: &FunctionDataBuilder) -> Result<Vec<bool>> {
    let mut boundaries = alloc::vec![false; instructions.len() + 1];
    for handler in &data.exception_handlers {
        for target in [handler.start_ip, handler.end_ip] {
            *boundaries.get_mut(target as usize).ok_or_else(|| {
                ParseError::Other(alloc::format!("exception handler boundary out of bounds: {target}"))
            })? = true;
        }
        for catch in &handler.catches {
            let target = catch.landing_pad();
            *boundaries
                .get_mut(target as usize)
                .ok_or_else(|| ParseError::Other(alloc::format!("exception landing pad out of bounds: {target}")))? =
                true;
        }
    }
    for &instruction in instructions {
        if let Some(target) = instruction_target(data, instruction) {
            *boundaries
                .get_mut(target as usize)
                .ok_or_else(|| ParseError::Other(alloc::format!("instruction target out of bounds: {target}")))? = true;
        }
        if let Some((start, count)) = branch_table_range(data, instruction) {
            let end =
                start.checked_add(count).ok_or_else(|| ParseError::Other("branch table range overflow".into()))?;
            let targets = data
                .branch_table_targets
                .get(start as usize..end as usize)
                .ok_or_else(|| ParseError::Other("branch table range out of bounds".into()))?;
            for &target in targets {
                *boundaries.get_mut(target as usize).ok_or_else(|| {
                    ParseError::Other(alloc::format!("branch table target out of bounds: {target}"))
                })? = true;
            }
        }
    }
    Ok(boundaries)
}

fn remap_target(target: u32, old_to_new: Option<&[u32]>, len: u32) -> Result<u32> {
    let target = if let Some(map) = old_to_new {
        *map.get(target as usize)
            .ok_or_else(|| ParseError::Other(alloc::format!("instruction target out of bounds: {target}")))?
    } else {
        target
    };
    if target >= len {
        return Err(ParseError::Other(alloc::format!("instruction target out of bounds: {target}")));
    }
    Ok(target)
}

pub(super) fn finalize(
    instructions: &mut [Instruction],
    data: &mut FunctionDataBuilder,
    old_to_new: Option<&[u32]>,
) -> Result<()> {
    let len = instructions.len() as u32;
    for handler in &mut data.exception_handlers {
        if let Some(map) = old_to_new {
            handler.start_ip = *map
                .get(handler.start_ip as usize)
                .ok_or_else(|| ParseError::Other("exception handler boundary out of bounds".into()))?;
            handler.end_ip = *map
                .get(handler.end_ip as usize)
                .ok_or_else(|| ParseError::Other("exception handler boundary out of bounds".into()))?;
            for catch in &mut handler.catches {
                let landing_pad = match catch {
                    tinywasm_types::ExceptionCatch::Tag { landing_pad, .. }
                    | tinywasm_types::ExceptionCatch::All { landing_pad, .. } => landing_pad,
                };
                *landing_pad = remap_target(*landing_pad, Some(map), len)?;
            }
        }
        if handler.start_ip > handler.end_ip || handler.end_ip > len {
            return Err(ParseError::Other("exception handler range out of bounds".into()));
        }
    }
    for target in &mut data.branch_table_targets {
        *target = remap_target(*target, old_to_new, len)?;
    }
    for instruction in instructions {
        if let Some(target) = instruction_target(data, *instruction) {
            set_target(instruction, data, remap_target(target, old_to_new, len)?);
        }
        if let Some((start, count)) = branch_table_range(data, *instruction) {
            let end =
                start.checked_add(count).ok_or_else(|| ParseError::Other("branch table range overflow".into()))?;
            data.branch_table_targets
                .get(start as usize..end as usize)
                .ok_or_else(|| ParseError::Other("branch table range out of bounds".into()))?;
        }
    }
    Ok(())
}
