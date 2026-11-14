//! Parse-time tracking of value locations and known copies during streaming emission.

use crate::visit::FunctionDataBuilder;
use alloc::vec::Vec;
use tinywasm_types::{
    Instruction, Instruction::*, InstructionEffects, InstructionValueEffect, ValueCounts, ValueLane, ValueLanes,
    ValueSource,
};

/// Tracks live ownership separately from reusable copies of cached values.
#[derive(Default)]
pub(crate) struct ValueTracker {
    lanes: [LaneState; 3],
}

#[derive(Clone, Copy, Default)]
struct LaneState {
    live: bool,
    // A local read or constant that reproduces the physical cached value.
    source: Option<ValueSource>,
}

impl ValueTracker {
    /// Returns local copies left intact by a selected branch instruction.
    pub(crate) fn branch_copies(&self, instruction: Instruction, data: &FunctionDataBuilder) -> [Option<u16>; 3] {
        let inputs = match instruction.effects() {
            InstructionEffects::Branch { inputs } => inputs,
            InstructionEffects::Boundary => ValueLanes::EMPTY,
            InstructionEffects::Operands { .. } => return [None; 3],
        };
        [ValueLane::S32, ValueLane::S64, ValueLane::S128].map(|lane| match self.lanes[lane as usize].source {
            Some(ValueSource::Local(local))
                if (self.lanes[lane as usize].live || !inputs.contains(lane))
                    && !data.may_write_local(instruction, lane, local) =>
            {
                Some(local)
            }
            _ => None,
        })
    }

    fn spill(&mut self, lane: usize, instructions: &mut Vec<Instruction>) {
        if self.lanes[lane].live {
            instructions.push([Spill32, Spill64, Spill128][lane]);
        }
        self.lanes[lane] = LaneState::default();
    }

    /// Materializes live values before a join, checkpoint, or helper boundary.
    pub(crate) fn spill_all(&mut self, instructions: &mut Vec<Instruction>) {
        self.spill(0, instructions);
        self.spill(1, instructions);
        self.spill(2, instructions);
    }

    fn consume(&mut self, inputs: ValueLanes, instructions: &mut Vec<Instruction>) {
        for (lane, kind) in [ValueLane::S32, ValueLane::S64, ValueLane::S128].into_iter().enumerate() {
            if inputs.contains(kind) {
                if !self.lanes[lane].live {
                    instructions.push([Reload32, Reload64, Reload128][lane]);
                }
                self.lanes[lane] = LaneState::default();
            }
        }
    }

    /// Supplies scalar argument tops and preserves unrelated caller values.
    pub(crate) fn prepare_call(&mut self, inputs: ValueLanes, instructions: &mut Vec<Instruction>) {
        let mut scalar_inputs = ValueLanes::EMPTY;
        for lane in [ValueLane::S32, ValueLane::S64] {
            if inputs.contains(lane) {
                scalar_inputs = scalar_inputs.with(lane);
            }
        }
        self.consume(scalar_inputs, instructions);
        self.spill_all(instructions);
    }

    /// Keeps the last result in each scalar lane in its accumulator after a call.
    pub(crate) fn call_results(&mut self, outputs: &[ValueLane]) {
        debug_assert!(self.lanes.iter().all(|lane| !lane.live));
        self.lanes = [ValueLane::S32, ValueLane::S64, ValueLane::S128]
            .map(|lane| LaneState { live: lane != ValueLane::S128 && outputs.contains(&lane), source: None });
    }

    /// Supplies each scalar result lane's top value without materializing it.
    pub(crate) fn emit_return(
        &mut self,
        instruction: Instruction,
        results: ValueCounts,
        instructions: &mut Vec<Instruction>,
    ) {
        let mut inputs = ValueLanes::EMPTY;
        if results.c32 != 0 {
            inputs = inputs.with(ValueLane::S32);
        }
        if results.c64 != 0 {
            inputs = inputs.with(ValueLane::S64);
        }
        self.consume(inputs, instructions);
        // Vector call results retain their existing materialized-stack convention.
        self.spill(2, instructions);
        self.lanes = [LaneState::default(); 3];
        instructions.push(instruction);
    }

    /// Emits one selected instruction using cached operand tops by default.
    pub(crate) fn emit(
        &mut self,
        instruction: Instruction,
        instructions: &mut Vec<Instruction>,
        optimize: bool,
        data: &FunctionDataBuilder,
    ) {
        let (effects, value_effect) = instruction.lowering_effects();
        let (inputs, output) = match effects {
            InstructionEffects::Operands { inputs, output } => (inputs, output),
            InstructionEffects::Branch { inputs } => {
                self.consume(inputs, instructions);
                // Consume the condition, then materialize surviving join values.
                self.spill_all(instructions);
                instructions.push(instruction);
                return;
            }
            InstructionEffects::Boundary => {
                if optimize
                    && let Jump(target) = instruction
                    && let Some(lane) = self.lanes.iter().position(|lane| lane.live)
                {
                    for other in 0..3 {
                        if other != lane {
                            self.spill(other, instructions);
                        }
                    }
                    self.lanes[lane] = LaneState::default();
                    instructions.push([JumpSpill32(target), JumpSpill64(target), JumpSpill128(target)][lane]);
                    return;
                }
                self.spill_all(instructions);
                instructions.push(instruction);
                return;
            }
        };

        // These writes leave the cached bits intact and establish a local copy.
        if let InstructionValueEffect::LocalWrite(lane, destination) = value_effect {
            let local = data.local_destination(destination);
            if self.lanes[lane as usize].source == Some(ValueSource::Local(local)) {
                self.lanes[lane as usize].source = None;
            }
        }
        if let InstructionValueEffect::LocalStore(lane, local) = value_effect {
            let lane = lane as usize;
            let source = ValueSource::Local(local);
            let redundant = self.lanes[lane].live && self.lanes[lane].source == Some(source);
            self.consume(inputs, instructions);
            self.lanes[lane] = LaneState { live: output.is_some(), source: Some(source) };
            if !optimize || !redundant {
                instructions.push(instruction);
            }
            return;
        }
        if let InstructionValueEffect::Drop(lane) = value_effect {
            let lane = lane as usize;
            let source = if self.lanes[lane].live { self.lanes[lane].source } else { None };
            self.consume(inputs, instructions);
            self.lanes[lane].source = source;
            if !optimize {
                instructions.push(instruction);
            }
            return;
        }

        self.consume(inputs, instructions);
        if let Some(lane) = output {
            let read = match value_effect {
                InstructionValueEffect::Read(source) => Some(source),
                _ => None,
            };
            let source = match value_effect {
                InstructionValueEffect::LocalResult(destination) => {
                    Some(ValueSource::Local(data.local_destination(destination)))
                }
                _ => read,
            };
            let lane = lane as usize;
            if optimize && !self.lanes[lane].live && read.is_some() && self.lanes[lane].source == read {
                self.lanes[lane].live = true;
                return;
            }
            if self.lanes[lane].live
                && optimize
                && let Some(instruction) = instruction.preserving()
            {
                instructions.push(instruction);
                self.lanes[lane].source = source;
                return;
            }
            // Cross-width operations must preserve an unrelated destination value.
            self.spill(lane, instructions);
            self.lanes[lane] = LaneState { live: true, source };
        } else if value_effect == InstructionValueEffect::Unknown {
            // Other no-result instructions can write locals through pooled operands.
            // Forget aliases until their local-write effects are explicitly modeled.
            for lane in &mut self.lanes {
                lane.source = None;
            }
        }
        instructions.push(instruction);
    }
}
