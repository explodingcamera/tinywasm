use crate::visit::FunctionDataBuilder;
use crate::{ParseError, Result};
use alloc::vec::Vec;
use tinywasm_types::{Instruction, PackedOp};

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
    macro_rules! rewrite_target {
        ($slot:ident, operand64) => {{ data.push_target_operand64(data.operand64(*$slot).with_target(target))? }};
        ($slot:ident, operand128) => {{ data.push_target_operand128(data.operand128(*$slot).with_target(target))? }};
        ($slot:ident, packed64) => {{ PackedOp::new($slot.op, data.push_target_operand64(data.operand64($slot.index).with_target(target))?) }};
        ($slot:ident, packed128) => {{ PackedOp::new($slot.op, data.push_target_operand128(data.operand128($slot.index).with_target(target))?) }};
    }

    use Instruction::*;
    match instruction {
        JumpCmpStackLocal32(packed) => *packed = rewrite_target!(packed, packed64),
        JumpCmpStackLocal64(packed) => *packed = rewrite_target!(packed, packed64),
        BrOnCast(index) | BrOnCastFail(index) => *index = rewrite_target!(index, operand64),
        JumpCmpStackConst32(packed) => *packed = rewrite_target!(packed, packed64),
        JumpCmpStackConst64(packed) => *packed = rewrite_target!(packed, packed128),
        BinOpStackConstTeeLocalJump32(packed) => *packed = rewrite_target!(packed, packed128),
        BinOpLocalConstJump32(packed) => *packed = rewrite_target!(packed, packed128),
        BinOpLocalConstJumpCmpLocal32(packed) => *packed = rewrite_target!(packed, packed128),
        BinOpGlobalConstJump32(packed) => *packed = rewrite_target!(packed, packed128),
        IncLocalJump32(index) => *index = rewrite_target!(index, operand128),
        IncStackTeeLocalJump32(index) => *index = rewrite_target!(index, operand128),
        IncGlobalJump32(index) => *index = rewrite_target!(index, operand128),
        IncLocalJumpCmpLocal32(packed) => *packed = rewrite_target!(packed, packed128),
        JumpCmpLocalConst32(packed) => *packed = rewrite_target!(packed, packed128),
        JumpCmpLocalConst64(packed) => *packed = rewrite_target!(packed, packed128),
        JumpCmpLocalLocal32(packed) => *packed = rewrite_target!(packed, packed64),
        JumpCmpLocalLocal64(packed) => *packed = rewrite_target!(packed, packed64),
        BranchTable(index) => *index = rewrite_target!(index, operand128),
        _ => set_target(instruction, data, target),
    }

    Ok(())
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
        JumpCmpStackLocal32(packed) | JumpCmpStackLocal64(packed) => data.operand64(packed.index).target(),
        BrOnCast(index) | BrOnCastFail(index) => data.operand64(index).target(),
        JumpCmpStackConst32(packed) => data.operand64(packed.index).target(),
        JumpCmpStackConst64(packed) => data.operand128(packed.index).target(),
        BinOpLocalConstJump32(packed) | BinOpStackConstTeeLocalJump32(packed) => data.operand128(packed.index).target(),
        BinOpLocalConstJumpCmpLocal32(packed) => data.operand128(packed.index).target(),
        BinOpGlobalConstJump32(packed) => data.operand128(packed.index).target(),
        IncLocalJump32(index) | IncStackTeeLocalJump32(index) => data.operand128(index).target(),
        IncGlobalJump32(index) => data.operand128(index).target(),
        IncLocalJumpCmpLocal32(packed) => data.operand128(packed.index).target(),
        JumpCmpLocalConst32(packed) | JumpCmpLocalConst64(packed) => data.operand128(packed.index).target(),
        JumpCmpLocalLocal32(packed) | JumpCmpLocalLocal64(packed) => data.operand64(packed.index).target(),
        BranchTable(index) => data.operand128(index).target(),
        _ => return None,
    })
}

fn set_target(instruction: &mut Instruction, data: &mut FunctionDataBuilder, target: u32) {
    macro_rules! set_target {
        ($index:expr, operand64) => {{ data.set_operand64($index, data.operand64($index).with_target(target)) }};
        ($index:expr, operand128) => {{ data.set_operand128($index, data.operand128($index).with_target(target)) }};
        ($packed:expr, packed64) => {{ data.set_operand64($packed.index, data.operand64($packed.index).with_target(target)) }};
        ($packed:expr, packed128) => {{ data.set_operand128($packed.index, data.operand128($packed.index).with_target(target)) }};
    }

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
        JumpCmpStackLocal32(packed) | JumpCmpStackLocal64(packed) => set_target!(*packed, packed64),
        BrOnCast(index) | BrOnCastFail(index) => set_target!(*index, operand64),
        JumpCmpStackConst32(packed) => set_target!(*packed, packed64),
        JumpCmpStackConst64(packed) => set_target!(*packed, packed128),
        BinOpLocalConstJump32(packed) => set_target!(*packed, packed128),
        BinOpStackConstTeeLocalJump32(packed) => set_target!(*packed, packed128),
        BinOpLocalConstJumpCmpLocal32(packed) => set_target!(*packed, packed128),
        BinOpGlobalConstJump32(packed) => set_target!(*packed, packed128),
        IncLocalJump32(index) | IncStackTeeLocalJump32(index) => set_target!(*index, operand128),
        IncGlobalJump32(index) => set_target!(*index, operand128),
        IncLocalJumpCmpLocal32(packed) => set_target!(*packed, packed128),
        JumpCmpLocalConst32(packed) | JumpCmpLocalConst64(packed) => set_target!(*packed, packed128),
        JumpCmpLocalLocal32(packed) | JumpCmpLocalLocal64(packed) => set_target!(*packed, packed64),
        BranchTable(index) => set_target!(*index, operand128),
        _ => {}
    }
}

fn branch_table_range(data: &FunctionDataBuilder, instruction: Instruction) -> Option<(u32, u32)> {
    let Instruction::BranchTable(index) = instruction else { return None };
    let operand = data.operand128(index);
    Some((operand.start(), operand.size()))
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
