//! Streaming instruction selection with stable labels and target-only relocation.

use crate::{
    ParseError, ParserOptions, Result, conversion::FunctionLoweringContext, selection, visit::FunctionDataBuilder,
};
use alloc::vec::Vec;
use tinywasm_types::{
    BranchTableOperand, ExceptionCatch, ExceptionHandler, Instruction, MemoryOperand, Operand128, Operand128Idx,
    ValueCounts,
};

const LOOKBEHIND: usize = 3;

/// Restricted vector view, with no readable or mutable access to the committed prefix.
pub(crate) struct PendingTail<'a> {
    instructions: &'a mut Vec<Instruction>,
    start: usize,
}

impl<'a> PendingTail<'a> {
    fn new(instructions: &'a mut Vec<Instruction>, start: usize) -> Self {
        assert!(start < instructions.len() && instructions.len() - start <= LOOKBEHIND + 1);
        Self { instructions, start }
    }

    /// Copies at most four pending instructions, including the incoming instruction.
    pub(crate) fn suffix<const N: usize>(&self) -> Option<[Instruction; N]> {
        const { assert!(N > 0 && N <= LOOKBEHIND + 1) };
        let pending = &self.instructions[self.start..];
        let start = pending.len().checked_sub(N)?;
        Some(pending[start..].try_into().unwrap())
    }

    /// Splices a non-growing suffix and truncates immediately, leaving no holes.
    pub(crate) fn replace<const N: usize, const M: usize>(&mut self, replacements: [Instruction; M]) {
        const {
            assert!(N > 0 && N <= LOOKBEHIND + 1);
            assert!(M <= N);
        }
        let pending = &mut self.instructions[self.start..];
        let start = pending.len().checked_sub(N).expect("replacement exceeds pending tail");
        pending[start..start + M].copy_from_slice(&replacements);
        self.instructions.truncate(self.start + start + M);
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

/// Owns one instruction vector with an immutable prefix and at most three pending instructions.
pub(crate) struct Emitter {
    instructions: Vec<Instruction>,
    committed: usize,
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
            committed: 0,
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
            debug_assert_eq!(self.instructions.len(), self.committed + 1);
            debug_assert!(matches!(self.instructions.last(), Some(Instruction::Jump(_))));
            self.pending_jump = None;
            self.instructions.pop();
        }
        self.seal_jump();
        self.commit(self.instructions.len());
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
        self.commit(self.instructions.len().saturating_sub(LOOKBEHIND).max(self.committed));
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
        self.commit(self.instructions.len());
        Ok(())
    }

    fn push(&mut self, instruction: Instruction) -> Result<()> {
        self.seal_jump();
        if self.instructions.len() >= u32::MAX as usize {
            return Err(ParseError::Other("function body is too large".into()));
        }
        self.instructions.push(instruction);
        Ok(())
    }

    fn seal_jump(&mut self) {
        if let Some(label) = self.pending_jump.take() {
            self.uses.push((PatchSite::Instruction(self.instructions.len() - 1), label));
            self.committed = self.instructions.len();
        }
    }

    /// Appends and applies one statically selected family, without redispatching replacements.
    #[inline]
    pub(crate) fn emit_with(
        &mut self,
        data: &mut FunctionDataBuilder,
        instruction: Instruction,
        rules: impl FnOnce(&mut PendingTail<'_>, &mut FunctionDataBuilder) -> Result<()>,
    ) -> Result<()> {
        self.push(instruction)?;
        if self.select {
            rules(&mut PendingTail::new(&mut self.instructions, self.committed), data)?;
        }
        self.commit(self.instructions.len().saturating_sub(LOOKBEHIND).max(self.committed));
        Ok(())
    }

    /// Selects a conditional branch before sealing and recording its final patch site.
    pub(crate) fn branch_if(&mut self, data: &mut FunctionDataBuilder, on_zero: bool, label: LabelId) -> Result<()> {
        let instruction = if on_zero { Instruction::JumpIfZero32(0) } else { Instruction::JumpIfNonZero32(0) };
        self.emit_with(data, instruction, |tail, data| selection::conditional(tail, data, on_zero, 0))?;
        // Conditional rules always retain a final target-bearing instruction.
        assert!(self.instructions.len() > self.committed, "conditional rule removed its branch");
        let site = self.instructions.len() - 1;
        self.uses.push((PatchSite::Instruction(site), label));
        self.commit(self.instructions.len());
        Ok(())
    }

    /// Emits a target-bearing instruction and records its symbolic destination.
    /// Side operands must come from the non-deduplicating target operand allocators
    /// and belong exclusively to this instruction.
    pub(crate) fn branch(&mut self, instruction: Instruction, label: LabelId) -> Result<()> {
        if self.select && matches!(instruction, Instruction::Jump(_)) {
            self.commit(self.instructions.len());
            self.push(instruction)?;
            self.pending_jump = Some(label);
            return Ok(());
        }
        self.emit(instruction)?;
        let site = self.instructions.len() - 1;
        self.uses.push((PatchSite::Instruction(site), label));
        self.commit(self.instructions.len());
        Ok(())
    }

    /// Commits table storage and records case and default destinations.
    pub(crate) fn branch_table(
        &mut self,
        data: &mut FunctionDataBuilder,
        targets: &[LabelId],
        default: LabelId,
    ) -> Result<()> {
        let start = u32::try_from(data.branch_table_targets.len())
            .map_err(|_| ParseError::Other("branch table pool is too large".into()))?;
        let count = u32::try_from(targets.len()).map_err(|_| ParseError::Other("branch table is too large".into()))?;
        start.checked_add(count).ok_or_else(|| ParseError::Other("branch table range overflow".into()))?;
        let operand = data.push_target128(Operand128::<BranchTableOperand>::new(0, start, count))?;
        self.branch(Instruction::BranchTable(operand), default)?;
        for &label in targets {
            self.uses.push((PatchSite::TableEntry(data.branch_table_targets.len()), label));
            data.branch_table_targets.push(0);
        }
        Ok(())
    }

    /// Records exception boundaries and landing pads alongside their metadata.
    pub(crate) fn exception_handler(
        &mut self,
        data: &mut FunctionDataBuilder,
        start: LabelId,
        end: LabelId,
        catches: Vec<(ExceptionCatch, LabelId)>,
    ) {
        let index = data.exception_handlers.len();
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
        data.exception_handlers.push(ExceptionHandler { start_ip: 0, end_ip: 0, catches });
    }

    fn commit(&mut self, end: usize) {
        debug_assert!(self.committed <= end && end <= self.instructions.len());
        self.committed = end;
    }

    /// Flushes the tail and resolves recorded targets without moving instructions.
    pub(crate) fn finish(mut self, data: &mut FunctionDataBuilder) -> Result<Vec<Instruction>> {
        self.seal_jump();
        self.commit(self.instructions.len());
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
                    Self::patch_target(instruction, data, target)?;
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
        // Run after fusion so only surviving memory-0 loads become inline-offset instructions.
        if self.select {
            let inline_offset = |index: Operand128Idx<MemoryOperand>| {
                let arg = data.operand128(index);
                if arg.memory() == 0 { u32::try_from(arg.offset()).ok() } else { None }
            };
            for instruction in &mut self.instructions {
                use Instruction::*;
                let replacement = match *instruction {
                    I32Load(index) => inline_offset(index).map(I32LoadInline),
                    I32Load8U(index) => inline_offset(index).map(I32Load8UInline),
                    I32Load16S(index) => inline_offset(index).map(I32Load16SInline),
                    _ => None,
                };
                if let Some(replacement) = replacement {
                    *instruction = replacement;
                }
            }
        }
        Ok(self.instructions)
    }

    fn patch_target(instruction: &mut Instruction, data: &mut FunctionDataBuilder, target: u32) -> Result<()> {
        // Target operands are allocated separately from deduplicated immutable
        // operands. Relocation changes only their target field, not pool layout.
        macro_rules! patch {
            ($index:expr, $pool:ident) => {{
                let operand = data
                    .$pool
                    .get_mut($index.index())
                    .ok_or_else(|| ParseError::Other("target operand out of bounds".into()))?;
                *operand = operand.with_target(target);
            }};
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
            JumpCmpStackLocal32(packed) | JumpCmpStackLocal64(packed) => patch!(packed.index, operands64),
            BrOnCast(index) | BrOnCastFail(index) => patch!(*index, operands64),
            JumpCmpStackConst32(packed) => patch!(packed.index, operands64),
            JumpCmpStackConst64(packed) => patch!(packed.index, operands128),
            BinOpLocalConstJump32(packed) | BinOpStackConstTeeLocalJump32(packed) => patch!(packed.index, operands128),
            BinOpLocalConstJumpCmpLocal32(packed) => patch!(packed.index, operands128),
            BinOpGlobalConstJump32(packed) => patch!(packed.index, operands128),
            IncLocalJump32(index) | IncStackTeeLocalJump32(index) => patch!(*index, operands128),
            IncGlobalJump32(index) => patch!(*index, operands128),
            IncLocalJumpCmpLocal32(packed) => patch!(packed.index, operands128),
            JumpCmpLocalConst32(packed) | JumpCmpLocalConst64(packed) => patch!(packed.index, operands128),
            JumpCmpLocalLocal32(packed) | JumpCmpLocalLocal64(packed) => patch!(packed.index, operands64),
            BranchTable(index) => patch!(*index, operands128),
            _ => return Err(ParseError::Other("unsupported instruction label patch".into())),
        }
        Ok(())
    }
}
