use super::*;
use tinywasm_types::{I32LocalArg, Operand64, Operand128, PackedOp};

impl FunctionBuilder<'_> {
    /// Emits an operation with stack-based reference inputs and an `acc32` result.
    pub(super) fn emit_accumulator_reference32(
        &mut self,
        inputs: &[ValueLane],
        instruction: Instruction,
    ) -> Result<()> {
        if self.has_polymorphic_inputs(inputs.len()) {
            return self.apply_effect(inputs, &[ValueLane::S32]);
        }
        self.materialize_lane(ValueLane::S32);
        inputs.iter().rev().try_for_each(|&lane| self.pop_stack_value(lane))?;
        self.push_accumulator32(ValueLane::S32)?;
        self.instructions.push(instruction);
        Ok(())
    }

    /// Emits a 32-bit unary operation through `acc32` or its stack fallback.
    pub(super) fn emit_accumulator_unary32(&mut self, accumulator: Instruction, op: UnaryOp32) -> Result<()> {
        if self.has_polymorphic_inputs(1) {
            return self.apply_effect(&[ValueLane::S32], &[ValueLane::S32]);
        }
        if self.try_emit_register_unary(accumulator)? {
            return Ok(());
        }
        self.materialize_lane(ValueLane::S32);
        self.pop_stack_value(ValueLane::S32)?;
        self.push_accumulator32(ValueLane::S32)?;
        self.instructions.push(Instruction::AccUnaryStack32(op));
        Ok(())
    }

    /// Emits a 64-bit unary operation through `acc64` or its stack fallback.
    pub(super) fn emit_accumulator_unary64(&mut self, accumulator: Instruction, op: UnaryOp64) -> Result<()> {
        if self.has_polymorphic_inputs(1) {
            return self.apply_effect(&[ValueLane::S64], &[ValueLane::S64]);
        }
        if self.try_emit_register_unary64(accumulator)? {
            return Ok(());
        }
        self.materialize_lane(ValueLane::S64);
        self.pop_stack_value(ValueLane::S64)?;
        self.push_accumulator64(ValueLane::S64)?;
        self.instructions.push(Instruction::AccUnaryStack64(op));
        Ok(())
    }

    /// Emits a conversion between scalar accumulator lanes or its stack fallback.
    pub(super) fn emit_accumulator_cross_lane(
        &mut self,
        source: ValueLane,
        destination: ValueLane,
        accumulator: Instruction,
        stack_accumulator: Instruction,
    ) -> Result<()> {
        if self.has_polymorphic_inputs(1) {
            return self.apply_effect(&[source], &[destination]);
        }
        if self.try_emit_cross_lane(source, destination, accumulator)? {
            return Ok(());
        }
        self.materialize_lane(source);
        self.materialize_lane(destination);
        self.pop_stack_value(source)?;
        self.push_register_result(destination)?;
        self.instructions.push(stack_accumulator);
        Ok(())
    }

    /// Emits a 32-bit binary operation through `acc32` or its stack fallback.
    pub(super) fn emit_accumulator_binary32(&mut self, op: RegisterOp32) -> Result<()> {
        if self.has_polymorphic_inputs(2) {
            return self.apply_effect(&[ValueLane::S32, ValueLane::S32], &[ValueLane::S32]);
        }
        if self.try_emit_register_binary32(op)? {
            return Ok(());
        }
        self.materialize_lane(ValueLane::S32);
        self.pop_stack_value(ValueLane::S32)?;
        self.pop_stack_value(ValueLane::S32)?;
        self.push_accumulator32(ValueLane::S32)?;
        self.instructions.push(Self::register_stack_stack_binary_instruction(op));
        Ok(())
    }

    /// Emits a 64-bit binary operation through an accumulator or its stack fallback.
    pub(super) fn emit_accumulator_binary64(&mut self, op: RegisterOp64) -> Result<()> {
        if self.has_polymorphic_inputs(2) {
            return self.apply_effect(&[ValueLane::S64, ValueLane::S64], &[op.result_lane()]);
        }
        if self.try_emit_register_binary64(op)? {
            return Ok(());
        }
        self.materialize_lane(ValueLane::S64);
        let destination = op.result_lane();
        if destination == ValueLane::S32 {
            self.materialize_lane(ValueLane::S32);
        }
        self.pop_stack_value(ValueLane::S64)?;
        self.pop_stack_value(ValueLane::S64)?;
        self.push_register_result(destination)?;
        self.emit_selected_acc64_instruction(Self::register_stack_stack_binary64_instruction(op));
        Ok(())
    }

    /// Emits a checked 32-bit integer operation with an accumulator result.
    pub(super) fn emit_accumulator_int_binary32(&mut self, op: IntBinOp) -> Result<()> {
        if self.has_polymorphic_inputs(2) {
            return self.apply_effect(&[ValueLane::S32, ValueLane::S32], &[ValueLane::S32]);
        }
        if self.top_is_accumulator32() {
            self.pop_accumulator32(ValueLane::S32)?;
            self.materialize_lane(ValueLane::S32);
            self.pop_stack_value(ValueLane::S32)?;
            self.push_accumulator32(ValueLane::S32)?;
            self.instructions.push(Instruction::AccIntBinOpStack32(op));
            return Ok(());
        }
        self.materialize_lane(ValueLane::S32);
        self.pop_stack_value(ValueLane::S32)?;
        self.pop_stack_value(ValueLane::S32)?;
        self.push_accumulator32(ValueLane::S32)?;
        self.instructions.push(Instruction::AccIntBinOpStackStack32(op));
        Ok(())
    }

    /// Emits a checked 64-bit integer operation with an accumulator result.
    pub(super) fn emit_accumulator_int_binary64(&mut self, op: IntBinOp) -> Result<()> {
        if self.has_polymorphic_inputs(2) {
            return self.apply_effect(&[ValueLane::S64, ValueLane::S64], &[ValueLane::S64]);
        }
        if self.top_is_accumulator64() {
            self.pop_accumulator64(ValueLane::S64)?;
            self.materialize_lane(ValueLane::S64);
            self.pop_stack_value(ValueLane::S64)?;
            self.push_accumulator64(ValueLane::S64)?;
            self.instructions.push(Instruction::AccIntBinOpStack64(op));
            return Ok(());
        }
        self.materialize_lane(ValueLane::S64);
        self.pop_stack_value(ValueLane::S64)?;
        self.pop_stack_value(ValueLane::S64)?;
        self.push_accumulator64(ValueLane::S64)?;
        self.instructions.push(Instruction::AccIntBinOpStackStack64(op));
        Ok(())
    }

    fn try_emit_register_binary32(&mut self, op: RegisterOp32) -> Result<bool> {
        let rhs = self.operand_stack.last().copied();
        let lhs = self.operand_stack.iter().rev().nth(1).copied();
        match (lhs, rhs) {
            (Some(ValueLocation::Accumulator32), Some(rhs)) if rhs.is_deferred_binary32() => {
                let RegisterOp32::Bin(outer) = op else { return Ok(false) };
                self.replace_binary_with_register()?;
                let instruction = self.nested_binary32_instruction(outer, rhs)?;
                self.instructions.push(instruction);
            }
            (Some(lhs), Some(ValueLocation::Accumulator32)) if lhs.is_numeric32_source() && op.is_commutative() => {
                self.replace_binary_with_register()?;
                self.emit_selected_i32_instruction(Self::numeric32_direct_instruction(op, lhs));
            }
            (Some(ValueLocation::Accumulator32), Some(rhs)) if rhs.is_numeric32_source() => {
                self.replace_binary_with_register()?;
                self.emit_selected_i32_instruction(Self::numeric32_direct_instruction(op, rhs));
            }
            (Some(lhs), Some(rhs)) if self.accumulator32_is_live() => {
                let Some(deferred) = Self::deferred_binary32(op, lhs, rhs) else { return Ok(false) };
                self.replace_binary_with_deferred(ValueLane::S32, deferred)?;
            }
            (Some(lhs), Some(rhs))
                if lhs.is_numeric32_source() && rhs.is_numeric32_source() && !self.accumulator32_is_live() =>
            {
                self.replace_binary_with_register()?;
                if let Some(instruction) = self.numeric32_source_binary_instruction(op, lhs, rhs)? {
                    self.emit_selected_i32_instruction(instruction);
                } else {
                    self.instructions.push(Self::numeric32_source_instruction(lhs));
                    self.emit_selected_i32_instruction(Self::numeric32_direct_instruction(op, rhs));
                }
            }
            (Some(ValueLocation::Stack(ValueLane::S32)), Some(rhs))
                if rhs.is_numeric32_source() && !self.accumulator32_is_live() =>
            {
                self.pop_deferred_value(ValueLane::S32)?;
                self.pop_stack_value(ValueLane::S32)?;
                self.push_accumulator32(ValueLane::S32)?;
                self.instructions.push(Self::numeric32_source_instruction(rhs));
                self.instructions.push(Self::register_stack_binary_instruction(op));
            }
            (Some(ValueLocation::Stack(ValueLane::S32)), Some(ValueLocation::Accumulator32)) => {
                self.pop_accumulator32(ValueLane::S32)?;
                self.pop_stack_value(ValueLane::S32)?;
                self.push_accumulator32(ValueLane::S32)?;
                self.instructions.push(Self::register_stack_binary_instruction(op));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Lowers a scalar memory instruction through a live accumulator when possible.
    pub(super) fn try_emit_register_memory(&mut self, instruction: Instruction) -> Result<bool> {
        let local_store = match instruction {
            Instruction::I32Store(index) | Instruction::F32Store(index) => Some((index, ValueLane::S32)),
            Instruction::I64Store(index) | Instruction::F64Store(index) => Some((index, ValueLane::S64)),
            _ => None,
        };
        if let Some((memory_index, value_lane)) = local_store {
            let address_lane = self.metadata.memory_size(self.data.operand128(memory_index).memory())?;
            let local_pair = self.operand_stack.as_slice().split_last().and_then(|(value, rest)| {
                let address = rest.last()?;
                let address = match (address_lane, *address) {
                    (ValueLane::S32, ValueLocation::LocalNumeric32(local))
                    | (ValueLane::S64, ValueLocation::LocalNumeric64(local)) => u8::try_from(local).ok()?,
                    _ => return None,
                };
                let value = match (value_lane, *value) {
                    (ValueLane::S32, ValueLocation::LocalNumeric32(local))
                    | (ValueLane::S64, ValueLocation::LocalNumeric64(local)) => u8::try_from(local).ok()?,
                    _ => return None,
                };
                Some((address, value))
            });
            if let Some((address, value)) = local_pair
                && let Ok(memory) = CompactMemoryArg::try_from(self.data.operand128(memory_index))
            {
                let memory_arg_idx = self.push_operand64(Operand64::from(memory))?;
                self.pop_deferred_value(value_lane)?;
                self.pop_deferred_value(address_lane)?;
                let argument = MemoryLocalArg { memory_arg_idx, local1: address, local2: value };
                let store = match value_lane {
                    ValueLane::S32 => Instruction::StoreLocalLocal32(argument),
                    ValueLane::S64 => Instruction::StoreLocalLocal64(argument),
                    ValueLane::S128 => unreachable!(),
                };
                self.instructions.push(store);
                return Ok(true);
            }
        }

        if self.top_is_accumulator32()
            && let Some((store, memory_index)) = Self::accumulator_store_instruction(instruction)
        {
            let address_lane = self.metadata.memory_size(self.data.operand128(memory_index).memory())?;
            self.materialize_lane_prefix(address_lane, self.operand_stack.len() - 1);
            self.pop_accumulator32(ValueLane::S32)?;
            self.pop_stack_value(address_lane)?;
            if matches!(store, Instruction::AccStore32(_))
                && let Some((load_memory_index, address)) = self.instructions.select_inc_memory_local32()
            {
                let load_memory = self.data.operand128(load_memory_index);
                let store_memory = self.data.operand128(memory_index);
                if load_memory == store_memory
                    && let (Ok(memory), Ok(address)) = (CompactMemoryArg::try_from(store_memory), u8::try_from(address))
                {
                    let memory_arg_idx = self.push_operand64(Operand64::from(memory))?;
                    self.instructions.push(Instruction::IncMemoryLocal32(MemoryLocalArg {
                        memory_arg_idx,
                        local1: address,
                        local2: 0,
                    }));
                    return Ok(true);
                }
                self.instructions.push(Instruction::AccLocalGet32(address));
                self.instructions.push(Instruction::AccLoad32(load_memory_index));
                self.instructions.push(Instruction::AccI32AddConst(1));
                self.instructions.push(Instruction::LocalGet32(address));
            }
            self.instructions.push(store);
            return Ok(true);
        }
        if self.top_is_accumulator64()
            && let Some((store, memory_index)) = Self::accumulator_store64_instruction(instruction)
        {
            let address_lane = self.metadata.memory_size(self.data.operand128(memory_index).memory())?;
            self.materialize_lane_prefix(address_lane, self.operand_stack.len() - 1);
            self.pop_accumulator64(ValueLane::S64)?;
            self.pop_stack_value(address_lane)?;
            self.instructions.push(store);
            return Ok(true);
        }

        let memory_index = match instruction {
            Instruction::I32Load(index)
            | Instruction::F32Load(index)
            | Instruction::I32Load8S(index)
            | Instruction::I32Load8U(index)
            | Instruction::I32Load16S(index)
            | Instruction::I32Load16U(index)
            | Instruction::I64Load(index)
            | Instruction::F64Load(index)
            | Instruction::I64Load8S(index)
            | Instruction::I64Load8U(index)
            | Instruction::I64Load16S(index)
            | Instruction::I64Load16U(index)
            | Instruction::I64Load32S(index)
            | Instruction::I64Load32U(index) => index,
            _ => return Ok(false),
        };
        let address = self.metadata.memory_size(self.data.operand128(memory_index).memory())?;
        let location = self.operand_stack.last().copied();
        if address == ValueLane::S32
            && location.is_some_and(ValueLocation::is_numeric32_source)
            && !self.accumulator32_is_live()
        {
            let location = location.unwrap();
            self.replace_top_location(ValueLocation::Accumulator32)?;
            self.instructions.push(Self::numeric32_source_instruction(location));
        }
        if address == ValueLane::S64
            && location.is_some_and(ValueLocation::is_numeric64_source)
            && !self.accumulator64_is_live()
        {
            let location = location.unwrap();
            self.replace_top_location(ValueLocation::Accumulator64)?;
            let source = self.numeric64_source_instruction(location)?;
            self.instructions.push(source);
        }
        if address == ValueLane::S32 && self.top_is_accumulator32() {
            if let Some(load) = Self::accumulator_load_instruction(instruction) {
                self.instructions.push(load);
                return Ok(true);
            }
            if !self.accumulator64_is_live()
                && let Some(load) = Self::accumulator_load64_instruction(instruction, address)
            {
                self.pop_accumulator32(ValueLane::S32)?;
                self.push_accumulator64(ValueLane::S64)?;
                self.instructions.push(load);
                return Ok(true);
            }
        }
        if address == ValueLane::S64 && self.top_is_accumulator64() {
            if let Some(load) = Self::accumulator_load64_instruction(instruction, address) {
                self.instructions.push(load);
                return Ok(true);
            }
            if !self.accumulator32_is_live()
                && let Some(load) = Self::accumulator_load32_addr64_instruction(instruction)
            {
                self.pop_accumulator64(ValueLane::S64)?;
                self.push_accumulator32(ValueLane::S32)?;
                self.instructions.push(load);
                return Ok(true);
            }
        }
        if let Some((destination, load)) = Self::accumulator_stack_load_instruction(instruction) {
            self.materialize_lane(address);
            if destination != address {
                self.materialize_lane(destination);
            }
            self.pop_stack_value(address)?;
            self.push_register_result(destination)?;
            self.instructions.push(load);
            return Ok(true);
        }
        Ok(false)
    }

    fn accumulator_stack_load_instruction(instruction: Instruction) -> Option<(ValueLane, Instruction)> {
        Some(match instruction {
            Instruction::I32Load(index) | Instruction::F32Load(index) => {
                (ValueLane::S32, Instruction::AccLoadStack32(PackedOp::new(LoadOp32::Full, index)))
            }
            Instruction::I32Load8S(index) => {
                (ValueLane::S32, Instruction::AccLoadStack32(PackedOp::new(LoadOp32::I8S, index)))
            }
            Instruction::I32Load8U(index) => {
                (ValueLane::S32, Instruction::AccLoadStack32(PackedOp::new(LoadOp32::I8U, index)))
            }
            Instruction::I32Load16S(index) => {
                (ValueLane::S32, Instruction::AccLoadStack32(PackedOp::new(LoadOp32::I16S, index)))
            }
            Instruction::I32Load16U(index) => {
                (ValueLane::S32, Instruction::AccLoadStack32(PackedOp::new(LoadOp32::I16U, index)))
            }
            Instruction::I64Load(index) | Instruction::F64Load(index) => {
                (ValueLane::S64, Instruction::AccLoadStack64(PackedOp::new(LoadOp64::Full, index)))
            }
            Instruction::I64Load8S(index) => {
                (ValueLane::S64, Instruction::AccLoadStack64(PackedOp::new(LoadOp64::I8S, index)))
            }
            Instruction::I64Load8U(index) => {
                (ValueLane::S64, Instruction::AccLoadStack64(PackedOp::new(LoadOp64::I8U, index)))
            }
            Instruction::I64Load16S(index) => {
                (ValueLane::S64, Instruction::AccLoadStack64(PackedOp::new(LoadOp64::I16S, index)))
            }
            Instruction::I64Load16U(index) => {
                (ValueLane::S64, Instruction::AccLoadStack64(PackedOp::new(LoadOp64::I16U, index)))
            }
            Instruction::I64Load32S(index) => {
                (ValueLane::S64, Instruction::AccLoadStack64(PackedOp::new(LoadOp64::I32S, index)))
            }
            Instruction::I64Load32U(index) => {
                (ValueLane::S64, Instruction::AccLoadStack64(PackedOp::new(LoadOp64::I32U, index)))
            }
            _ => return None,
        })
    }

    fn try_emit_register_unary(&mut self, instruction: Instruction) -> Result<bool> {
        let Some(location) = self.operand_stack.last().copied() else {
            return Ok(false);
        };
        match location {
            ValueLocation::Accumulator32 => {
                self.instructions.push(instruction);
            }
            location if location.is_numeric32_source() && !self.accumulator32_is_live() => {
                self.replace_top_location(ValueLocation::Accumulator32)?;
                self.instructions.push(Self::numeric32_source_instruction(location));
                self.instructions.push(instruction);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn try_emit_register_unary64(&mut self, instruction: Instruction) -> Result<bool> {
        let Some(location) = self.operand_stack.last().copied() else {
            return Ok(false);
        };
        match location {
            ValueLocation::Accumulator64 => {
                self.instructions.push(instruction);
            }
            location if location.is_numeric64_source() && !self.accumulator64_is_live() => {
                self.replace_top_location(ValueLocation::Accumulator64)?;
                let source = self.numeric64_source_instruction(location)?;
                self.instructions.push(source);
                self.instructions.push(instruction);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn try_emit_cross_lane(
        &mut self,
        source: ValueLane,
        destination: ValueLane,
        instruction: Instruction,
    ) -> Result<bool> {
        let Some(location) = self.operand_stack.last().copied() else {
            return Ok(false);
        };
        let source_is_accumulator = matches!(
            (source, location),
            (ValueLane::S32, ValueLocation::Accumulator32) | (ValueLane::S64, ValueLocation::Accumulator64)
        );
        let destination_is_free = match destination {
            ValueLane::S32 => !self.accumulator32_is_live(),
            ValueLane::S64 => !self.accumulator64_is_live(),
            ValueLane::S128 => false,
        };
        if !source_is_accumulator || !destination_is_free {
            return Ok(false);
        }
        match source {
            ValueLane::S32 => self.pop_accumulator32(source)?,
            ValueLane::S64 => self.pop_accumulator64(source)?,
            ValueLane::S128 => unreachable!(),
        }
        match destination {
            ValueLane::S32 => self.push_accumulator32(destination)?,
            ValueLane::S64 => self.push_accumulator64(destination)?,
            ValueLane::S128 => unreachable!(),
        }
        self.instructions.push(instruction);
        Ok(true)
    }

    fn try_emit_register_binary64(&mut self, op: RegisterOp64) -> Result<bool> {
        let rhs = self.operand_stack.last().copied();
        let lhs = self.operand_stack.iter().rev().nth(1).copied();
        let destination_free = match op.result_lane() {
            ValueLane::S32 => !self.accumulator32_is_live(),
            ValueLane::S64 => !self.accumulator64_is_live(),
            ValueLane::S128 => false,
        };
        match (lhs, rhs) {
            (Some(ValueLocation::Accumulator64), Some(rhs)) if rhs.is_deferred_binary64() => {
                let RegisterOp64::Bin(outer) = op else { return Ok(false) };
                self.replace_binary64_with_register(op.result_lane())?;
                self.emit_nested_binary64(outer, rhs)?;
            }
            (Some(lhs), Some(ValueLocation::Accumulator64))
                if lhs.is_numeric64_source() && op.is_commutative() && self.acc64_binary_destination_available(op) =>
            {
                self.replace_binary64_with_register(op.result_lane())?;
                let instruction = self.numeric64_direct_instruction(op, lhs)?;
                self.emit_selected_acc64_instruction(instruction);
            }
            (Some(ValueLocation::Accumulator64), Some(rhs))
                if rhs.is_numeric64_source() && self.acc64_binary_destination_available(op) =>
            {
                self.replace_binary64_with_register(op.result_lane())?;
                let instruction = self.numeric64_direct_instruction(op, rhs)?;
                self.emit_selected_acc64_instruction(instruction);
            }
            (Some(lhs), Some(rhs)) if self.accumulator64_is_live() => {
                let Some(deferred) = Self::deferred_binary64(op, lhs, rhs) else { return Ok(false) };
                self.replace_binary_with_deferred(ValueLane::S64, deferred)?;
            }
            (Some(lhs), Some(rhs)) if lhs.is_numeric64_source() && rhs.is_numeric64_source() && destination_free => {
                let direct = self.numeric64_source_binary_instruction(op, lhs, rhs)?;
                if direct.is_none() && self.accumulator64_is_live() {
                    return Ok(false);
                }
                self.replace_binary64_with_register(op.result_lane())?;
                if let Some(instruction) = direct {
                    self.emit_selected_acc64_instruction(instruction);
                } else {
                    let source = self.numeric64_source_instruction(lhs)?;
                    self.instructions.push(source);
                    let instruction = self.numeric64_direct_instruction(op, rhs)?;
                    self.emit_selected_acc64_instruction(instruction);
                }
            }
            (Some(ValueLocation::Stack(ValueLane::S64)), Some(rhs))
                if rhs.is_numeric64_source() && destination_free && !self.accumulator64_is_live() =>
            {
                self.pop_deferred_value(ValueLane::S64)?;
                self.pop_stack_value(ValueLane::S64)?;
                self.push_register_result(op.result_lane())?;
                let source = self.numeric64_source_instruction(rhs)?;
                self.instructions.push(source);
                self.emit_selected_acc64_instruction(Self::register_stack_binary64_instruction(op));
            }
            (Some(ValueLocation::Stack(ValueLane::S64)), Some(ValueLocation::Accumulator64))
                if self.acc64_binary_destination_available(op) =>
            {
                self.pop_accumulator64(ValueLane::S64)?;
                self.pop_stack_value(ValueLane::S64)?;
                self.push_register_result(op.result_lane())?;
                self.emit_selected_acc64_instruction(Self::register_stack_binary64_instruction(op));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn acc64_binary_destination_available(&self, op: RegisterOp64) -> bool {
        match op.result_lane() {
            ValueLane::S64 => true,
            ValueLane::S32 => !self.accumulator32_is_live(),
            ValueLane::S128 => false,
        }
    }

    fn replace_binary64_with_register(&mut self, result: ValueLane) -> Result<()> {
        for _ in 0..2 {
            if self.top_is_accumulator64() {
                self.pop_accumulator64(ValueLane::S64)?;
            } else {
                self.pop_deferred_value(ValueLane::S64)?;
            }
        }
        self.push_register_result(result)
    }

    fn push_register_result(&mut self, result: ValueLane) -> Result<()> {
        match result {
            ValueLane::S32 => self.push_accumulator32(result),
            ValueLane::S64 => self.push_accumulator64(result),
            ValueLane::S128 => unreachable!(),
        }
    }

    fn numeric64_source_binary_instruction(
        &mut self,
        op: RegisterOp64,
        lhs: ValueLocation,
        rhs: ValueLocation,
    ) -> Result<Option<Instruction>> {
        Ok(Some(match (lhs, rhs) {
            (ValueLocation::LocalNumeric64(lhs), ValueLocation::LocalNumeric64(rhs)) => match op {
                RegisterOp64::Bin(op) => Instruction::AccBinOpLocalLocal64(op, lhs, rhs),
                RegisterOp64::I64Cmp(op) => Instruction::AccI64CmpLocalLocal(op, lhs, rhs),
                RegisterOp64::F64Cmp(op) => Instruction::AccF64CmpLocalLocal(op, lhs, rhs),
            },
            (ValueLocation::LocalNumeric64(local), ValueLocation::ConstNumeric64(value)) => {
                self.numeric64_local_const_instruction(op, local, value)?
            }
            (ValueLocation::ConstNumeric64(value), ValueLocation::LocalNumeric64(local)) if op.is_commutative() => {
                self.numeric64_local_const_instruction(op, local, value)?
            }
            _ => return Ok(None),
        }))
    }

    fn numeric64_local_const_instruction(&mut self, op: RegisterOp64, local: u16, value: i64) -> Result<Instruction> {
        let index = self.push_operand128(Operand128::<(u16, u64)>::new(local, value as u64))?;
        Ok(match op {
            RegisterOp64::Bin(op) => Instruction::AccBinOpLocalConst64(PackedOp::new(op, index)),
            RegisterOp64::I64Cmp(op) => Instruction::AccI64CmpLocalConst(PackedOp::new(op, index)),
            RegisterOp64::F64Cmp(op) => Instruction::AccF64CmpLocalConst(PackedOp::new(op, index)),
        })
    }

    fn numeric64_direct_instruction(&mut self, op: RegisterOp64, location: ValueLocation) -> Result<Instruction> {
        Ok(match (op, location) {
            (RegisterOp64::Bin(op), ValueLocation::LocalNumeric64(local)) => Instruction::AccBinOpLocal64(op, local),
            (RegisterOp64::I64Cmp(op), ValueLocation::LocalNumeric64(local)) => Instruction::AccI64CmpLocal(op, local),
            (RegisterOp64::F64Cmp(op), ValueLocation::LocalNumeric64(local)) => Instruction::AccF64CmpLocal(op, local),
            (RegisterOp64::Bin(op), ValueLocation::ConstNumeric64(value)) => {
                Instruction::AccBinOpConst64(PackedOp::new(op, self.push_operand64(Operand64::<i64>::new(value))?))
            }
            (RegisterOp64::I64Cmp(op), ValueLocation::ConstNumeric64(value)) => {
                Instruction::AccI64CmpConst(PackedOp::new(op, self.push_operand64(Operand64::<i64>::new(value))?))
            }
            (RegisterOp64::F64Cmp(op), ValueLocation::ConstNumeric64(value)) => {
                Instruction::AccF64CmpConst(PackedOp::new(op, self.push_operand64(Operand64::<i64>::new(value))?))
            }
            _ => unreachable!(),
        })
    }

    fn register_stack_binary64_instruction(op: RegisterOp64) -> Instruction {
        match op {
            RegisterOp64::Bin(op) => Instruction::AccBinOpStack64(op),
            RegisterOp64::I64Cmp(op) => Instruction::AccI64CmpStack(op),
            RegisterOp64::F64Cmp(op) => Instruction::AccF64CmpStack(op),
        }
    }

    fn register_stack_stack_binary64_instruction(op: RegisterOp64) -> Instruction {
        match op {
            RegisterOp64::Bin(op) => Instruction::AccBinOpStackStack64(op),
            RegisterOp64::I64Cmp(op) => Instruction::AccI64CmpStackStack(op),
            RegisterOp64::F64Cmp(op) => Instruction::AccF64CmpStackStack(op),
        }
    }

    fn replace_binary_with_register(&mut self) -> Result<()> {
        if self.top_is_accumulator32() {
            self.pop_accumulator32(ValueLane::S32)?;
        } else {
            self.pop_deferred_value(ValueLane::S32)?;
        }
        if self.top_is_accumulator32() {
            self.pop_accumulator32(ValueLane::S32)?;
        } else {
            self.pop_deferred_value(ValueLane::S32)?;
        }
        self.push_accumulator32(ValueLane::S32)
    }

    fn replace_binary_with_deferred(&mut self, lane: ValueLane, location: ValueLocation) -> Result<()> {
        self.pop_deferred_value(lane)?;
        self.pop_deferred_value(lane)?;
        self.push_location(location)
    }

    fn deferred_binary32(op: RegisterOp32, lhs: ValueLocation, rhs: ValueLocation) -> Option<ValueLocation> {
        let commutative = op.is_commutative();
        let RegisterOp32::Bin(op) = op else { return None };
        Some(match (lhs, rhs) {
            (ValueLocation::LocalNumeric32(left), ValueLocation::LocalNumeric32(right)) => {
                ValueLocation::DeferredBinLocalLocal32 { op, left, right }
            }
            (ValueLocation::LocalNumeric32(local), ValueLocation::ConstNumeric32(value)) => {
                ValueLocation::DeferredBinLocalConst32 { op, local, value }
            }
            (ValueLocation::ConstNumeric32(value), ValueLocation::LocalNumeric32(local)) if commutative => {
                ValueLocation::DeferredBinLocalConst32 { op, local, value }
            }
            _ => return None,
        })
    }

    fn deferred_binary64(op: RegisterOp64, lhs: ValueLocation, rhs: ValueLocation) -> Option<ValueLocation> {
        let commutative = op.is_commutative();
        let RegisterOp64::Bin(op) = op else { return None };
        Some(match (lhs, rhs) {
            (ValueLocation::LocalNumeric64(left), ValueLocation::LocalNumeric64(right)) => {
                ValueLocation::DeferredBinLocalLocal64 { op, left, right }
            }
            (ValueLocation::LocalNumeric64(local), ValueLocation::ConstNumeric64(value)) => {
                ValueLocation::DeferredBinLocalConst64 { op, local, value }
            }
            (ValueLocation::ConstNumeric64(value), ValueLocation::LocalNumeric64(local)) if commutative => {
                ValueLocation::DeferredBinLocalConst64 { op, local, value }
            }
            _ => return None,
        })
    }

    fn nested_binary32_instruction(&mut self, outer: BinOp, rhs: ValueLocation) -> Result<Instruction> {
        Ok(match rhs {
            ValueLocation::DeferredBinLocalLocal32 { op, left, right } => {
                let operand = self.push_operand64(Operand64::<(u16, u16)>::new(left, right))?;
                Instruction::AccBinOpNestedLocalLocal32(PackedOp::new((outer, op), operand))
            }
            ValueLocation::DeferredBinLocalConst32 { op, local, value } => {
                let operand = self.push_operand64(Operand64::<(u16, u32)>::new(local, value as u32))?;
                Instruction::AccBinOpNestedLocalConst32(PackedOp::new((outer, op), operand))
            }
            _ => unreachable!(),
        })
    }

    fn emit_nested_binary64(&mut self, outer: BinOp, rhs: ValueLocation) -> Result<()> {
        let instruction = match rhs {
            ValueLocation::DeferredBinLocalLocal64 { op, left, right } => {
                let operand = self.push_operand64(Operand64::<(u16, u16)>::new(left, right))?;
                Instruction::AccBinOpNestedLocalLocal64(PackedOp::new((outer, op), operand))
            }
            ValueLocation::DeferredBinLocalConst64 { op, local, value } => {
                let operand = self.push_operand128(Operand128::<(u16, u64)>::new(local, value as u64))?;
                Instruction::AccBinOpNestedLocalConst64(PackedOp::new((outer, op), operand))
            }
            _ => unreachable!(),
        };
        self.instructions.stage_acc_binary64(instruction);
        Ok(())
    }

    fn emit_selected_i32_instruction(&mut self, instruction: Instruction) {
        match instruction {
            Instruction::AccI32AddLocalConst(argument) => {
                self.instructions.stage_i32_add_local_const(argument.local, argument.value);
            }
            Instruction::AccI32CmpLocal(op, right) => {
                self.instructions.stage_i32_compare_acc_local(op, right);
            }
            Instruction::AccI32CmpLocalConst(packed) => {
                self.instructions.stage_i32_compare_local_const(packed);
            }
            Instruction::AccI32CmpLocalLocal(op, left, right) => {
                self.instructions.stage_i32_compare_local_local(op, left, right);
            }
            instruction => self.instructions.stage_acc_binary32(instruction),
        }
    }

    fn emit_selected_acc64_instruction(&mut self, instruction: Instruction) {
        self.instructions.stage_acc_binary64(instruction);
    }

    fn numeric32_source_binary_instruction(
        &mut self,
        op: RegisterOp32,
        lhs: ValueLocation,
        rhs: ValueLocation,
    ) -> Result<Option<Instruction>> {
        let (local, value) = match (lhs, rhs) {
            (ValueLocation::LocalNumeric32(local), ValueLocation::ConstNumeric32(value)) => (local, value),
            (ValueLocation::ConstNumeric32(value), ValueLocation::LocalNumeric32(local)) if op.is_commutative() => {
                (local, value)
            }
            (ValueLocation::LocalNumeric32(lhs), ValueLocation::LocalNumeric32(rhs)) => {
                return Ok(Some(match op {
                    RegisterOp32::Bin(op) => Instruction::AccBinOpLocalLocal32(op, lhs, rhs),
                    RegisterOp32::I32Cmp(op) => Instruction::AccI32CmpLocalLocal(op, lhs, rhs),
                    RegisterOp32::F32Cmp(op) => Instruction::AccF32CmpLocalLocal(op, lhs, rhs),
                }));
            }
            _ => return Ok(None),
        };
        if op == RegisterOp32::Bin(BinOp::IAdd) {
            return Ok(Some(Instruction::AccI32AddLocalConst(I32LocalArg { value, local })));
        }
        let index = self.push_operand64(Operand64::<(u16, u32)>::new(local, value as u32))?;
        Ok(Some(match op {
            RegisterOp32::Bin(op) => Instruction::AccBinOpLocalConst32(PackedOp::new(op, index)),
            RegisterOp32::I32Cmp(op) => Instruction::AccI32CmpLocalConst(PackedOp::new(op, index)),
            RegisterOp32::F32Cmp(op) => Instruction::AccF32CmpLocalConst(PackedOp::new(op, index)),
        }))
    }
}

impl ValueLocation {
    fn is_numeric32_source(self) -> bool {
        matches!(self, Self::LocalNumeric32(_) | Self::ConstNumeric32(_))
    }

    fn is_numeric64_source(self) -> bool {
        matches!(self, Self::LocalNumeric64(_) | Self::ConstNumeric64(_))
    }

    fn is_deferred_binary32(self) -> bool {
        matches!(self, Self::DeferredBinLocalLocal32 { .. } | Self::DeferredBinLocalConst32 { .. })
    }

    fn is_deferred_binary64(self) -> bool {
        matches!(self, Self::DeferredBinLocalLocal64 { .. } | Self::DeferredBinLocalConst64 { .. })
    }
}

pub(super) fn direct_instruction(op: RegisterOp32, location: ValueLocation) -> Instruction {
    match (op, location) {
        (RegisterOp32::Bin(BinOp::IAdd), ValueLocation::LocalNumeric32(local)) => Instruction::AccI32AddLocal(local),
        (RegisterOp32::Bin(BinOp::IAdd), ValueLocation::ConstNumeric32(value)) => Instruction::AccI32AddConst(value),
        (RegisterOp32::Bin(op), ValueLocation::LocalNumeric32(local)) => Instruction::AccBinOpLocal32(op, local),
        (RegisterOp32::I32Cmp(op), ValueLocation::LocalNumeric32(local)) => Instruction::AccI32CmpLocal(op, local),
        (RegisterOp32::F32Cmp(op), ValueLocation::LocalNumeric32(local)) => Instruction::AccF32CmpLocal(op, local),
        (RegisterOp32::Bin(op), ValueLocation::ConstNumeric32(value)) => Instruction::AccBinOpConst32(op, value),
        (RegisterOp32::I32Cmp(op), ValueLocation::ConstNumeric32(value)) => Instruction::AccI32CmpConst(op, value),
        (RegisterOp32::F32Cmp(op), ValueLocation::ConstNumeric32(value)) => Instruction::AccF32CmpConst(op, value),
        _ => unreachable!(),
    }
}
