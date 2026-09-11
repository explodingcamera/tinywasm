use super::FunctionDataBuilder;
use crate::{ParseError, Result};
use alloc::vec::Vec;
use tinywasm_types::Instruction;

/// Identifies a parser-local control-flow label.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct LabelId(u32);

enum LabelUse {
    Instruction(usize),
    Operand64(usize),
    Operand128(usize),
    BranchTableTarget(usize),
    HandlerStart(usize),
    HandlerEnd(usize),
    CatchLanding { handler: usize, catch: usize },
}

/// Tracks parser-local labels and their final bytecode relocation slots.
#[derive(Default)]
pub(super) struct LabelRegistry {
    positions: Vec<Option<u32>>,
    uses: Vec<(LabelId, LabelUse)>,
}

impl LabelRegistry {
    /// Allocates an unresolved label.
    pub(super) fn new_label(&mut self) -> LabelId {
        let label = LabelId(self.positions.len() as u32);
        self.positions.push(None);
        label
    }

    /// Pins a label to a committed instruction boundary.
    pub(super) fn pin(&mut self, label: LabelId, position: usize) -> Result<()> {
        let position = u32::try_from(position).map_err(|_| ParseError::Other("function body is too large".into()))?;
        let slot = self
            .positions
            .get_mut(label.0 as usize)
            .ok_or_else(|| ParseError::Other("unknown control-flow label".into()))?;
        if slot.replace(position).is_some() {
            return Err(ParseError::Other("control-flow label was pinned twice".into()));
        }
        Ok(())
    }

    /// Records an inline instruction target.
    pub(super) fn use_instruction(&mut self, label: LabelId, instruction: usize) {
        self.uses.push((label, LabelUse::Instruction(instruction)));
    }

    /// Records a target stored in a 64-bit side operand.
    pub(super) fn use_operand64(&mut self, label: LabelId, operand: usize) {
        self.uses.push((label, LabelUse::Operand64(operand)));
    }

    /// Records a target stored in a 128-bit side operand.
    pub(super) fn use_operand128(&mut self, label: LabelId, operand: usize) {
        self.uses.push((label, LabelUse::Operand128(operand)));
    }

    /// Records one branch-table case target.
    pub(super) fn use_branch_table_target(&mut self, label: LabelId, target: usize) {
        self.uses.push((label, LabelUse::BranchTableTarget(target)));
    }

    /// Records an exception handler's inclusive start boundary.
    pub(super) fn use_handler_start(&mut self, label: LabelId, handler: usize) {
        self.uses.push((label, LabelUse::HandlerStart(handler)));
    }

    /// Records an exception handler's exclusive end boundary.
    pub(super) fn use_handler_end(&mut self, label: LabelId, handler: usize) {
        self.uses.push((label, LabelUse::HandlerEnd(handler)));
    }

    /// Records an exception catch landing pad.
    pub(super) fn use_catch_landing(&mut self, label: LabelId, handler: usize, catch: usize) {
        self.uses.push((label, LabelUse::CatchLanding { handler, catch }));
    }

    /// Resolves all labels into final instructions and function data.
    pub(super) fn resolve(self, instructions: &mut [Instruction], data: &mut FunctionDataBuilder) -> Result<()> {
        if self.positions.iter().any(Option::is_none) {
            return Err(ParseError::Other("unresolved control-flow label".into()));
        }

        for (label, usage) in self.uses {
            let target = self.positions[label.0 as usize].unwrap();
            match usage {
                LabelUse::Instruction(index) => {
                    if target as usize >= instructions.len() {
                        return Err(ParseError::Other("branch target is outside the function body".into()));
                    }
                    let instruction = instructions
                        .get_mut(index)
                        .ok_or_else(|| ParseError::Other("branch instruction is outside the function body".into()))?;
                    match instruction {
                        Instruction::Jump(target_ip)
                        | Instruction::JumpIfZero32(target_ip)
                        | Instruction::JumpIfNonZero32(target_ip)
                        | Instruction::JumpIfZero64(target_ip)
                        | Instruction::JumpIfNonZero64(target_ip)
                        | Instruction::JumpIfAccZero32(target_ip)
                        | Instruction::JumpIfAccNonZero32(target_ip)
                        | Instruction::JumpIfAccZero64(target_ip)
                        | Instruction::JumpIfAccNonZero64(target_ip)
                        | Instruction::JumpIfRefNull(target_ip)
                        | Instruction::JumpIfRefNonNull(target_ip) => *target_ip = target,
                        Instruction::JumpIfLocalZero32(argument)
                        | Instruction::JumpIfLocalNonZero32(argument)
                        | Instruction::JumpIfLocalZero64(argument)
                        | Instruction::JumpIfLocalNonZero64(argument) => argument.target_ip = target,
                        _ => {
                            return Err(ParseError::Other(
                                "instruction does not contain an inline branch target".into(),
                            ));
                        }
                    }
                }
                LabelUse::Operand64(index) => {
                    if target as usize >= instructions.len() {
                        return Err(ParseError::Other("branch target is outside the function body".into()));
                    }
                    let operand = data
                        .operands64
                        .get_mut(index)
                        .ok_or_else(|| ParseError::Other("branch operand is outside the function data".into()))?;
                    *operand = operand.with_target(target);
                }
                LabelUse::Operand128(index) => {
                    if target as usize >= instructions.len() {
                        return Err(ParseError::Other("branch target is outside the function body".into()));
                    }
                    let operand = data
                        .operands128
                        .get_mut(index)
                        .ok_or_else(|| ParseError::Other("branch operand is outside the function data".into()))?;
                    *operand = operand.with_target(target);
                }
                LabelUse::BranchTableTarget(index) => {
                    if target as usize >= instructions.len() {
                        return Err(ParseError::Other("branch table target is outside the function body".into()));
                    }
                    *data.branch_table_targets.get_mut(index).ok_or_else(|| {
                        ParseError::Other("branch table target is outside the function data".into())
                    })? = target;
                }
                LabelUse::HandlerStart(handler) => {
                    if target as usize > instructions.len() {
                        return Err(ParseError::Other("exception handler starts outside the function body".into()));
                    }
                    data.exception_handlers[handler].start_ip = target;
                }
                LabelUse::HandlerEnd(handler) => {
                    if target as usize > instructions.len() {
                        return Err(ParseError::Other("exception handler ends outside the function body".into()));
                    }
                    data.exception_handlers[handler].end_ip = target;
                }
                LabelUse::CatchLanding { handler, catch } => {
                    if target as usize >= instructions.len() {
                        return Err(ParseError::Other("exception landing pad is outside the function body".into()));
                    }
                    let catch = data
                        .exception_handlers
                        .get_mut(handler)
                        .and_then(|handler| handler.catches.get_mut(catch))
                        .ok_or_else(|| ParseError::Other("exception catch is outside the function data".into()))?;
                    match catch {
                        tinywasm_types::ExceptionCatch::Tag { landing_pad, .. }
                        | tinywasm_types::ExceptionCatch::All { landing_pad, .. } => *landing_pad = target,
                    }
                }
            }
        }
        Ok(())
    }
}
