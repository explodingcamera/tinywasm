use alloc::vec::Vec;
use tinywasm_types::{
    BinOp, CmpOp, Instruction, LoadOp32, LocalAddr, MemoryOperand, Operand64Idx, Operand128Idx, PackedOp, PackedOp64,
};

#[derive(Clone, Copy)]
enum PendingOp {
    I32AddLocalConst { local: LocalAddr, value: i32 },
    I32AddLocalConstTee { local: LocalAddr, value: i32, destination: LocalAddr, operand: Operand64Idx<(u16, u16, u32)> },
    AccBinOpLocalConst32(PackedOp64<BinOp, (u16, u32)>),
    AccBinOpLocalLocal32 { op: BinOp, left: LocalAddr, right: LocalAddr },
    AccBinOpConst32 { op: BinOp, value: i32 },
    I32CompareAccLocal { op: CmpOp, right: LocalAddr },
    I32CompareAccConst { op: CmpOp, value: i32 },
    I32CompareLocalConst(PackedOp64<CmpOp, (u16, u32)>),
    I32CompareLocalLocal { op: CmpOp, left: LocalAddr, right: LocalAddr },
    AccBinOpStack64(BinOp),
    AccBinOpStackStack64(BinOp),
    AccBinOpLocal64 { op: BinOp, local: LocalAddr },
    AccBinOpConst64(PackedOp64<BinOp, i64>),
    AccBinOpNestedLocalLocal64(PackedOp64<(BinOp, BinOp), (u16, u16)>),
}

impl PendingOp {
    fn fallback(self) -> Instruction {
        match self {
            Self::I32AddLocalConst { local, value } => {
                Instruction::AccI32AddLocalConst(tinywasm_types::I32LocalArg { value, local })
            }
            Self::I32AddLocalConstTee { operand, .. } => Instruction::AccI32AddLocalConstTee(operand),
            Self::AccBinOpLocalConst32(packed) => Instruction::AccBinOpLocalConst32(packed),
            Self::AccBinOpLocalLocal32 { op, left, right } => Instruction::AccBinOpLocalLocal32(op, left, right),
            Self::AccBinOpConst32 { op, value } => Instruction::AccBinOpConst32(op, value),
            Self::I32CompareAccLocal { op, right } => Instruction::AccI32CmpLocal(op, right),
            Self::I32CompareAccConst { op, value } => Instruction::AccI32CmpConst(op, value),
            Self::I32CompareLocalConst(packed) => Instruction::AccI32CmpLocalConst(packed),
            Self::I32CompareLocalLocal { op, left, right } => Instruction::AccI32CmpLocalLocal(op, left, right),
            Self::AccBinOpStack64(op) => Instruction::AccBinOpStack64(op),
            Self::AccBinOpStackStack64(op) => Instruction::AccBinOpStackStack64(op),
            Self::AccBinOpLocal64 { op, local } => Instruction::AccBinOpLocal64(op, local),
            Self::AccBinOpConst64(packed) => Instruction::AccBinOpConst64(packed),
            Self::AccBinOpNestedLocalLocal64(packed) => Instruction::AccBinOpNestedLocalLocal64(packed),
        }
    }
}

/// An accumulator operation fused with a following 64-bit local tee.
pub(super) enum Acc64LocalTeeSelection {
    /// Combines a stack operand with `acc64` and writes the result to a local.
    Stack(BinOp),
    /// Combines two stack operands and writes the result to a local.
    StackStack(BinOp),
    /// Combines `acc64` with a local and writes the result to another local.
    Local { op: BinOp, local: LocalAddr },
    /// Combines `acc64` with a constant and writes the result to a local.
    Const(PackedOp64<BinOp, i64>),
    /// Combines `acc64` with a deferred two-local operation and writes the result to a local.
    NestedLocalLocal(PackedOp64<(BinOp, BinOp), (u16, u16)>),
}

/// An accumulator operation that can absorb a following 32-bit local tee.
pub(super) enum Acc32LocalTeeSelection {
    /// Loads a value and writes it to a local while retaining it in `acc32`.
    Load(Operand128Idx<MemoryOperand>),
    /// Adds a constant to a local and writes the result to a local.
    AddLocalConst { local: LocalAddr, value: i32 },
    /// Combines a local and constant and writes the result to a local.
    LocalConst(PackedOp64<BinOp, (u16, u32)>),
    /// Combines two locals and writes the result to a local.
    LocalLocal { op: BinOp, left: LocalAddr, right: LocalAddr },
    /// Combines `acc32` and a constant and writes the result to a local.
    Const { op: BinOp, value: i32 },
}

/// An accumulator producer that can write directly to a 32-bit local and retire the result.
pub(super) enum Acc32LocalSetSelection {
    /// Loads from the current accumulator address and writes the result to a local.
    Load(Operand128Idx<MemoryOperand>),
    /// Loads from a local address and writes the result to a local.
    LocalLoad { op: LoadOp32, memory: Operand128Idx<MemoryOperand>, address: LocalAddr },
    /// Adds a constant to a local and writes the result to a local.
    AddLocalConst { local: LocalAddr, value: i32 },
    /// Combines a local and constant and writes the result to a local.
    LocalConst(PackedOp64<BinOp, (u16, u32)>),
    /// Combines two locals and writes the result to a local.
    LocalLocal { op: BinOp, left: LocalAddr, right: LocalAddr },
    /// Multiplies two stack values and accumulates into the destination local.
    MultiplyAccumulate { floating: bool },
}

/// A selected i32 conditional branch that still needs a target operand.
pub(super) enum I32BranchSelection {
    /// Branches directly on the accumulator before a consumed `i32.eqz`.
    AccEqz,
    /// Branches directly on a local loaded into the accumulator.
    Local(LocalAddr),
    /// Updates a local with a constant and branches on the updated value.
    UpdateLocal { bin_op: BinOp, value: i32, local: LocalAddr, on_zero: bool },
    /// Updates a local, compares the result with another local, and branches.
    UpdateCompareLocal { bin_op: BinOp, value: i32, local: LocalAddr, right: LocalAddr, cmp_op: CmpOp },
    /// Compares two locals and branches.
    CompareLocalLocal { left: LocalAddr, right: LocalAddr, cmp_op: CmpOp },
    /// Compares a local with a constant and branches.
    CompareLocalConst { operand: Operand64Idx<(u16, u32)>, cmp_op: CmpOp },
    /// Compares a stack value with a local and branches.
    CompareStackLocal { local: LocalAddr, cmp_op: CmpOp },
    /// Compares a stack value with a constant and branches.
    CompareStackConst { value: i32, cmp_op: CmpOp },
}

/// Owns committed instructions and the bounded semantic instruction tail.
pub(super) struct InstructionSelector {
    committed: Vec<Instruction>,
    pending: Vec<PendingOp>,
    fusion_boundary: usize,
}

impl InstructionSelector {
    /// Creates a selector with capacity for the final instruction stream.
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self { committed: Vec::with_capacity(capacity), pending: Vec::with_capacity(3), fusion_boundary: 0 }
    }

    /// Stages an i32 local-plus-constant operation.
    pub(super) fn stage_i32_add_local_const(&mut self, local: LocalAddr, value: i32) {
        self.stage(PendingOp::I32AddLocalConst { local, value });
    }

    /// Stages an i32 comparison between the accumulator and a local.
    pub(super) fn stage_i32_compare_acc_local(&mut self, op: CmpOp, right: LocalAddr) {
        self.stage(PendingOp::I32CompareAccLocal { op, right });
    }

    /// Stages an i32 comparison between a local and a constant.
    pub(super) fn stage_i32_compare_local_const(&mut self, packed: PackedOp64<CmpOp, (u16, u32)>) {
        self.stage(PendingOp::I32CompareLocalConst(packed));
    }

    /// Stages an i32 comparison between two locals.
    pub(super) fn stage_i32_compare_local_local(&mut self, op: CmpOp, left: LocalAddr, right: LocalAddr) {
        self.stage(PendingOp::I32CompareLocalLocal { op, left, right });
    }

    /// Stages an accumulator operation that may absorb a following 32-bit local tee.
    pub(super) fn stage_acc_binary32(&mut self, instruction: Instruction) {
        let operation = match instruction {
            Instruction::AccBinOpLocalConst32(packed) => PendingOp::AccBinOpLocalConst32(packed),
            Instruction::AccBinOpLocalLocal32(op, left, right) => PendingOp::AccBinOpLocalLocal32 { op, left, right },
            Instruction::AccBinOpConst32(op, value) => PendingOp::AccBinOpConst32 { op, value },
            Instruction::AccI32CmpConst(op, value) => PendingOp::I32CompareAccConst { op, value },
            instruction => return self.push(instruction),
        };
        self.stage(operation);
    }

    /// Selects an accumulator operation that can absorb a 32-bit local tee.
    pub(super) fn select_acc_local_tee32(&mut self) -> Option<Acc32LocalTeeSelection> {
        if let Some(operation) = self.pending.last() {
            let selection = match operation {
                PendingOp::I32AddLocalConst { local, value } => {
                    Acc32LocalTeeSelection::AddLocalConst { local: *local, value: *value }
                }
                PendingOp::AccBinOpLocalConst32(packed) => Acc32LocalTeeSelection::LocalConst(*packed),
                PendingOp::AccBinOpLocalLocal32 { op, left, right } => {
                    Acc32LocalTeeSelection::LocalLocal { op: *op, left: *left, right: *right }
                }
                PendingOp::AccBinOpConst32 { op, value } => Acc32LocalTeeSelection::Const { op: *op, value: *value },
                _ => return None,
            };
            self.pending.pop();
            return Some(selection);
        }
        if self.committed.len() <= self.fusion_boundary {
            return None;
        }
        match self.committed.last() {
            Some(Instruction::AccLoad32(index)) => {
                let index = *index;
                self.committed.pop();
                Some(Acc32LocalTeeSelection::Load(index))
            }
            _ => None,
        }
    }

    /// Selects an accumulator producer that can write directly to a 32-bit local.
    pub(super) fn select_acc_local_set32(&mut self, destination: LocalAddr) -> Option<Acc32LocalSetSelection> {
        if let Some(operation) = self.pending.last() {
            let selection = match operation {
                PendingOp::I32AddLocalConst { local, value } => {
                    Acc32LocalSetSelection::AddLocalConst { local: *local, value: *value }
                }
                PendingOp::AccBinOpLocalConst32(packed) => Acc32LocalSetSelection::LocalConst(*packed),
                PendingOp::AccBinOpLocalLocal32 { op, left, right } => {
                    Acc32LocalSetSelection::LocalLocal { op: *op, left: *left, right: *right }
                }
                _ => return None,
            };
            self.pending.pop();
            return Some(selection);
        }
        if self.committed.len() <= self.fusion_boundary {
            return None;
        }
        if self.committed.len() >= self.fusion_boundary + 3 {
            let tail = &self.committed[self.committed.len() - 3..];
            if let [Instruction::LocalGet32(local), Instruction::AccConst32(value), Instruction::AccI32AddStack] = tail
            {
                let selection = Acc32LocalSetSelection::AddLocalConst { local: *local, value: *value };
                self.committed.truncate(self.committed.len() - 3);
                return Some(selection);
            }
        }
        if self.committed.len() >= self.fusion_boundary + 2 {
            let tail = &self.committed[self.committed.len() - 2..];
            let floating = match tail {
                [Instruction::AccBinOpStackStack32(BinOp::IMul), Instruction::AccI32AddLocal(local)]
                    if *local == destination =>
                {
                    Some(false)
                }
                [Instruction::AccBinOpStackStack32(BinOp::FMul), Instruction::AccBinOpLocal32(BinOp::FAdd, local)]
                    if *local == destination =>
                {
                    Some(true)
                }
                _ => None,
            };
            if let Some(floating) = floating {
                self.committed.truncate(self.committed.len() - 2);
                return Some(Acc32LocalSetSelection::MultiplyAccumulate { floating });
            }
        }
        if let Some((op, memory, address)) = self.take_acc_local_load32() {
            return Some(Acc32LocalSetSelection::LocalLoad { op, memory, address });
        }
        match self.committed.last() {
            Some(Instruction::AccLoad32(index)) => {
                let index = *index;
                self.committed.pop();
                Some(Acc32LocalSetSelection::Load(index))
            }
            _ => None,
        }
    }

    /// Selects a local-address load whose accumulator result is being materialized.
    pub(super) fn select_acc_local_load32(&mut self) -> Option<(LoadOp32, Operand128Idx<MemoryOperand>, LocalAddr)> {
        if !self.pending.is_empty() {
            return None;
        }
        self.take_acc_local_load32()
    }

    fn take_acc_local_load32(&mut self) -> Option<(LoadOp32, Operand128Idx<MemoryOperand>, LocalAddr)> {
        if self.committed.len() < self.fusion_boundary + 2 {
            return None;
        }
        let pair = &self.committed[self.committed.len() - 2..];
        let selection = match pair {
            [Instruction::AccLocalGet32(address), Instruction::AccLoad32(memory)] => {
                Some((LoadOp32::Full, *memory, *address))
            }
            [Instruction::AccLocalGet32(address), Instruction::AccLoad8S32(memory)] => {
                Some((LoadOp32::I8S, *memory, *address))
            }
            [Instruction::AccLocalGet32(address), Instruction::AccLoad8U32(memory)] => {
                Some((LoadOp32::I8U, *memory, *address))
            }
            [Instruction::AccLocalGet32(address), Instruction::AccLoad16S32(memory)] => {
                Some((LoadOp32::I16S, *memory, *address))
            }
            [Instruction::AccLocalGet32(address), Instruction::AccLoad16U32(memory)] => {
                Some((LoadOp32::I16U, *memory, *address))
            }
            _ => None,
        };
        if selection.is_some() {
            self.committed.truncate(self.committed.len() - 2);
        }
        selection
    }

    /// Selects an in-place 32-bit memory increment through a local address.
    pub(super) fn select_inc_memory_local32(&mut self) -> Option<(Operand128Idx<MemoryOperand>, LocalAddr)> {
        if !self.pending.is_empty() || self.committed.len() < self.fusion_boundary + 4 {
            return None;
        }
        let tail = &self.committed[self.committed.len() - 4..];
        let selection = match tail {
            [
                Instruction::AccLocalGet32(load_address),
                Instruction::AccLoad32(memory),
                Instruction::AccI32AddConst(1),
                Instruction::LocalGet32(store_address),
            ] if load_address == store_address => Some((*memory, *load_address)),
            _ => None,
        };
        if selection.is_some() {
            self.committed.truncate(self.committed.len() - 4);
        }
        selection
    }

    /// Stages a fused local-add, local-tee operation for possible branch selection.
    pub(super) fn stage_i32_add_local_const_tee(
        &mut self,
        local: LocalAddr,
        value: i32,
        destination: LocalAddr,
        operand: Operand64Idx<(u16, u16, u32)>,
    ) {
        self.stage(PendingOp::I32AddLocalConstTee { local, value, destination, operand });
    }

    /// Stages an accumulator operation that may absorb a following 64-bit local tee.
    pub(super) fn stage_acc_binary64(&mut self, instruction: Instruction) {
        let operation = match instruction {
            Instruction::AccBinOpStack64(op) => PendingOp::AccBinOpStack64(op),
            Instruction::AccBinOpStackStack64(op) => PendingOp::AccBinOpStackStack64(op),
            Instruction::AccBinOpLocal64(op, local) => PendingOp::AccBinOpLocal64 { op, local },
            Instruction::AccBinOpConst64(packed) => PendingOp::AccBinOpConst64(packed),
            Instruction::AccBinOpNestedLocalLocal64(packed) => PendingOp::AccBinOpNestedLocalLocal64(packed),
            instruction => return self.push(instruction),
        };
        self.stage(operation);
    }

    /// Selects a fused accumulator operation and 64-bit local tee.
    pub(super) fn select_acc_local_tee64(&mut self) -> Option<Acc64LocalTeeSelection> {
        let selection = match self.pending.last()? {
            PendingOp::AccBinOpStack64(op) => Acc64LocalTeeSelection::Stack(*op),
            PendingOp::AccBinOpStackStack64(op) => Acc64LocalTeeSelection::StackStack(*op),
            PendingOp::AccBinOpLocal64 { op, local } => Acc64LocalTeeSelection::Local { op: *op, local: *local },
            PendingOp::AccBinOpConst64(packed) => Acc64LocalTeeSelection::Const(*packed),
            PendingOp::AccBinOpNestedLocalLocal64(packed) => Acc64LocalTeeSelection::NestedLocalLocal(*packed),
            _ => return None,
        };
        self.pending.pop();
        Some(selection)
    }

    /// Selects the longest conditional branch pattern from the pending tail.
    pub(super) fn select_i32_branch(&mut self, branch_on_nonzero: bool) -> Option<I32BranchSelection> {
        let matched = match self.pending.as_slice() {
            [
                ..,
                PendingOp::I32AddLocalConstTee { local, value, destination: tee_local, .. },
                PendingOp::I32CompareAccLocal { op, right },
            ] if local == tee_local => {
                let cmp_op = if branch_on_nonzero { *op } else { op.inverse() };
                Some((
                    2,
                    I32BranchSelection::UpdateCompareLocal {
                        bin_op: BinOp::IAdd,
                        value: *value,
                        local: *local,
                        right: *right,
                        cmp_op,
                    },
                ))
            }
            [.., PendingOp::I32AddLocalConstTee { local, value, destination, .. }] if local == destination => Some((
                1,
                I32BranchSelection::UpdateLocal {
                    bin_op: BinOp::IAdd,
                    value: *value,
                    local: *local,
                    on_zero: !branch_on_nonzero,
                },
            )),
            [.., PendingOp::I32CompareLocalLocal { op, left, right }] => {
                let cmp_op = if branch_on_nonzero { *op } else { op.inverse() };
                Some((1, I32BranchSelection::CompareLocalLocal { left: *left, right: *right, cmp_op }))
            }
            [.., PendingOp::I32CompareLocalConst(packed)] => {
                let cmp_op = if branch_on_nonzero { packed.op } else { packed.op.inverse() };
                Some((1, I32BranchSelection::CompareLocalConst { operand: packed.index, cmp_op }))
            }
            _ => None,
        };
        if let Some((consumed, selection)) = matched {
            let prefix = self.pending.len() - consumed;
            for operation in self.pending.drain(..prefix) {
                self.committed.push(operation.fallback());
            }
            self.pending.clear();
            return Some(selection);
        }
        let comparison = match self.pending.last() {
            Some(PendingOp::I32CompareAccLocal { op, right }) => {
                Some(I32BranchSelection::CompareStackLocal { local: *right, cmp_op: *op })
            }
            Some(PendingOp::I32CompareAccConst { op, value }) => {
                Some(I32BranchSelection::CompareStackConst { value: *value, cmp_op: *op })
            }
            _ => None,
        };
        if let Some(mut selection) = comparison {
            let comparison = self.pending.pop().expect("selected pending comparison");
            self.flush();
            if self.fuse_push_acc32() {
                match &mut selection {
                    I32BranchSelection::CompareStackLocal { cmp_op, .. }
                    | I32BranchSelection::CompareStackConst { cmp_op, .. } => {
                        if !branch_on_nonzero {
                            *cmp_op = cmp_op.inverse();
                        }
                    }
                    _ => unreachable!(),
                }
                return Some(selection);
            }
            self.committed.push(comparison.fallback());
        }
        if !self.pending.is_empty() || self.committed.len() <= self.fusion_boundary {
            return None;
        }
        if self.committed.len() >= self.fusion_boundary + 3 {
            let tail = &self.committed[self.committed.len() - 3..];
            if let [
                Instruction::LocalGet32(left),
                Instruction::LocalGet32(right),
                Instruction::AccI32CmpStackStack(op),
            ] = tail
            {
                let selection = I32BranchSelection::CompareLocalLocal {
                    left: *left,
                    right: *right,
                    cmp_op: if branch_on_nonzero { *op } else { op.inverse() },
                };
                self.committed.truncate(self.committed.len() - 3);
                return Some(selection);
            }
        }
        if self.committed.len() >= self.fusion_boundary + 2 {
            let tail = &self.committed[self.committed.len() - 2..];
            let selection = match tail {
                [Instruction::AccConst32(value), Instruction::AccI32CmpStack(op)] => {
                    Some(I32BranchSelection::CompareStackConst {
                        value: *value,
                        cmp_op: if branch_on_nonzero { *op } else { op.inverse() },
                    })
                }
                [Instruction::AccLocalGet32(local), Instruction::AccI32CmpStack(op)] => {
                    Some(I32BranchSelection::CompareStackLocal {
                        local: *local,
                        cmp_op: if branch_on_nonzero { *op } else { op.inverse() },
                    })
                }
                _ => None,
            };
            if selection.is_some() {
                self.committed.truncate(self.committed.len() - 2);
                return selection;
            }
        }
        match self.committed.last() {
            Some(Instruction::AccI32Eqz) => {
                self.committed.pop();
                return Some(I32BranchSelection::AccEqz);
            }
            Some(Instruction::AccLocalGet32(local)) => {
                let local = *local;
                self.committed.pop();
                return Some(I32BranchSelection::Local(local));
            }
            _ => {}
        }
        None
    }

    /// Commits an instruction that cannot participate in staged selection.
    pub(super) fn push(&mut self, instruction: Instruction) {
        self.flush();
        if self.fuse_stack_local_write(instruction) {
            return;
        }
        if instruction == Instruction::PushAcc32 && self.fuse_push_acc32() {
            return;
        }
        if instruction == Instruction::PushAcc64 && self.fuse_push_acc64() {
            return;
        }
        self.committed.push(instruction);
    }

    fn fuse_stack_local_write(&mut self, instruction: Instruction) -> bool {
        if self.committed.len() <= self.fusion_boundary {
            return false;
        }
        let Some(previous) = self.committed.last_mut() else { return false };
        *previous = match (instruction, *previous) {
            (Instruction::LocalSet32(destination), Instruction::LocalGet32(source)) => {
                Instruction::LocalCopy32(source, destination)
            }
            (Instruction::LocalSet64(destination), Instruction::LocalGet64(source)) => {
                Instruction::LocalCopy64(source, destination)
            }
            (Instruction::LocalSet128(destination), Instruction::LocalGet128(source)) => {
                Instruction::LocalCopy128(source, destination)
            }
            (Instruction::LocalSet32(destination), Instruction::Const32(value)) => {
                Instruction::SetLocalConst32(tinywasm_types::I32LocalArg { value, local: destination })
            }
            (Instruction::LocalSet32(destination), Instruction::AddLocalConst32(argument))
                if argument.local == destination =>
            {
                Instruction::IncLocal32(argument)
            }
            (Instruction::LocalSet32(destination), Instruction::LoadLocal32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalSet32(argument)
            }
            (Instruction::LocalSet32(destination), Instruction::LoadLocal8S32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalSet8S32(argument)
            }
            (Instruction::LocalSet32(destination), Instruction::LoadLocal8U32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalSet8U32(argument)
            }
            (Instruction::LocalSet32(destination), Instruction::LoadLocal16S32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalSet16S32(argument)
            }
            (Instruction::LocalSet32(destination), Instruction::LoadLocal16U32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalSet16U32(argument)
            }
            (Instruction::LocalTee32(destination), Instruction::LoadLocal32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalTee32(argument)
            }
            (Instruction::LocalTee32(destination), Instruction::LoadLocal8S32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalTee8S32(argument)
            }
            (Instruction::LocalTee32(destination), Instruction::LoadLocal8U32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalTee8U32(argument)
            }
            (Instruction::LocalTee32(destination), Instruction::LoadLocal16S32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalTee16S32(argument)
            }
            (Instruction::LocalTee32(destination), Instruction::LoadLocal16U32(mut argument))
                if let Ok(destination) = u8::try_from(destination) =>
            {
                argument.local2 = destination;
                Instruction::LoadLocalTee16U32(argument)
            }
            (Instruction::LocalTee32(destination), Instruction::BinOpLocalLocal32(BinOp::IAdd, left, right)) => {
                Instruction::AddLocalLocalTee32(tinywasm_types::LocalTripleArg { left, right, dst: destination })
            }
            _ => return false,
        };
        true
    }

    fn fuse_push_acc32(&mut self) -> bool {
        if self.committed.len() <= self.fusion_boundary {
            return false;
        }
        let Some(instruction) = self.committed.last_mut() else { return false };
        *instruction = match *instruction {
            Instruction::LocalGet32(local) => Instruction::LocalGetPushAcc32(local),
            Instruction::Const32(value) => Instruction::ConstPushAcc32(value),
            Instruction::AccLocalGet32(local) => Instruction::AccLocalGetPush32(local),
            Instruction::AccUnaryStack32(op) => Instruction::AccUnaryStackPush32(op),
            Instruction::AccLoad32(index) => Instruction::AccLoadPush32(PackedOp::new(LoadOp32::Full, index)),
            Instruction::AccLoad8S32(index) => Instruction::AccLoadPush32(PackedOp::new(LoadOp32::I8S, index)),
            Instruction::AccLoad8U32(index) => Instruction::AccLoadPush32(PackedOp::new(LoadOp32::I8U, index)),
            Instruction::AccLoad16S32(index) => Instruction::AccLoadPush32(PackedOp::new(LoadOp32::I16S, index)),
            Instruction::AccLoad16U32(index) => Instruction::AccLoadPush32(PackedOp::new(LoadOp32::I16U, index)),
            Instruction::AccLoadTee32(argument) => Instruction::AccLoadTeePush32(argument),
            Instruction::AccLoadStack32(argument) => Instruction::AccLoadStackPush32(argument),
            Instruction::AccI32AddStack => Instruction::AccBinOpStackPush32(BinOp::IAdd),
            Instruction::AccI32AddLocal(local) => Instruction::AccBinOpLocalPush32(BinOp::IAdd, local),
            Instruction::AccI32AddConst(value) => Instruction::AccBinOpConstPush32(BinOp::IAdd, value),
            Instruction::AccI32AddLocalConst(argument) => Instruction::AccI32AddLocalConstPush(argument),
            Instruction::AccI32AddLocalConstTee(index) => Instruction::AccI32AddLocalConstTeePush(index),
            Instruction::AccBinOpLocalConst32(argument) => Instruction::AccBinOpLocalConstPush32(argument),
            Instruction::AccBinOpLocalConstTee32(argument) => Instruction::AccBinOpLocalConstTeePush32(argument),
            Instruction::AccBinOpLocalLocal32(op, lhs, rhs) => Instruction::AccBinOpLocalLocalPush32(op, lhs, rhs),
            Instruction::AccBinOpLocalLocalTee32(argument) => Instruction::AccBinOpLocalLocalTeePush32(argument),
            Instruction::AccBinOpStackStack32(op) => Instruction::AccBinOpStackStackPush32(op),
            Instruction::AccBinOpStack32(op) => Instruction::AccBinOpStackPush32(op),
            Instruction::AccI32CmpStackStack(op) => Instruction::AccI32CmpStackStackPush32(op),
            Instruction::AccBinOpLocal32(op, local) => Instruction::AccBinOpLocalPush32(op, local),
            Instruction::AccI32CmpLocal(op, local) => Instruction::AccI32CmpLocalPush32(op, local),
            Instruction::AccBinOpConst32(op, value) => Instruction::AccBinOpConstPush32(op, value),
            Instruction::AccBinOpConstTee32(argument) => Instruction::AccBinOpConstTeePush32(argument),
            Instruction::AccI32CmpConst(op, value) => Instruction::AccI32CmpConstPush32(op, value),
            Instruction::AccLocalTee32(local) => Instruction::AccLocalTeePush32(local),
            Instruction::AccSelect32 => Instruction::AccSelectPush32,
            Instruction::AccConvertStack64To32(op) => Instruction::AccConvertStack64To32Push32(op),
            _ => return false,
        };
        true
    }

    fn fuse_push_acc64(&mut self) -> bool {
        if self.committed.len() <= self.fusion_boundary {
            return false;
        }
        let Some(instruction) = self.committed.last_mut() else { return false };
        *instruction = match *instruction {
            Instruction::AccLocalGetPush32(local) => Instruction::AccLocalGetPush32Push64(local),
            Instruction::AccI32AddLocalConstPush(argument) => Instruction::AccI32AddLocalConstPush32Push64(argument),
            Instruction::AccI32AddLocalConstTeePush(index) => Instruction::AccI32AddLocalConstTeePush32Push64(index),
            Instruction::LocalGetPushAcc32(local) => Instruction::LocalGetPushAcc32PushAcc64(local),
            Instruction::AccConvertStack32To64(op) => Instruction::AccConvertStack32To64Push64(op),
            Instruction::AccLoad64Addr32(index) => Instruction::AccLoad64Addr32Push64(index),
            Instruction::AccLoadStack64(argument) => Instruction::AccLoadStackPush64(argument),
            Instruction::AccBinOpStackStack64(op) => Instruction::AccBinOpStackStackPush64(op),
            Instruction::AccBinOpStackStackTee64(op, local) => Instruction::AccBinOpStackStackTeePush64(op, local),
            Instruction::AccBinOpLocal64(op, local) => Instruction::AccBinOpLocalPush64(op, local),
            Instruction::AccBinOpLocalTee64(op, local, destination) => {
                Instruction::AccBinOpLocalTeePush64(op, local, destination)
            }
            Instruction::AccBinOpConst64(argument) => Instruction::AccBinOpConstPush64(argument),
            Instruction::AccBinOpConstTee64(argument) => Instruction::AccBinOpConstTeePush64(argument),
            Instruction::AccBinOpLocalConst64(argument) => Instruction::AccBinOpLocalConstPush64(argument),
            Instruction::AccBinOpLocalLocal64(op, lhs, rhs) => Instruction::AccBinOpLocalLocalPush64(op, lhs, rhs),
            Instruction::AccBinOpNestedLocalConst64(argument) => Instruction::AccBinOpNestedLocalConstPush64(argument),
            Instruction::AccLocalTee64(local) => Instruction::AccLocalTeePush64(local),
            _ => return false,
        };
        true
    }

    /// Returns the last committed instruction after flushing staged operations.
    pub(super) fn last(&mut self) -> Option<&Instruction> {
        self.flush();
        self.committed.last()
    }

    /// Removes the last committed instruction after flushing staged operations.
    pub(super) fn pop(&mut self) -> Option<Instruction> {
        self.flush();
        let instruction = self.committed.pop();
        self.fusion_boundary = self.fusion_boundary.min(self.committed.len());
        instruction
    }

    /// Appends committed instructions after flushing staged operations.
    pub(super) fn extend(&mut self, instructions: impl IntoIterator<Item = Instruction>) {
        self.flush();
        self.committed.extend(instructions);
    }

    /// Returns the committed length after flushing staged operations.
    pub(super) fn len(&mut self) -> usize {
        self.flush();
        self.fusion_boundary = self.committed.len();
        self.fusion_boundary
    }

    /// Finishes the selector and returns immutable-position instructions.
    pub(super) fn finish(mut self) -> Vec<Instruction> {
        self.flush();
        self.committed
    }

    fn stage(&mut self, operation: PendingOp) {
        if self.pending.len() == 3 {
            let oldest = self.pending.remove(0);
            self.committed.push(oldest.fallback());
        }
        self.pending.push(operation);
    }

    fn flush(&mut self) {
        for operation in self.pending.drain(..) {
            self.committed.push(operation.fallback());
        }
    }
}
