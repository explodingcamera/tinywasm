//! Streaming instruction selection with stable labels and target-only relocation.

use crate::{
    ParseError, ParserOptions, Result, conversion::FunctionLoweringContext, selection, value_tracker::ValueTracker,
    visit::FunctionDataBuilder,
};
use alloc::vec::Vec;
use tinywasm_types::{
    BranchTableOperand, ExceptionCatch, ExceptionHandler, Instruction, InstructionEffects, InstructionValueEffect,
    Operand128, ValueCounts, ValueLane, ValueLanes, ValueSource,
};

// The longest rule recognizes the seven-instruction SIMD rotate idiom.
const LOOKBEHIND: usize = 6;

/// Bounded selection buffer, separate from the committed instruction prefix.
pub(crate) struct PendingTail {
    instructions: [Instruction; LOOKBEHIND + 1],
    len: usize,
}

impl PendingTail {
    fn new() -> Self {
        Self { instructions: [Instruction::Unreachable; LOOKBEHIND + 1], len: 0 }
    }

    /// Copies at most seven pending instructions, including the incoming instruction.
    pub(crate) fn suffix<const N: usize>(&self) -> Option<[Instruction; N]> {
        const { assert!(N > 0 && N <= LOOKBEHIND + 1) };
        let pending = &self.instructions[..self.len];
        let start = pending.len().checked_sub(N)?;
        Some(pending[start..].try_into().unwrap())
    }

    /// Splices a non-growing suffix and truncates immediately, leaving no holes.
    pub(crate) fn replace<const N: usize, const M: usize>(&mut self, replacements: [Instruction; M]) {
        const {
            assert!(N > 0 && N <= LOOKBEHIND + 1);
            assert!(M <= N);
        }
        let pending = &mut self.instructions[..self.len];
        let start = pending.len().checked_sub(N).expect("replacement exceeds pending tail");
        pending[start..start + M].copy_from_slice(&replacements);
        self.len = start + M;
    }

    /// Moves an unchanged local or constant read after the right-hand expression.
    pub(crate) fn move_read_to_rhs(&mut self, lane: ValueLane, data: &FunctionDataBuilder) -> bool {
        let mut needed = [0u16; 3];
        needed[lane as usize] = 1;
        let end = self.len - 1;
        for start in (0..end).rev() {
            let instruction = self.instructions[start];
            let Some(inputs) = instruction.input_counts() else { return false };
            let InstructionEffects::Operands { output, .. } = instruction.effects() else { return false };
            if needed == [0; 3] {
                if inputs.is_empty()
                    && output == Some(lane)
                    && let InstructionValueEffect::Read(source) = instruction.value_effect()
                {
                    if let ValueSource::Local(local) = source
                        && self.instructions[start + 1..end].iter().any(|&next| data.may_write_local(next, lane, local))
                    {
                        return false;
                    }
                    self.instructions[start..end].rotate_left(1);
                    return true;
                }
                if output.is_some() || !inputs.is_empty() {
                    return false;
                }
            } else {
                if let Some(output) = output {
                    let Some(count) = needed[output as usize].checked_sub(1) else { return false };
                    needed[output as usize] = count;
                }
                for (count, inputs) in needed.iter_mut().zip([inputs.c32, inputs.c64, inputs.c128]) {
                    *count += inputs;
                }
            }
        }
        false
    }
}

/// Identity of a stable instruction boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LabelId(usize);

enum PatchSite {
    Instruction(usize, [Option<u16>; 3]),
    TableEntry(usize),
    ExceptionStart(usize),
    ExceptionEnd(usize),
    Catch(usize, usize),
}

/// Owns committed bytecode and a separate bounded instruction-selection buffer.
pub(crate) struct Emitter {
    instructions: Vec<Instruction>,
    pub(crate) data: FunctionDataBuilder,
    pending: PendingTail,
    checkpoint_start: usize,
    checkpoint_interval: usize,
    select: bool,
    values: ValueTracker,
    self_func_addr: u32,
    return_instruction: Instruction,
    results: ValueCounts,
    pending_jump: Option<LabelId>,
    labels: Vec<Option<u32>>,
    uses: Vec<(PatchSite, LabelId)>,
}

impl Emitter {
    /// Creates an emitter with a bounded initial allocation.
    pub(crate) fn new(body_size: usize, context: FunctionLoweringContext, options: &ParserOptions) -> Self {
        use Instruction::*;
        let return_instruction = match (options.optimize(), context.function_results) {
            (true, ValueCounts { c32: 0, c64: 0, c128: 0 }) => ReturnVoid,
            (true, ValueCounts { c32: 1, c64: 0, c128: 0 }) => Return32,
            (true, ValueCounts { c32: 0, c64: 1, c128: 0 }) => Return64,
            (true, ValueCounts { c32: 0, c64: 0, c128: 1 }) => Return128,
            _ => Return,
        };
        Self {
            instructions: Vec::with_capacity(body_size.min(1024)),
            data: FunctionDataBuilder::new(options.deduplicate_operands()),
            pending: PendingTail::new(),
            checkpoint_start: 0,
            checkpoint_interval: options.checkpoint_interval(),
            select: options.optimize(),
            values: ValueTracker::default(),
            self_func_addr: context.self_func_addr,
            return_instruction,
            results: context.function_results,
            pending_jump: None,
            labels: Vec::new(),
            uses: Vec::new(),
        }
    }

    /// Whether lowering may select optimized instruction and target forms.
    pub(crate) fn optimizations_enabled(&self) -> bool {
        self.select
    }

    /// Allocates an unbound structural or executable label.
    pub(crate) fn new_label(&mut self) -> LabelId {
        let label = LabelId(self.labels.len());
        self.labels.push(None);
        label
    }

    /// Flushes the pending tail and records a stable boundary.
    pub(crate) fn bind(&mut self, label: LabelId) -> Result<()> {
        // Only the uncommitted jump can disappear. Any other binding seals it,
        // so removing it cannot move a previously bound boundary.
        if self.pending_jump == Some(label) {
            debug_assert_eq!(self.pending.len, 1);
            debug_assert!(matches!(self.pending.instructions[0], Instruction::Jump(_)));
            self.pending_jump = None;
            self.pending.len = 0;
        }
        self.seal_jump()?;
        self.commit(self.pending.len)?;
        self.values.spill_all(&mut self.instructions);
        let offset = u32::try_from(self.instructions.len())
            .map_err(|_| ParseError::Other("function body is too large".into()))?;
        let binding = self.labels.get_mut(label.0).ok_or_else(|| ParseError::Other("unknown label".into()))?;
        if binding.is_some() {
            return Err(ParseError::Other("label bound more than once".into()));
        }
        *binding = Some(offset);
        Ok(())
    }

    /// Appends an instruction without applying logical stack effects.
    #[inline]
    pub(crate) fn emit(&mut self, instruction: Instruction) -> Result<()> {
        self.push(instruction)?;
        self.commit(self.pending.len.saturating_sub(LOOKBEHIND))?;
        Ok(())
    }

    /// Emits a helper or terminator and seals the tail against subsequent fusion.
    pub(crate) fn emit_boundary(&mut self, instruction: Instruction) -> Result<()> {
        use Instruction::*;
        let instruction = match instruction {
            Return => self.return_instruction,
            _ => instruction,
        };
        self.push(instruction)?;
        self.commit(self.pending.len)?;
        Ok(())
    }

    /// Seals a call with scalar argument and result tops in their accumulators.
    pub(crate) fn emit_call(
        &mut self,
        instruction: Instruction,
        inputs: ValueLanes,
        outputs: &[ValueLane],
    ) -> Result<()> {
        self.seal_jump()?;
        self.commit(self.pending.len)?;
        self.checkpoint();
        self.values.prepare_call(inputs, &mut self.instructions);
        let instruction = match instruction {
            Instruction::Call(address) if self.select && address == self.self_func_addr => Instruction::CallSelf,
            Instruction::ReturnCall(address) if self.select && address == self.self_func_addr => {
                Instruction::ReturnCallSelf
            }
            _ => instruction,
        };
        self.instructions.push(instruction);
        if self.instructions.len() > u32::MAX as usize {
            return Err(ParseError::Other("function body is too large".into()));
        }
        self.values.call_results(outputs);
        Ok(())
    }

    fn checkpoint(&mut self) {
        if self.instructions.len() - self.checkpoint_start >= self.checkpoint_interval {
            self.values.spill_all(&mut self.instructions);
            self.instructions.push(Instruction::Checkpoint);
            self.checkpoint_start = self.instructions.len();
        }
    }

    fn push(&mut self, instruction: Instruction) -> Result<()> {
        self.seal_jump()?;
        if self.instructions.len().checked_add(self.pending.len + 1).is_none_or(|len| len > u32::MAX as usize) {
            return Err(ParseError::Other("function body is too large".into()));
        }
        self.pending.instructions[self.pending.len] = instruction;
        self.pending.len += 1;
        Ok(())
    }

    fn seal_jump(&mut self) -> Result<()> {
        if let Some(label) = self.pending_jump.take() {
            self.commit_branch(label)?;
        }
        Ok(())
    }

    /// Appends and applies one statically selected family, without redispatching replacements.
    #[inline]
    pub(crate) fn emit_with(
        &mut self,
        instruction: Instruction,
        rules: impl FnOnce(&mut PendingTail, &mut FunctionDataBuilder) -> Result<()>,
    ) -> Result<()> {
        self.push(instruction)?;
        if self.select {
            rules(&mut self.pending, &mut self.data)?;
        }
        self.commit(self.pending.len.saturating_sub(LOOKBEHIND))?;
        Ok(())
    }

    /// Selects a conditional branch before sealing and recording its final patch site.
    pub(crate) fn branch_if(&mut self, on_zero: bool, label: LabelId) -> Result<()> {
        if self.select
            && let Some(&Instruction::Const32(value)) = self.pending.instructions[..self.pending.len].last()
        {
            self.pending.len -= 1;
            if (value == 0) == on_zero {
                self.branch(Instruction::Jump(0), label)?;
            }
            return Ok(());
        }
        let instruction = if on_zero { Instruction::JumpIfZero32(0) } else { Instruction::JumpIfNonZero32(0) };
        self.emit_with(instruction, |tail, data| selection::conditional(tail, data, on_zero, 0))?;
        // Nonconstant conditional rules retain a final target-bearing instruction.
        assert!(self.pending.len > 0, "conditional rule removed its branch");
        self.commit_branch(label)
    }

    /// Emits a target-bearing instruction and records its symbolic destination.
    /// Side operands must come from the non-deduplicating target operand allocators
    /// and belong exclusively to this instruction.
    pub(crate) fn branch(&mut self, instruction: Instruction, label: LabelId) -> Result<()> {
        if self.select && matches!(instruction, Instruction::Jump(_)) {
            // Record any earlier jump before committing the tail for this one.
            self.seal_jump()?;
            self.commit(self.pending.len)?;
            self.push(instruction)?;
            self.pending_jump = Some(label);
            return Ok(());
        }
        self.emit(instruction)?;
        self.commit_branch(label)
    }

    /// Commits table storage and records case and default destinations.
    pub(crate) fn branch_table(&mut self, targets: &[LabelId], default: LabelId) -> Result<()> {
        let start = u32::try_from(self.data.branch_table_targets.len())
            .map_err(|_| ParseError::Other("branch table pool is too large".into()))?;
        let count = u32::try_from(targets.len()).map_err(|_| ParseError::Other("branch table is too large".into()))?;
        start.checked_add(count).ok_or_else(|| ParseError::Other("branch table range overflow".into()))?;
        let operand = self.data.push_target128(Operand128::<BranchTableOperand>::new(0, start, count))?;
        self.branch(Instruction::BranchTable(operand), default)?;
        for &label in targets {
            self.uses.push((PatchSite::TableEntry(self.data.branch_table_targets.len()), label));
            self.data.branch_table_targets.push(0);
        }
        Ok(())
    }

    /// Records exception boundaries and landing pads alongside their metadata.
    pub(crate) fn exception_handler(&mut self, start: LabelId, end: LabelId, catches: Vec<(ExceptionCatch, LabelId)>) {
        let index = self.data.exception_handlers.len();
        self.uses.push((PatchSite::ExceptionStart(index), start));
        self.uses.push((PatchSite::ExceptionEnd(index), end));
        let catches = catches
            .into_iter()
            .enumerate()
            .map(|(catch_index, (catch, label))| {
                self.uses.push((PatchSite::Catch(index, catch_index), label));
                catch
            })
            .collect();
        self.data.exception_handlers.push(ExceptionHandler { start_ip: 0, end_ip: 0, catches });
    }

    fn commit(&mut self, count: usize) -> Result<()> {
        self.commit_tail::<false>(count).map(|_| ())
    }

    fn commit_branch(&mut self, label: LabelId) -> Result<()> {
        debug_assert!(self.pending.len > 0);
        let copies = self.commit_tail::<true>(self.pending.len)?;
        self.uses.push((PatchSite::Instruction(self.instructions.len() - 1, copies), label));
        Ok(())
    }

    fn commit_tail<const BRANCH: bool>(&mut self, count: usize) -> Result<[Option<u16>; 3]> {
        debug_assert!(count <= self.pending.len);
        let mut copies = [None; 3];
        if count == 0 {
            return Ok(copies);
        }
        for index in 0..count {
            let instruction = self.pending.instructions[index];
            self.checkpoint();
            if BRANCH && self.select && index + 1 == count {
                copies = self.values.branch_copies(instruction, &self.data);
            }
            if matches!(
                instruction,
                Instruction::Return
                    | Instruction::ReturnVoid
                    | Instruction::Return32
                    | Instruction::Return64
                    | Instruction::Return128
            ) {
                self.values.emit_return(instruction, self.results, &mut self.instructions);
            } else {
                self.values.emit(instruction, &mut self.instructions, self.select, &self.data);
            }
        }
        self.pending.instructions.copy_within(count..self.pending.len, 0);
        self.pending.len -= count;
        if self.instructions.len().checked_add(self.pending.len).is_none_or(|len| len > u32::MAX as usize) {
            return Err(ParseError::Other("function body is too large".into()));
        }
        Ok(copies)
    }

    /// Flushes the tail and resolves recorded targets without moving instructions.
    pub(crate) fn finish(mut self) -> Result<(Vec<Instruction>, FunctionDataBuilder)> {
        self.seal_jump()?;
        self.commit(self.pending.len)?;
        let invalid_site = || ParseError::Other("invalid label patch site".into());
        for (site, label) in self.uses {
            let mut target = self
                .labels
                .get(label.0)
                .copied()
                .flatten()
                .ok_or_else(|| ParseError::Other("referenced label is not bound".into()))?;
            let boundary = matches!(site, PatchSite::ExceptionEnd(_));
            if target as usize > self.instructions.len() || (!boundary && target as usize == self.instructions.len()) {
                return Err(ParseError::Other("label target out of bounds".into()));
            }
            match site {
                PatchSite::Instruction(index, copies) => {
                    if self.select
                        && let Some(instruction) = self.instructions.get(target as usize)
                        && let (
                            InstructionEffects::Operands { inputs, output: Some(lane) },
                            InstructionValueEffect::Read(ValueSource::Local(local)),
                        ) = instruction.lowering_effects()
                        && inputs == ValueLanes::EMPTY
                        && copies[lane as usize] == Some(local)
                    {
                        target += 1;
                    }
                    if target as usize > index {
                        use Instruction::*;
                        match (self.instructions.get(index), self.instructions.get(target as usize)) {
                            (Some(JumpSpill32(_)), Some(Reload32)) => self.instructions[index] = JumpKeep32(target),
                            (Some(JumpSpill64(_)), Some(Reload64)) => self.instructions[index] = JumpKeep64(target),
                            (Some(JumpSpill128(_)), Some(Reload128)) => self.instructions[index] = JumpKeep128(target),
                            _ => {}
                        }
                    }
                    let instruction = self.instructions.get_mut(index).ok_or_else(invalid_site)?;
                    // Target operands are private allocations, never shared immutable entries.
                    if !instruction.retarget(target, &mut self.data.operands64, &mut self.data.operands128) {
                        return Err(invalid_site());
                    }
                    if let Instruction::BranchTable(index) = instruction {
                        let operand = self.data.operand128(*index);
                        let end = operand.start().checked_add(operand.size()).ok_or_else(invalid_site)?;
                        self.data
                            .branch_table_targets
                            .get(operand.start() as usize..end as usize)
                            .ok_or_else(invalid_site)?;
                    }
                    if self.select
                        && target as usize == index
                        && let Some(looping) = instruction.self_loop_form()
                    {
                        *instruction = looping;
                    }
                }
                PatchSite::TableEntry(index) => {
                    *self.data.branch_table_targets.get_mut(index).ok_or_else(invalid_site)? = target
                }
                PatchSite::ExceptionStart(index) => {
                    self.data.exception_handlers.get_mut(index).ok_or_else(invalid_site)?.start_ip = target
                }
                PatchSite::ExceptionEnd(index) => {
                    self.data.exception_handlers.get_mut(index).ok_or_else(invalid_site)?.end_ip = target
                }
                PatchSite::Catch(handler, catch) => {
                    let catch = self
                        .data
                        .exception_handlers
                        .get_mut(handler)
                        .and_then(|handler| handler.catches.get_mut(catch))
                        .ok_or_else(invalid_site)?;
                    match catch {
                        ExceptionCatch::Tag { landing_pad, .. } | ExceptionCatch::All { landing_pad, .. } => {
                            *landing_pad = target
                        }
                    }
                }
            }
        }
        if self.select {
            for &target in self.labels.iter().flatten() {
                let Some(previous) = target.checked_sub(1) else { continue };
                // Binding places this spill before the label, not on an incoming edge.
                use Instruction::*;
                match (self.instructions.get(previous as usize), self.instructions.get(target as usize)) {
                    (Some(Spill32), Some(Reload32)) => self.instructions[previous as usize] = JumpKeep32(target),
                    (Some(Spill64), Some(Reload64)) => self.instructions[previous as usize] = JumpKeep64(target),
                    (Some(Spill128), Some(Reload128)) => self.instructions[previous as usize] = JumpKeep128(target),
                    _ => {}
                }
            }
        }
        for handler in &self.data.exception_handlers {
            if handler.start_ip > handler.end_ip || handler.end_ip as usize > self.instructions.len() {
                return Err(ParseError::Other("exception handler range out of bounds".into()));
            }
        }
        Ok((self.instructions, self.data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinywasm_types::Instruction::*;

    #[test]
    fn commitment_keeps_only_the_pending_suffix() {
        let context =
            FunctionLoweringContext { ty_idx: 0, self_func_addr: 0, function_results: ValueCounts::default() };
        let mut emitter = Emitter::new(32, context, &ParserOptions::default());
        for local in 0..20 {
            emitter.emit(LocalGet32(local)).unwrap();
            let len = emitter.pending.len;
            assert_eq!(len, (local as usize + 1).min(LOOKBEHIND));
            assert!(emitter.pending.instructions[len - 1] == LocalGet32(local));
            for (offset, instruction) in emitter.pending.instructions[..len].iter().enumerate() {
                assert!(*instruction == LocalGet32(local + 1 - len as u16 + offset as u16));
            }
        }
        emitter.commit(emitter.pending.len).unwrap();
        assert_eq!(emitter.pending.len, 0);
        let mut expected = Vec::new();
        for local in 0..20 {
            if local != 0 {
                expected.push(LocalGet32Preserve(local));
            } else {
                expected.push(LocalGet32(local));
            }
        }
        assert!(emitter.instructions == expected);
    }

    #[test]
    fn replacement_never_exposes_stale_pending_entries() {
        let mut pending = PendingTail::new();
        pending.instructions = [Const32(1), Const32(2), I32Add, Const32(3), I32Add, Const32(4), I32Add];
        pending.len = 7;
        pending.replace::<3, 1>([Const32(7)]);
        assert_eq!(pending.len, 5);
        assert!(pending.suffix::<3>() == Some([I32Add, Const32(3), Const32(7)]));
        assert!(pending.suffix::<6>().is_none());
        pending.replace::<5, 0>([]);
        assert_eq!(pending.len, 0);
        assert!(pending.suffix::<1>().is_none());
    }

    #[test]
    fn joins_keep_result_tops() {
        let context = FunctionLoweringContext {
            ty_idx: 0,
            self_func_addr: 0,
            function_results: ValueCounts { c32: 1, c64: 0, c128: 0 },
        };
        let mut emitter = Emitter::new(6, context, &ParserOptions::default());
        let otherwise = emitter.new_label();
        let join = emitter.new_label();
        emitter.emit(GlobalGet32(0)).unwrap();
        emitter.branch_if(true, otherwise).unwrap();
        emitter.emit(Const32(13)).unwrap();
        emitter.branch(Jump(0), join).unwrap();
        emitter.bind(otherwise).unwrap();
        emitter.emit(Const32(21)).unwrap();
        emitter.bind(join).unwrap();
        emitter.emit_boundary(Return).unwrap();
        let (code, _) = emitter.finish().unwrap();
        let targets: Vec<_> =
            code.iter().filter_map(|op| if let JumpKeep32(target) = op { Some(*target) } else { None }).collect();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0], targets[1]);
        assert!(matches!(code[targets[0] as usize], Reload32));
        let otherwise =
            code.iter().find_map(|op| if let JumpIfZero32(target) = op { Some(*target) } else { None }).unwrap();
        assert!(matches!(code[otherwise as usize], Const32(21)));
    }

    #[test]
    fn call_continuations_are_not_branch_destinations() {
        let context = FunctionLoweringContext {
            ty_idx: 0,
            self_func_addr: 0,
            function_results: ValueCounts { c32: 1, c64: 1, c128: 0 },
        };
        let mut emitter = Emitter::new(8, context, &ParserOptions::default());
        let join = emitter.new_label();
        emitter.emit(Const32(1)).unwrap();
        emitter.emit(Const64Imm(2)).unwrap();
        emitter.branch(Jump(0), join).unwrap();
        emitter.emit_call(Call(7), ValueLanes::EMPTY, &[ValueLane::S32, ValueLane::S64]).unwrap();
        emitter.bind(join).unwrap();
        emitter.emit_boundary(Return).unwrap();
        let (code, _) = emitter.finish().unwrap();
        let continuation = code.iter().position(|op| matches!(op, Call(7))).unwrap() + 1;
        assert!(code[continuation..].starts_with(&[Spill32, Spill64]));
        let target =
            code.iter().find_map(|op| if let JumpKeep32(target) = op { Some(*target as usize) } else { None }).unwrap();
        assert_ne!(target, continuation);
        assert!(code[target..].starts_with(&[Reload32, Reload64, Return]));
    }
}
