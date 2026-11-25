//! Streaming instruction selection with stable labels and target-only relocation.

use crate::{
    ParseError, ParserOptions, Result, conversion::FunctionLoweringContext, selection, visit::FunctionDataBuilder,
};
use alloc::vec::Vec;
use tinywasm_types::{BranchTableOperand, ExceptionCatch, ExceptionHandler, Instruction, Operand128, ValueCounts};

const LOOKBEHIND: usize = 3;

/// Bounded selection buffer, separate from the committed instruction prefix.
pub(crate) struct PendingTail {
    instructions: [Instruction; LOOKBEHIND + 1],
    len: usize,
}

impl PendingTail {
    fn new() -> Self {
        Self { instructions: [Instruction::Unreachable; LOOKBEHIND + 1], len: 0 }
    }

    /// Copies at most four pending instructions, including the incoming instruction.
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
}

/// Identity of a stable instruction boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LabelId(usize);

enum PatchSite {
    Instruction(usize),
    TableEntry(usize),
    ExceptionStart(usize),
    ExceptionEnd(usize),
    Catch(usize, usize),
}

/// Owns function bytecode and its side data, with at most three pending instructions.
pub(crate) struct Emitter {
    instructions: Vec<Instruction>,
    pub(crate) data: FunctionDataBuilder,
    pending: PendingTail,
    checkpoint_start: usize,
    checkpoint_interval: usize,
    select: bool,
    self_func_addr: u32,
    return_instruction: Instruction,
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
            self_func_addr: context.self_func_addr,
            return_instruction,
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
        self.seal_jump();
        self.commit(self.pending.len);
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
        self.commit(self.pending.len.saturating_sub(LOOKBEHIND));
        Ok(())
    }

    /// Emits a call or terminator and seals the tail against subsequent fusion.
    pub(crate) fn emit_boundary(&mut self, instruction: Instruction) -> Result<()> {
        use Instruction::*;
        let instruction = match instruction {
            Call(address) if self.select && address == self.self_func_addr => CallSelf,
            ReturnCall(address) if self.select && address == self.self_func_addr => ReturnCallSelf,
            Return => self.return_instruction,
            _ => instruction,
        };
        self.push(instruction)?;
        self.commit(self.pending.len);
        Ok(())
    }

    fn push(&mut self, instruction: Instruction) -> Result<()> {
        self.seal_jump();
        let checkpoint = self.instructions.len() - self.checkpoint_start >= self.checkpoint_interval;
        let added = self.pending.len + 1 + usize::from(checkpoint);
        if self.instructions.len().checked_add(added).is_none_or(|len| len > u32::MAX as usize) {
            return Err(ParseError::Other("function body is too large".into()));
        }
        if checkpoint {
            // Seal the pending fusion window instead of inserting into the committed prefix.
            // This permits at most LOOKBEHIND extra guest instructions per interval. Labels bound
            // at this boundary target the checkpoint, and exclusive exception ends exclude it.
            self.commit(self.pending.len);
            self.instructions.push(Instruction::Checkpoint);
            self.checkpoint_start = self.instructions.len();
        }
        self.pending.instructions[self.pending.len] = instruction;
        self.pending.len += 1;
        Ok(())
    }

    fn seal_jump(&mut self) {
        if let Some(label) = self.pending_jump.take() {
            self.commit_branch(label);
        }
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
        self.commit(self.pending.len.saturating_sub(LOOKBEHIND));
        Ok(())
    }

    /// Selects a conditional branch before sealing and recording its final patch site.
    pub(crate) fn branch_if(&mut self, on_zero: bool, label: LabelId) -> Result<()> {
        let instruction = if on_zero { Instruction::JumpIfZero32(0) } else { Instruction::JumpIfNonZero32(0) };
        self.emit_with(instruction, |tail, data| selection::conditional(tail, data, on_zero, 0))?;
        // Conditional rules always retain a final target-bearing instruction.
        assert!(self.pending.len > 0, "conditional rule removed its branch");
        self.commit_branch(label);
        Ok(())
    }

    /// Emits a target-bearing instruction and records its symbolic destination.
    /// Side operands must come from the non-deduplicating target operand allocators
    /// and belong exclusively to this instruction.
    pub(crate) fn branch(&mut self, instruction: Instruction, label: LabelId) -> Result<()> {
        if self.select && matches!(instruction, Instruction::Jump(_)) {
            // Seal an earlier jump before moving its instruction into the committed prefix.
            self.seal_jump();
            self.commit(self.pending.len);
            self.push(instruction)?;
            self.pending_jump = Some(label);
            return Ok(());
        }
        self.emit(instruction)?;
        self.commit_branch(label);
        Ok(())
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

    fn commit(&mut self, count: usize) {
        debug_assert!(count <= self.pending.len);
        if count == 0 {
            return;
        }
        self.instructions.extend_from_slice(&self.pending.instructions[..count]);
        self.pending.instructions.copy_within(count..self.pending.len, 0);
        self.pending.len -= count;
    }

    fn commit_branch(&mut self, label: LabelId) {
        debug_assert!(self.pending.len > 0);
        self.commit(self.pending.len);
        self.uses.push((PatchSite::Instruction(self.instructions.len() - 1), label));
    }

    /// Flushes the tail and resolves recorded targets without moving instructions.
    pub(crate) fn finish(mut self) -> Result<(Vec<Instruction>, FunctionDataBuilder)> {
        self.seal_jump();
        self.commit(self.pending.len);
        let data = &mut self.data;
        let invalid_site = || ParseError::Other("invalid label patch site".into());
        for (site, label) in self.uses {
            let target = self
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
                PatchSite::Instruction(index) => {
                    let instruction = self.instructions.get_mut(index).ok_or_else(invalid_site)?;
                    // Target operands are private allocations, never shared immutable entries.
                    if !instruction.retarget(target, &mut data.operands64, &mut data.operands128) {
                        return Err(invalid_site());
                    }
                    if let Instruction::BranchTable(index) = instruction {
                        let operand = data.operand128(*index);
                        let end = operand.start().checked_add(operand.size()).ok_or_else(invalid_site)?;
                        data.branch_table_targets
                            .get(operand.start() as usize..end as usize)
                            .ok_or_else(invalid_site)?;
                    }
                }
                PatchSite::TableEntry(index) => {
                    *data.branch_table_targets.get_mut(index).ok_or_else(invalid_site)? = target
                }
                PatchSite::ExceptionStart(index) => {
                    data.exception_handlers.get_mut(index).ok_or_else(invalid_site)?.start_ip = target
                }
                PatchSite::ExceptionEnd(index) => {
                    data.exception_handlers.get_mut(index).ok_or_else(invalid_site)?.end_ip = target
                }
                PatchSite::Catch(handler, catch) => {
                    let catch = data
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
        for handler in &data.exception_handlers {
            if handler.start_ip > handler.end_ip || handler.end_ip as usize > self.instructions.len() {
                return Err(ParseError::Other("exception handler range out of bounds".into()));
            }
        }
        Ok((self.instructions, self.data))
    }
}
