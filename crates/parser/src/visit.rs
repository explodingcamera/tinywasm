use crate::{Result, conversion::convert_heaptype, macros::visit::*};
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use tinywasm_types::{
    FuncType, Global, Import, ImportKind, Instruction, MemoryArch, MemoryArg, MemoryType, TableType, ValueCounts,
    WasmFunctionData, WasmType,
};
use wasmparser::{
    FuncValidator, FuncValidatorAllocations, FunctionBody, OperatorsReader, OperatorsReaderAllocations,
    ValidatorResources, VisitOperator, VisitSimdOperator,
};

#[derive(Debug, Clone, Copy)]
enum BlockKind {
    Function,
    Block,
    Loop,
    If,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OperandSize {
    S32,
    S64,
    S128,
}

impl OperandSize {
    fn choose<T>(self, s32: T, s64: T, s128: T) -> T {
        match self {
            Self::S32 => s32,
            Self::S64 => s64,
            Self::S128 => s128,
        }
    }
}

impl From<wasmparser::ValType> for OperandSize {
    fn from(ty: wasmparser::ValType) -> Self {
        match ty {
            wasmparser::ValType::I32 | wasmparser::ValType::F32 | wasmparser::ValType::Ref(_) => Self::S32,
            wasmparser::ValType::I64 | wasmparser::ValType::F64 => Self::S64,
            wasmparser::ValType::V128 => Self::S128,
        }
    }
}

impl From<&WasmType> for OperandSize {
    fn from(ty: &WasmType) -> Self {
        match ty {
            WasmType::I32 | WasmType::F32 | WasmType::RefFunc | WasmType::RefExtern => Self::S32,
            WasmType::I64 | WasmType::F64 => Self::S64,
            WasmType::V128 => Self::S128,
        }
    }
}

impl From<MemoryArch> for OperandSize {
    fn from(arch: MemoryArch) -> Self {
        match arch {
            MemoryArch::I32 => Self::S32,
            MemoryArch::I64 => Self::S64,
        }
    }
}

struct ControlFrame {
    kind: BlockKind,
    has_else: bool,
    start_ip: usize,
    branch_jumps: Vec<usize>,
    height: usize,
    base: ValueCounts,
    params: Vec<OperandSize>,
    results: Vec<OperandSize>,
    unreachable: bool,
    entry_unreachable: bool,
    end_reachable: bool,
}

#[derive(Clone)]
pub(crate) struct Signature {
    pub params: Vec<OperandSize>,
    results: Vec<OperandSize>,
}

pub(crate) struct ModuleMetadata {
    signatures: Vec<Signature>,
    functions: Vec<u32>,
    globals: Vec<OperandSize>,
    memories: Vec<OperandSize>,
    tables: Vec<OperandSize>,
}

#[derive(Default)]
struct FunctionDataBuilder {
    v128_constants: Vec<[u8; 16]>,
    branch_table_targets: Vec<u32>,
}

pub(crate) struct FunctionBuilder<'a> {
    instructions: Vec<Instruction>,
    data: FunctionDataBuilder,
    control_stack: Vec<ControlFrame>,
    operand_stack: Vec<OperandSize>,
    lane_counts: ValueCounts,
    metadata: &'a ModuleMetadata,
    local_types: Vec<OperandSize>,
    local_addr_map: Vec<u16>,
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn new(
        metadata: &'a ModuleMetadata,
        signature: Signature,
        local_types: Vec<OperandSize>,
        local_addr_map: Vec<u16>,
        body_size: usize,
    ) -> Self {
        Self {
            local_types,
            local_addr_map,
            metadata,
            instructions: Vec::with_capacity(body_size.min(1024)),
            data: FunctionDataBuilder::default(),
            control_stack: alloc::vec![ControlFrame {
                kind: BlockKind::Function,
                has_else: false,
                start_ip: 0,
                branch_jumps: Vec::new(),
                height: 0,
                base: ValueCounts::default(),
                params: Vec::new(),
                results: signature.results,
                unreachable: false,
                entry_unreachable: false,
                end_reachable: false,
            }],
            operand_stack: Vec::new(),
            lane_counts: ValueCounts::default(),
        }
    }
}

struct ValidateThenVisit<'a, 'm> {
    validator: Option<&'a mut FuncValidator<ValidatorResources>>,
    builder: &'a mut FunctionBuilder<'m>,
    position: usize,
}

impl ModuleMetadata {
    pub(crate) fn new(
        types: &[Arc<FuncType>],
        code_type_addrs: &[u32],
        imports: &[Import],
        globals: &[Global],
        memories: &[MemoryType],
        tables: &[TableType],
    ) -> Self {
        let mut functions = Vec::with_capacity(imports.len() + code_type_addrs.len());
        let mut global_sizes = Vec::with_capacity(imports.len() + globals.len());
        let mut memory_sizes = Vec::with_capacity(imports.len() + memories.len());
        let mut table_sizes = Vec::with_capacity(imports.len() + tables.len());

        for import in imports {
            match &import.kind {
                ImportKind::Function(ty) => functions.push(*ty),
                ImportKind::Global(ty) => global_sizes.push(OperandSize::from(&ty.ty)),
                ImportKind::Memory(ty) => memory_sizes.push(OperandSize::from(ty.arch())),
                ImportKind::Table(ty) => table_sizes.push(OperandSize::from(ty.arch())),
            }
        }

        functions.extend_from_slice(code_type_addrs);
        global_sizes.extend(globals.iter().map(|global| OperandSize::from(&global.ty.ty)));
        memory_sizes.extend(memories.iter().map(|ty| OperandSize::from(ty.arch())));
        table_sizes.extend(tables.iter().map(|ty| OperandSize::from(ty.arch())));

        let signatures = types
            .iter()
            .map(|ty| Signature {
                params: ty.params().iter().map(OperandSize::from).collect(),
                results: ty.results().iter().map(OperandSize::from).collect(),
            })
            .collect();
        Self { signatures, functions, globals: global_sizes, memories: memory_sizes, tables: table_sizes }
    }

    pub(crate) fn signature(&self, idx: u32) -> Result<&Signature> {
        self.signatures
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("type index out of bounds: {idx}")))
    }

    fn function_signature(&self, idx: u32) -> Result<&Signature> {
        let ty = *self
            .functions
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("function index out of bounds: {idx}")))?;
        self.signature(ty)
    }

    fn global_size(&self, idx: u32) -> Result<OperandSize> {
        Self::indexed_size(&self.globals, "global", idx)
    }

    fn memory_size(&self, idx: u32) -> Result<OperandSize> {
        Self::indexed_size(&self.memories, "memory", idx)
    }

    fn table_size(&self, idx: u32) -> Result<OperandSize> {
        Self::indexed_size(&self.tables, "table", idx)
    }

    fn indexed_size(sizes: &[OperandSize], entity: &str, idx: u32) -> Result<OperandSize> {
        sizes
            .get(idx as usize)
            .copied()
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("{entity} index out of bounds: {idx}")))
    }
}

impl<'a> VisitOperator<'a> for ValidateThenVisit<'_, '_> {
    type Output = Result<()>;

    wasmparser::for_each_visit_operator!(validate_then_visit);

    fn simd_visitor(&mut self) -> Option<&mut dyn VisitSimdOperator<'a, Output = Self::Output>> {
        Some(self)
    }
}

impl VisitSimdOperator<'_> for ValidateThenVisit<'_, '_> {
    wasmparser::for_each_visit_simd_operator!(validate_then_visit_simd);
}

pub(crate) fn process_operators_and_validate(
    mut validator: Option<FuncValidator<ValidatorResources>>,
    body: FunctionBody<'_>,
    local_types: Vec<OperandSize>,
    local_addr_map: Vec<u16>,
    metadata: &ModuleMetadata,
    ty_idx: u32,
    allocs: OperatorsReaderAllocations,
) -> Result<(Vec<Instruction>, WasmFunctionData, Option<FuncValidatorAllocations>, OperatorsReaderAllocations)> {
    let body_size = body.as_bytes().len();
    let reader = body.get_binary_reader_for_operators()?;
    let mut reader = OperatorsReader::new_with_allocs(reader, allocs);
    let signature = metadata.signature(ty_idx)?.clone();
    let mut builder = FunctionBuilder::new(metadata, signature, local_types, local_addr_map, body_size);

    while !reader.eof() {
        let position = reader.original_position();
        let res = reader
            .visit_operator(&mut ValidateThenVisit { validator: validator.as_mut(), builder: &mut builder, position })
            .map_err(|e| crate::ParseError::ParseError { message: e.to_string(), offset: position });

        if let Err(e) = res.flatten() {
            core::hint::cold_path();
            return Err(e);
        }
    }

    reader.finish()?;

    let validator_allocations = validator.map(FuncValidator::into_allocations);
    let data = WasmFunctionData {
        v128_constants: builder.data.v128_constants.into_boxed_slice(),
        branch_table_targets: builder.data.branch_table_targets.into_boxed_slice(),
    };
    Ok((builder.instructions, data, validator_allocations, reader.into_allocations()))
}

impl<'a> wasmparser::VisitOperator<'a> for FunctionBuilder<'_> {
    type Output = Result<()>;

    fn simd_visitor(&mut self) -> Option<&mut dyn VisitSimdOperator<'a, Output = Self::Output>> {
        Some(self)
    }

    wasmparser::for_each_visit_operator!(impl_visit_operator);

    lowering_ops! {
        memory [Addr] => [S32] {
            visit_i32_load => I32Load, visit_f32_load => F32Load, visit_i32_load8_s => I32Load8S,
            visit_i32_load8_u => I32Load8U, visit_i32_load16_s => I32Load16S,
            visit_i32_load16_u => I32Load16U,
        }
        memory [Addr] => [S64] {
            visit_i64_load => I64Load, visit_f64_load => F64Load, visit_i64_load8_s => I64Load8S,
            visit_i64_load8_u => I64Load8U, visit_i64_load16_s => I64Load16S,
            visit_i64_load16_u => I64Load16U, visit_i64_load32_s => I64Load32S,
            visit_i64_load32_u => I64Load32U,
        }
        memory [Addr, S32] => [] {
            visit_f32_store => F32Store, visit_i32_store8 => I32Store8,
            visit_i32_store16 => I32Store16, visit_i32_store => I32Store,
        }
        memory [Addr, S64] => [] {
            visit_f64_store => F64Store, visit_i64_store8 => I64Store8,
            visit_i64_store16 => I64Store16, visit_i64_store32 => I64Store32, visit_i64_store => I64Store,
        }
        fixed [] => [] { visit_data_drop(segment: u32) => DataDrop, visit_elem_drop(segment: u32) => ElemDrop }
        fixed [] => [S32] { visit_i32_const(value: i32) => Const32, visit_ref_func(function: u32) => RefFunc }
        fixed [] => [S64] { visit_i64_const(value: i64) => Const64 }
        fixed [S32] => [S32] {
            visit_i32_eqz => I32Eqz, visit_ref_is_null => RefIsNull, visit_i32_clz => I32Clz,
            visit_i32_ctz => I32Ctz, visit_i32_popcnt => I32Popcnt, visit_i32_extend8_s => I32Extend8S,
            visit_i32_extend16_s => I32Extend16S, visit_i32_trunc_f32_s => I32TruncF32S,
            visit_i32_trunc_f32_u => I32TruncF32U, visit_f32_convert_i32_s => F32ConvertI32S,
            visit_f32_convert_i32_u => F32ConvertI32U, visit_i32_trunc_sat_f32_s => I32TruncSatF32S,
            visit_i32_trunc_sat_f32_u => I32TruncSatF32U, visit_f32_abs => F32Abs, visit_f32_neg => F32Neg,
            visit_f32_ceil => F32Ceil, visit_f32_floor => F32Floor, visit_f32_trunc => F32Trunc,
            visit_f32_nearest => F32Nearest, visit_f32_sqrt => F32Sqrt,
        }
        fixed [S64] => [S64] {
            visit_i64_clz => I64Clz, visit_i64_ctz => I64Ctz, visit_i64_popcnt => I64Popcnt,
            visit_i64_extend8_s => I64Extend8S, visit_i64_extend16_s => I64Extend16S,
            visit_i64_extend32_s => I64Extend32S, visit_i64_trunc_f64_s => I64TruncF64S,
            visit_i64_trunc_f64_u => I64TruncF64U, visit_f64_convert_i64_s => F64ConvertI64S,
            visit_f64_convert_i64_u => F64ConvertI64U, visit_i64_trunc_sat_f64_s => I64TruncSatF64S,
            visit_i64_trunc_sat_f64_u => I64TruncSatF64U, visit_f64_abs => F64Abs, visit_f64_neg => F64Neg,
            visit_f64_ceil => F64Ceil, visit_f64_floor => F64Floor, visit_f64_trunc => F64Trunc,
            visit_f64_nearest => F64Nearest, visit_f64_sqrt => F64Sqrt,
        }
        fixed [S64] => [S32] {
            visit_i64_eqz => I64Eqz, visit_i32_wrap_i64 => I32WrapI64, visit_i32_trunc_f64_s => I32TruncF64S,
            visit_i32_trunc_f64_u => I32TruncF64U, visit_f32_convert_i64_s => F32ConvertI64S,
            visit_f32_convert_i64_u => F32ConvertI64U, visit_f32_demote_f64 => F32DemoteF64,
            visit_i32_trunc_sat_f64_s => I32TruncSatF64S, visit_i32_trunc_sat_f64_u => I32TruncSatF64U,
        }
        fixed [S32] => [S64] {
            visit_i64_extend_i32_s => I64ExtendI32S, visit_i64_extend_i32_u => I64ExtendI32U,
            visit_i64_trunc_f32_s => I64TruncF32S, visit_i64_trunc_f32_u => I64TruncF32U,
            visit_f64_convert_i32_s => F64ConvertI32S, visit_f64_convert_i32_u => F64ConvertI32U,
            visit_f64_promote_f32 => F64PromoteF32, visit_i64_trunc_sat_f32_s => I64TruncSatF32S,
            visit_i64_trunc_sat_f32_u => I64TruncSatF32U,
        }
        fixed [S32, S32] => [S32] {
            visit_i32_eq => I32Eq, visit_i32_ne => I32Ne, visit_i32_lt_s => I32LtS, visit_i32_lt_u => I32LtU,
            visit_i32_gt_s => I32GtS, visit_i32_gt_u => I32GtU, visit_i32_le_s => I32LeS,
            visit_i32_le_u => I32LeU, visit_i32_ge_s => I32GeS, visit_i32_ge_u => I32GeU,
            visit_f32_eq => F32Eq, visit_f32_ne => F32Ne, visit_f32_lt => F32Lt, visit_f32_gt => F32Gt,
            visit_f32_le => F32Le, visit_f32_ge => F32Ge, visit_i32_add => I32Add, visit_i32_sub => I32Sub,
            visit_i32_mul => I32Mul, visit_i32_div_s => I32DivS, visit_i32_div_u => I32DivU,
            visit_i32_rem_s => I32RemS, visit_i32_rem_u => I32RemU, visit_i32_and => I32And,
            visit_i32_or => I32Or, visit_i32_xor => I32Xor, visit_i32_shl => I32Shl, visit_i32_shr_s => I32ShrS,
            visit_i32_shr_u => I32ShrU, visit_i32_rotl => I32Rotl, visit_i32_rotr => I32Rotr,
            visit_f32_add => F32Add, visit_f32_sub => F32Sub, visit_f32_mul => F32Mul, visit_f32_div => F32Div,
            visit_f32_min => F32Min, visit_f32_max => F32Max, visit_f32_copysign => F32Copysign,
        }
        fixed [S64, S64] => [S32] {
            visit_i64_eq => I64Eq, visit_i64_ne => I64Ne, visit_i64_lt_s => I64LtS, visit_i64_lt_u => I64LtU,
            visit_i64_gt_s => I64GtS, visit_i64_gt_u => I64GtU, visit_i64_le_s => I64LeS,
            visit_i64_le_u => I64LeU, visit_i64_ge_s => I64GeS, visit_i64_ge_u => I64GeU,
            visit_f64_eq => F64Eq, visit_f64_ne => F64Ne, visit_f64_lt => F64Lt, visit_f64_gt => F64Gt,
            visit_f64_le => F64Le, visit_f64_ge => F64Ge,
        }
        fixed [S64, S64] => [S64] {
            visit_i64_add => I64Add, visit_i64_sub => I64Sub, visit_i64_mul => I64Mul,
            visit_i64_div_s => I64DivS, visit_i64_div_u => I64DivU, visit_i64_rem_s => I64RemS,
            visit_i64_rem_u => I64RemU, visit_i64_and => I64And, visit_i64_or => I64Or, visit_i64_xor => I64Xor,
            visit_i64_shl => I64Shl, visit_i64_shr_s => I64ShrS, visit_i64_shr_u => I64ShrU,
            visit_i64_rotl => I64Rotl, visit_i64_rotr => I64Rotr, visit_f64_add => F64Add,
            visit_f64_sub => F64Sub, visit_f64_mul => F64Mul, visit_f64_div => F64Div,
            visit_f64_min => F64Min, visit_f64_max => F64Max, visit_f64_copysign => F64Copysign,
        }
        fixed [S64, S64, S64, S64] => [S64, S64] { visit_i64_add128 => I64Add128, visit_i64_sub128 => I64Sub128 }
        fixed [S64, S64] => [S64, S64] { visit_i64_mul_wide_s => I64MulWideS, visit_i64_mul_wide_u => I64MulWideU }
        effect [] => [] { visit_nop }
        effect [S32] => [S32] { visit_f32_reinterpret_i32, visit_i32_reinterpret_f32 }
        effect [S64] => [S64] { visit_f64_reinterpret_i64, visit_i64_reinterpret_f64 }
        terminating [] => [] { visit_unreachable => Unreachable, visit_return => Return }
        global [] => [Addr] { visit_global_get(global_index: u32) => GlobalGet }
        memory_index [] => [Addr] { visit_memory_size(memory: u32) => MemorySize }
        memory_index [Addr] => [Addr] { visit_memory_grow(memory: u32) => MemoryGrow }
        memory_index [Addr, S32, Addr] => [] {
            visit_memory_init(data_index: u32, memory: u32) => MemoryInit,
            visit_memory_fill(memory: u32) => MemoryFill,
        }
        table [Addr] => [S32] { visit_table_get(table: u32) => TableGet }
        table [Addr, S32] => [] { visit_table_set(table: u32) => TableSet }
        table [] => [Addr] { visit_table_size(table: u32) => TableSize }
        table [S32, Addr] => [Addr] { visit_table_grow(table: u32) => TableGrow }
        table [Addr, S32, Addr] => [] { visit_table_fill(table: u32) => TableFill }
        table [Addr, S32, S32] => [] { visit_table_init(elem_index: u32, table: u32) => TableInit }
    }

    fn visit_call(&mut self, function_index: u32) -> Self::Output {
        let signature = self.metadata.function_signature(function_index)?.clone();
        self.emit(&signature.params, &signature.results, Instruction::Call(function_index))
    }

    fn visit_call_indirect(&mut self, type_index: u32, table_index: u32) -> Self::Output {
        let signature = self.metadata.signature(type_index)?.clone();
        let mut inputs = signature.params;
        inputs.push(self.metadata.table_size(table_index)?);
        self.emit(&inputs, &signature.results, Instruction::CallIndirect(type_index, table_index))
    }

    fn visit_return_call(&mut self, function_index: u32) -> Self::Output {
        let signature = self.metadata.function_signature(function_index)?.clone();
        self.apply_effect(&signature.params, &[])?;
        self.mark_unreachable();
        self.instructions.push(Instruction::ReturnCall(function_index));
        Ok(())
    }

    fn visit_return_call_indirect(&mut self, type_index: u32, table_index: u32) -> Self::Output {
        let signature = self.metadata.signature(type_index)?.clone();
        let mut inputs = signature.params;
        inputs.push(self.metadata.table_size(table_index)?);
        self.apply_effect(&inputs, &[])?;
        self.mark_unreachable();
        self.instructions.push(Instruction::ReturnCallIndirect(type_index, table_index));
        Ok(())
    }

    fn visit_global_set(&mut self, global_index: u32) -> Self::Output {
        let size = self.metadata.global_size(global_index)?;
        let instruction = size.choose(
            Instruction::GlobalSet32(global_index),
            Instruction::GlobalSet64(global_index),
            Instruction::GlobalSet128(global_index),
        );
        self.emit(&[size], &[], instruction)
    }

    fn visit_drop(&mut self) -> Self::Output {
        let size = self.operand_stack.last().copied().unwrap_or(OperandSize::S32);
        let instruction = size.choose(Instruction::Drop32, Instruction::Drop64, Instruction::Drop128);
        self.emit(&[size], &[], instruction)
    }

    fn visit_select(&mut self) -> Self::Output {
        let size = self.operand_stack.iter().rev().nth(1).copied().unwrap_or(OperandSize::S32);
        let instruction = size.choose(Instruction::Select32, Instruction::Select64, Instruction::Select128);
        self.emit(&[size, size, OperandSize::S32], &[size], instruction)
    }

    fn visit_local_get(&mut self, idx: u32) -> Self::Output {
        let (size, local_idx) = self.local(idx)?;
        let instruction = size.choose(
            Instruction::LocalGet32(local_idx),
            Instruction::LocalGet64(local_idx),
            Instruction::LocalGet128(local_idx),
        );
        self.emit(&[], &[size], instruction)
    }

    fn visit_local_set(&mut self, idx: u32) -> Self::Output {
        let (size, local_idx) = self.local(idx)?;
        let instruction = size.choose(
            Instruction::LocalSet32(local_idx),
            Instruction::LocalSet64(local_idx),
            Instruction::LocalSet128(local_idx),
        );
        self.emit(&[size], &[], instruction)
    }

    fn visit_local_tee(&mut self, idx: u32) -> Self::Output {
        let (size, local_idx) = self.local(idx)?;
        self.apply_effect(&[size], &[size])?;
        let src = match (size, self.instructions.last()) {
            (OperandSize::S32, Some(Instruction::LocalGet32(src))) => Some(*src),
            (OperandSize::S64, Some(Instruction::LocalGet64(src))) => Some(*src),
            (OperandSize::S128, Some(Instruction::LocalGet128(src))) => Some(*src),
            _ => None,
        };
        if let Some(src) = src {
            self.instructions.pop();
            let instructions = match size {
                OperandSize::S32 => [Instruction::LocalCopy32(src, local_idx), Instruction::LocalGet32(local_idx)],
                OperandSize::S64 => [Instruction::LocalCopy64(src, local_idx), Instruction::LocalGet64(local_idx)],
                OperandSize::S128 => [Instruction::LocalCopy128(src, local_idx), Instruction::LocalGet128(local_idx)],
            };
            self.instructions.extend(instructions);
        } else {
            self.instructions.push(size.choose(
                Instruction::LocalTee32(local_idx),
                Instruction::LocalTee64(local_idx),
                Instruction::LocalTee128(local_idx),
            ));
        }
        Ok(())
    }

    fn visit_block(&mut self, blockty: wasmparser::BlockType) -> Self::Output {
        self.push_control(BlockKind::Block, blockty, None)
    }

    fn visit_loop(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.push_control(BlockKind::Loop, ty, None)
    }

    fn visit_if(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.pop_expect(OperandSize::S32)?;
        self.instructions.push(Instruction::JumpIfZero32(0));
        self.push_control(BlockKind::If, ty, Some(self.instructions.len() - 1))
    }

    fn visit_else(&mut self) -> Self::Output {
        let (cond_jump_ip, height, base, params, entry_unreachable) = {
            let ctx = self
                .control_stack
                .last_mut()
                .filter(|ctx| matches!(ctx.kind, BlockKind::If))
                .ok_or_else(|| crate::ParseError::Other("else without matching if".into()))?;
            ctx.end_reachable |= !ctx.unreachable;
            ctx.has_else = true;
            (ctx.branch_jumps[0], ctx.height, ctx.base, ctx.params.clone(), ctx.entry_unreachable)
        };
        let jump_ip = self.instructions.len();
        self.instructions.push(Instruction::Jump(0));
        self.control_stack.last_mut().unwrap().branch_jumps.push(jump_ip);
        self.patch_jump(cond_jump_ip, self.instructions.len());
        self.reset_stack(height, base);
        self.push_sizes(&params)?;
        self.control_stack.last_mut().unwrap().unreachable = entry_unreachable;
        Ok(())
    }

    fn visit_end(&mut self) -> Self::Output {
        let ctx =
            self.control_stack.pop().ok_or_else(|| crate::ParseError::Other("end without control frame".into()))?;
        if matches!(ctx.kind, BlockKind::Function) {
            self.instructions.push(Instruction::Return);
        } else {
            let reachable = !ctx.entry_unreachable
                && (!ctx.unreachable || ctx.end_reachable || matches!(ctx.kind, BlockKind::If) && !ctx.has_else);
            self.reset_stack(ctx.height, ctx.base);
            self.push_sizes(&ctx.results)?;
            if let Some(parent) = self.control_stack.last_mut() {
                parent.unreachable = !reachable;
            }
            self.patch_end_jumps(ctx, self.instructions.len());
        }
        Ok(())
    }

    fn visit_br(&mut self, depth: u32) -> Self::Output {
        self.emit_dropkeep_to_label(depth)?;
        self.emit_branch_jump_or_return(depth)?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_br_if(&mut self, depth: u32) -> Self::Output {
        self.pop_expect(OperandSize::S32)?;
        let cond_jump_ip = self.instructions.len();
        self.instructions.push(Instruction::JumpIfZero32(0));

        let branch_side_start = self.instructions.len();
        self.emit_dropkeep_to_label(depth)?;

        if self.instructions.len() == branch_side_start
            && let Ok(ctx_idx) = self.get_ctx_idx(depth)
            && !matches!(self.control_stack[ctx_idx].kind, BlockKind::Function)
        {
            self.instructions[cond_jump_ip] = Instruction::JumpIfNonZero32(0);
            self.control_stack[ctx_idx].branch_jumps.push(cond_jump_ip);
            self.control_stack[ctx_idx].end_reachable = true;
            return Ok(());
        }

        self.emit_branch_jump_or_return(depth)?;
        self.patch_jump(cond_jump_ip, self.instructions.len());
        Ok(())
    }

    fn visit_br_table(&mut self, targets: wasmparser::BrTable<'_>) -> Self::Output {
        let ts = targets.targets().collect::<Result<Vec<_>, wasmparser::Error>>()?;
        self.pop_expect(OperandSize::S32)?;

        let default_depth = targets.default();
        let len = ts.len() as u32;
        let target_depths: Vec<u32> = ts;

        let header_ip = self.instructions.len();
        let branch_table_start = self.data.branch_table_targets.len() as u32;
        self.instructions.push(Instruction::BranchTable(0, branch_table_start, len));

        struct PadInfo {
            depth: u32,
            pad_start: usize,
            jump_or_ret_ip: usize,
            is_return: bool,
        }
        let mut pads: Vec<PadInfo> = Vec::new();

        for &depth in target_depths.iter().chain(core::iter::once(&default_depth)) {
            if pads.iter().any(|pad| pad.depth == depth) {
                continue;
            }

            let pad_start = self.instructions.len();
            let (jump_or_ret_ip, is_return) = if self.is_unreachable() {
                self.instructions.push(Instruction::Return);
                (pad_start, true)
            } else {
                let frame = &self.control_stack[self.get_ctx_idx(depth)?];
                let base = frame.base;
                let label_types = if matches!(frame.kind, BlockKind::Loop) { &frame.params } else { &frame.results };
                self.emit_dropkeep(base, Self::value_counts(label_types));
                let jump_ip = self.instructions.len();
                self.instructions.push(Instruction::Jump(0));
                (jump_ip, false)
            };
            pads.push(PadInfo { depth, pad_start, jump_or_ret_ip, is_return });
        }

        for &depth in &target_depths {
            let pad = pads
                .iter()
                .find(|pad| pad.depth == depth)
                .ok_or_else(|| crate::ParseError::Other("missing branch table target".into()))?;
            self.data.branch_table_targets.push(pad.pad_start as u32);
        }

        let default_pad = pads
            .iter()
            .find(|pad| pad.depth == default_depth)
            .ok_or_else(|| crate::ParseError::Other("missing default branch table target".into()))?;
        if let Instruction::BranchTable(default_ip, _, _) = &mut self.instructions[header_ip] {
            *default_ip = default_pad.pad_start as u32;
        }

        for pad in &pads {
            if pad.is_return {
                continue;
            }
            let ctx_idx = self.get_ctx_idx(pad.depth)?;
            if matches!(self.control_stack[ctx_idx].kind, BlockKind::Function) {
                self.instructions[pad.jump_or_ret_ip] = Instruction::Return;
            } else if matches!(self.control_stack[ctx_idx].kind, BlockKind::Loop) {
                self.patch_jump(pad.jump_or_ret_ip, self.control_stack[ctx_idx].start_ip);
            } else {
                self.control_stack[ctx_idx].branch_jumps.push(pad.jump_or_ret_ip);
                self.control_stack[ctx_idx].end_reachable = true;
            }
        }
        self.mark_unreachable();
        Ok(())
    }

    fn visit_f32_const(&mut self, val: wasmparser::Ieee32) -> Self::Output {
        self.emit(&[], &[OperandSize::S32], Instruction::Const32(val.bits() as i32))
    }

    fn visit_f64_const(&mut self, val: wasmparser::Ieee64) -> Self::Output {
        self.emit(&[], &[OperandSize::S64], Instruction::Const64(val.bits() as i64))
    }

    fn visit_table_copy(&mut self, dst_table: u32, src_table: u32) -> Self::Output {
        let dst = self.metadata.table_size(dst_table)?;
        let src = self.metadata.table_size(src_table)?;
        let len = if dst == OperandSize::S32 || src == OperandSize::S32 { OperandSize::S32 } else { OperandSize::S64 };
        self.emit(&[dst, src, len], &[], Instruction::TableCopy { dst_table, src_table })
    }

    fn visit_memory_copy(&mut self, dst_mem: u32, src_mem: u32) -> Self::Output {
        let dst = self.metadata.memory_size(dst_mem)?;
        let src = self.metadata.memory_size(src_mem)?;
        let len = if dst == OperandSize::S32 || src == OperandSize::S32 { OperandSize::S32 } else { OperandSize::S64 };
        self.emit(&[dst, src, len], &[], Instruction::MemoryCopy { dst_mem, src_mem })
    }

    // Reference Types
    fn visit_ref_null(&mut self, ty: wasmparser::HeapType) -> Self::Output {
        let instruction = Instruction::RefNull(convert_heaptype(ty)?);
        self.emit(&[], &[OperandSize::S32], instruction)
    }

    fn visit_typed_select_multi(&mut self, tys: Vec<wasmparser::ValType>) -> Self::Output {
        let sizes: Vec<_> = tys.into_iter().map(OperandSize::from).collect();
        let counts = Self::value_counts(&sizes);
        self.emit(
            &[sizes.as_slice(), sizes.as_slice(), &[OperandSize::S32]].concat(),
            &sizes,
            Instruction::SelectMulti(counts),
        )
    }

    fn visit_typed_select(&mut self, ty: wasmparser::ValType) -> Self::Output {
        let size = OperandSize::from(ty);
        let instruction = size.choose(Instruction::Select32, Instruction::Select64, Instruction::Select128);
        self.emit(&[size, size, OperandSize::S32], &[size], instruction)
    }
}

macro_rules! impl_visit_simd_operator {
    ($(@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {
        $(impl_visit_operator!(@@$proposal $op $({ $($arg: $argty),* })? => $visit ($($ann:tt)*));)*
    };

    (@@simd $($rest:tt)* ) => {};
    (@@relaxed_simd $($rest:tt)* ) => {};
    (@@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*)) => {
        fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
            Err(crate::ParseError::UnsupportedOperator(stringify!($visit).to_string()))
        }
    };
}

impl wasmparser::VisitSimdOperator<'_> for FunctionBuilder<'_> {
    wasmparser::for_each_visit_simd_operator!(impl_visit_simd_operator);

    lowering_ops! {
        memory [Addr] => [S128] {
            visit_v128_load => V128Load, visit_v128_load8x8_s => V128Load8x8S,
            visit_v128_load8x8_u => V128Load8x8U, visit_v128_load16x4_s => V128Load16x4S,
            visit_v128_load16x4_u => V128Load16x4U, visit_v128_load32x2_s => V128Load32x2S,
            visit_v128_load32x2_u => V128Load32x2U, visit_v128_load8_splat => V128Load8Splat,
            visit_v128_load16_splat => V128Load16Splat, visit_v128_load32_splat => V128Load32Splat,
            visit_v128_load64_splat => V128Load64Splat, visit_v128_load32_zero => V128Load32Zero,
            visit_v128_load64_zero => V128Load64Zero,
        }
        memory [Addr, S128] => [] { visit_v128_store => V128Store }
        memory [Addr, S128] => [S128] {
            visit_v128_load8_lane(lane: u8) => V128Load8Lane,
            visit_v128_load16_lane(lane: u8) => V128Load16Lane,
            visit_v128_load32_lane(lane: u8) => V128Load32Lane,
            visit_v128_load64_lane(lane: u8) => V128Load64Lane,
        }
        memory [Addr, S128] => [] {
            visit_v128_store8_lane(lane: u8) => V128Store8Lane,
            visit_v128_store16_lane(lane: u8) => V128Store16Lane,
            visit_v128_store32_lane(lane: u8) => V128Store32Lane,
            visit_v128_store64_lane(lane: u8) => V128Store64Lane,
        }
        fixed [S32] => [S128] {
            visit_i8x16_splat => I8x16Splat, visit_i16x8_splat => I16x8Splat,
            visit_i32x4_splat => I32x4Splat, visit_f32x4_splat => F32x4Splat,
        }
        fixed [S64] => [S128] { visit_i64x2_splat => I64x2Splat, visit_f64x2_splat => F64x2Splat }
        fixed [S128] => [S32] {
            visit_v128_any_true => V128AnyTrue, visit_i8x16_all_true => I8x16AllTrue,
            visit_i8x16_bitmask => I8x16Bitmask, visit_i16x8_all_true => I16x8AllTrue,
            visit_i16x8_bitmask => I16x8Bitmask, visit_i32x4_all_true => I32x4AllTrue,
            visit_i32x4_bitmask => I32x4Bitmask, visit_i64x2_all_true => I64x2AllTrue,
            visit_i64x2_bitmask => I64x2Bitmask, visit_i8x16_extract_lane_s(lane: u8) => I8x16ExtractLaneS,
            visit_i8x16_extract_lane_u(lane: u8) => I8x16ExtractLaneU,
            visit_i16x8_extract_lane_s(lane: u8) => I16x8ExtractLaneS,
            visit_i16x8_extract_lane_u(lane: u8) => I16x8ExtractLaneU,
            visit_i32x4_extract_lane(lane: u8) => I32x4ExtractLane,
            visit_f32x4_extract_lane(lane: u8) => F32x4ExtractLane,
        }
        fixed [S128] => [S64] {
            visit_i64x2_extract_lane(lane: u8) => I64x2ExtractLane,
            visit_f64x2_extract_lane(lane: u8) => F64x2ExtractLane,
        }
        fixed [S128, S32] => [S128] {
            visit_i8x16_shl => I8x16Shl, visit_i8x16_shr_s => I8x16ShrS,
            visit_i8x16_shr_u => I8x16ShrU, visit_i16x8_shl => I16x8Shl, visit_i16x8_shr_s => I16x8ShrS,
            visit_i16x8_shr_u => I16x8ShrU, visit_i32x4_shl => I32x4Shl, visit_i32x4_shr_s => I32x4ShrS,
            visit_i32x4_shr_u => I32x4ShrU, visit_i64x2_shl => I64x2Shl, visit_i64x2_shr_s => I64x2ShrS,
            visit_i64x2_shr_u => I64x2ShrU, visit_i8x16_replace_lane(lane: u8) => I8x16ReplaceLane,
            visit_i16x8_replace_lane(lane: u8) => I16x8ReplaceLane,
            visit_i32x4_replace_lane(lane: u8) => I32x4ReplaceLane,
            visit_f32x4_replace_lane(lane: u8) => F32x4ReplaceLane,
        }
        fixed [S128, S64] => [S128] {
            visit_i64x2_replace_lane(lane: u8) => I64x2ReplaceLane,
            visit_f64x2_replace_lane(lane: u8) => F64x2ReplaceLane,
        }
        fixed [S128] => [S128] {
            visit_v128_not => V128Not, visit_i8x16_abs => I8x16Abs, visit_i8x16_neg => I8x16Neg,
            visit_i16x8_abs => I16x8Abs, visit_i16x8_neg => I16x8Neg, visit_i32x4_abs => I32x4Abs,
            visit_i32x4_neg => I32x4Neg, visit_i64x2_abs => I64x2Abs, visit_i64x2_neg => I64x2Neg,
            visit_i16x8_extadd_pairwise_i8x16_s => I16x8ExtAddPairwiseI8x16S,
            visit_i16x8_extadd_pairwise_i8x16_u => I16x8ExtAddPairwiseI8x16U,
            visit_i32x4_extadd_pairwise_i16x8_s => I32x4ExtAddPairwiseI16x8S,
            visit_i32x4_extadd_pairwise_i16x8_u => I32x4ExtAddPairwiseI16x8U,
            visit_i16x8_extend_low_i8x16_s => I16x8ExtendLowI8x16S,
            visit_i16x8_extend_low_i8x16_u => I16x8ExtendLowI8x16U,
            visit_i16x8_extend_high_i8x16_s => I16x8ExtendHighI8x16S,
            visit_i16x8_extend_high_i8x16_u => I16x8ExtendHighI8x16U,
            visit_i32x4_extend_low_i16x8_s => I32x4ExtendLowI16x8S,
            visit_i32x4_extend_low_i16x8_u => I32x4ExtendLowI16x8U,
            visit_i32x4_extend_high_i16x8_s => I32x4ExtendHighI16x8S,
            visit_i32x4_extend_high_i16x8_u => I32x4ExtendHighI16x8U,
            visit_i64x2_extend_low_i32x4_s => I64x2ExtendLowI32x4S,
            visit_i64x2_extend_low_i32x4_u => I64x2ExtendLowI32x4U,
            visit_i64x2_extend_high_i32x4_s => I64x2ExtendHighI32x4S,
            visit_i64x2_extend_high_i32x4_u => I64x2ExtendHighI32x4U, visit_i8x16_popcnt => I8x16Popcnt,
            visit_f32x4_ceil => F32x4Ceil, visit_f32x4_floor => F32x4Floor, visit_f32x4_trunc => F32x4Trunc,
            visit_f32x4_nearest => F32x4Nearest, visit_f32x4_abs => F32x4Abs, visit_f32x4_neg => F32x4Neg,
            visit_f32x4_sqrt => F32x4Sqrt, visit_f64x2_ceil => F64x2Ceil, visit_f64x2_floor => F64x2Floor,
            visit_f64x2_trunc => F64x2Trunc, visit_f64x2_nearest => F64x2Nearest, visit_f64x2_abs => F64x2Abs,
            visit_f64x2_neg => F64x2Neg, visit_f64x2_sqrt => F64x2Sqrt,
            visit_i32x4_trunc_sat_f32x4_s => I32x4TruncSatF32x4S,
            visit_i32x4_trunc_sat_f32x4_u => I32x4TruncSatF32x4U,
            visit_f32x4_convert_i32x4_s => F32x4ConvertI32x4S,
            visit_f32x4_convert_i32x4_u => F32x4ConvertI32x4U,
            visit_i32x4_trunc_sat_f64x2_s_zero => I32x4TruncSatF64x2SZero,
            visit_i32x4_trunc_sat_f64x2_u_zero => I32x4TruncSatF64x2UZero,
            visit_f64x2_convert_low_i32x4_s => F64x2ConvertLowI32x4S,
            visit_f64x2_convert_low_i32x4_u => F64x2ConvertLowI32x4U,
            visit_f32x4_demote_f64x2_zero => F32x4DemoteF64x2Zero,
            visit_f64x2_promote_low_f32x4 => F64x2PromoteLowF32x4,
            visit_i32x4_relaxed_trunc_f32x4_s => I32x4RelaxedTruncF32x4S,
            visit_i32x4_relaxed_trunc_f32x4_u => I32x4RelaxedTruncF32x4U,
            visit_i32x4_relaxed_trunc_f64x2_s_zero => I32x4RelaxedTruncF64x2SZero,
            visit_i32x4_relaxed_trunc_f64x2_u_zero => I32x4RelaxedTruncF64x2UZero,
        }
        fixed [S128, S128] => [S128] {
            visit_v128_and => V128And, visit_v128_andnot => V128AndNot, visit_v128_or => V128Or,
            visit_v128_xor => V128Xor, visit_i8x16_swizzle => I8x16Swizzle, visit_i8x16_eq => I8x16Eq,
            visit_i8x16_ne => I8x16Ne, visit_i8x16_lt_s => I8x16LtS, visit_i8x16_lt_u => I8x16LtU,
            visit_i8x16_gt_s => I8x16GtS, visit_i8x16_gt_u => I8x16GtU, visit_i8x16_le_s => I8x16LeS,
            visit_i8x16_le_u => I8x16LeU, visit_i8x16_ge_s => I8x16GeS, visit_i8x16_ge_u => I8x16GeU,
            visit_i16x8_eq => I16x8Eq, visit_i16x8_ne => I16x8Ne, visit_i16x8_lt_s => I16x8LtS,
            visit_i16x8_lt_u => I16x8LtU, visit_i16x8_gt_s => I16x8GtS, visit_i16x8_gt_u => I16x8GtU,
            visit_i16x8_le_s => I16x8LeS, visit_i16x8_le_u => I16x8LeU, visit_i16x8_ge_s => I16x8GeS,
            visit_i16x8_ge_u => I16x8GeU, visit_i32x4_eq => I32x4Eq, visit_i32x4_ne => I32x4Ne,
            visit_i32x4_lt_s => I32x4LtS, visit_i32x4_lt_u => I32x4LtU, visit_i32x4_gt_s => I32x4GtS,
            visit_i32x4_gt_u => I32x4GtU, visit_i32x4_le_s => I32x4LeS, visit_i32x4_le_u => I32x4LeU,
            visit_i32x4_ge_s => I32x4GeS, visit_i32x4_ge_u => I32x4GeU, visit_i64x2_eq => I64x2Eq,
            visit_i64x2_ne => I64x2Ne, visit_i64x2_lt_s => I64x2LtS, visit_i64x2_gt_s => I64x2GtS,
            visit_i64x2_le_s => I64x2LeS, visit_i64x2_ge_s => I64x2GeS, visit_f32x4_eq => F32x4Eq,
            visit_f32x4_ne => F32x4Ne, visit_f32x4_lt => F32x4Lt, visit_f32x4_gt => F32x4Gt,
            visit_f32x4_le => F32x4Le, visit_f32x4_ge => F32x4Ge, visit_f64x2_eq => F64x2Eq,
            visit_f64x2_ne => F64x2Ne, visit_f64x2_lt => F64x2Lt, visit_f64x2_gt => F64x2Gt,
            visit_f64x2_le => F64x2Le, visit_f64x2_ge => F64x2Ge, visit_i8x16_add => I8x16Add,
            visit_i8x16_sub => I8x16Sub, visit_i8x16_min_s => I8x16MinS, visit_i8x16_min_u => I8x16MinU,
            visit_i8x16_max_s => I8x16MaxS, visit_i8x16_max_u => I8x16MaxU,
            visit_i8x16_narrow_i16x8_s => I8x16NarrowI16x8S,
            visit_i8x16_narrow_i16x8_u => I8x16NarrowI16x8U, visit_i8x16_add_sat_s => I8x16AddSatS,
            visit_i8x16_add_sat_u => I8x16AddSatU, visit_i8x16_sub_sat_s => I8x16SubSatS,
            visit_i8x16_sub_sat_u => I8x16SubSatU, visit_i8x16_avgr_u => I8x16AvgrU,
            visit_i16x8_add => I16x8Add, visit_i16x8_sub => I16x8Sub, visit_i16x8_min_s => I16x8MinS,
            visit_i16x8_min_u => I16x8MinU, visit_i16x8_max_s => I16x8MaxS, visit_i16x8_max_u => I16x8MaxU,
            visit_i16x8_narrow_i32x4_s => I16x8NarrowI32x4S,
            visit_i16x8_narrow_i32x4_u => I16x8NarrowI32x4U, visit_i16x8_add_sat_s => I16x8AddSatS,
            visit_i16x8_add_sat_u => I16x8AddSatU, visit_i16x8_sub_sat_s => I16x8SubSatS,
            visit_i16x8_sub_sat_u => I16x8SubSatU, visit_i16x8_avgr_u => I16x8AvgrU,
            visit_i16x8_mul => I16x8Mul, visit_i32x4_add => I32x4Add, visit_i32x4_sub => I32x4Sub,
            visit_i32x4_min_s => I32x4MinS, visit_i32x4_min_u => I32x4MinU, visit_i32x4_max_s => I32x4MaxS,
            visit_i32x4_max_u => I32x4MaxU, visit_i32x4_mul => I32x4Mul, visit_i64x2_add => I64x2Add,
            visit_i64x2_sub => I64x2Sub, visit_i64x2_mul => I64x2Mul,
            visit_i16x8_extmul_low_i8x16_s => I16x8ExtMulLowI8x16S,
            visit_i16x8_extmul_low_i8x16_u => I16x8ExtMulLowI8x16U,
            visit_i16x8_extmul_high_i8x16_s => I16x8ExtMulHighI8x16S,
            visit_i16x8_extmul_high_i8x16_u => I16x8ExtMulHighI8x16U,
            visit_i32x4_extmul_low_i16x8_s => I32x4ExtMulLowI16x8S,
            visit_i32x4_extmul_low_i16x8_u => I32x4ExtMulLowI16x8U,
            visit_i32x4_extmul_high_i16x8_s => I32x4ExtMulHighI16x8S,
            visit_i32x4_extmul_high_i16x8_u => I32x4ExtMulHighI16x8U,
            visit_i64x2_extmul_low_i32x4_s => I64x2ExtMulLowI32x4S,
            visit_i64x2_extmul_low_i32x4_u => I64x2ExtMulLowI32x4U,
            visit_i64x2_extmul_high_i32x4_s => I64x2ExtMulHighI32x4S,
            visit_i64x2_extmul_high_i32x4_u => I64x2ExtMulHighI32x4U,
            visit_i16x8_q15mulr_sat_s => I16x8Q15MulrSatS, visit_i32x4_dot_i16x8_s => I32x4DotI16x8S,
            visit_f32x4_add => F32x4Add, visit_f32x4_sub => F32x4Sub, visit_f32x4_mul => F32x4Mul,
            visit_f32x4_div => F32x4Div, visit_f32x4_min => F32x4Min, visit_f32x4_max => F32x4Max,
            visit_f32x4_pmin => F32x4PMin, visit_f32x4_pmax => F32x4PMax, visit_f64x2_add => F64x2Add,
            visit_f64x2_sub => F64x2Sub, visit_f64x2_mul => F64x2Mul, visit_f64x2_div => F64x2Div,
            visit_f64x2_min => F64x2Min, visit_f64x2_max => F64x2Max, visit_f64x2_pmin => F64x2PMin,
            visit_f64x2_pmax => F64x2PMax, visit_i8x16_relaxed_swizzle => I8x16RelaxedSwizzle,
            visit_f32x4_relaxed_min => F32x4RelaxedMin, visit_f32x4_relaxed_max => F32x4RelaxedMax,
            visit_f64x2_relaxed_min => F64x2RelaxedMin, visit_f64x2_relaxed_max => F64x2RelaxedMax,
            visit_i16x8_relaxed_q15mulr_s => I16x8RelaxedQ15mulrS,
            visit_i16x8_relaxed_dot_i8x16_i7x16_s => I16x8RelaxedDotI8x16I7x16S,
        }
        fixed [S128, S128, S128] => [S128] {
            visit_v128_bitselect => V128Bitselect,
            visit_f32x4_relaxed_madd => F32x4RelaxedMadd, visit_f32x4_relaxed_nmadd => F32x4RelaxedNmadd,
            visit_f64x2_relaxed_madd => F64x2RelaxedMadd, visit_f64x2_relaxed_nmadd => F64x2RelaxedNmadd,
            visit_i8x16_relaxed_laneselect => I8x16RelaxedLaneselect,
            visit_i16x8_relaxed_laneselect => I16x8RelaxedLaneselect,
            visit_i32x4_relaxed_laneselect => I32x4RelaxedLaneselect,
            visit_i64x2_relaxed_laneselect => I64x2RelaxedLaneselect,
            visit_i32x4_relaxed_dot_i8x16_i7x16_add_s => I32x4RelaxedDotI8x16I7x16AddS,
        }
    }

    fn visit_i8x16_shuffle(&mut self, lanes: [u8; 16]) -> Self::Output {
        self.emit(
            &[OperandSize::S128, OperandSize::S128],
            &[OperandSize::S128],
            Instruction::I8x16Shuffle(self.data.v128_constants.len() as u32),
        )?;
        self.data.v128_constants.push(lanes);
        Ok(())
    }

    fn visit_v128_const(&mut self, value: wasmparser::V128) -> Self::Output {
        self.emit(&[], &[OperandSize::S128], Instruction::Const128(self.data.v128_constants.len() as u32))?;
        self.data.v128_constants.push(*value.bytes());
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    fn is_unreachable(&self) -> bool {
        self.control_stack.last().is_none_or(|frame| frame.unreachable)
    }

    fn get_ctx_idx(&self, depth: u32) -> Result<usize> {
        self.control_stack
            .len()
            .checked_sub(depth as usize + 1)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("branch depth out of bounds: {depth}")))
    }

    fn local(&self, idx: u32) -> Result<(OperandSize, u16)> {
        let size = *self
            .local_types
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("local index out of bounds: {idx}")))?;
        let addr = *self
            .local_addr_map
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("local address missing: {idx}")))?;
        Ok((size, addr))
    }

    /// Pushes logical operands while maintaining the lane counts used by `DropKeep`.
    fn push_sizes(&mut self, sizes: &[OperandSize]) -> Result<()> {
        for &size in sizes {
            let count = match size {
                OperandSize::S32 => &mut self.lane_counts.c32,
                OperandSize::S64 => &mut self.lane_counts.c64,
                OperandSize::S128 => &mut self.lane_counts.c128,
            };
            *count = count
                .checked_add(1)
                .ok_or_else(|| crate::ParseError::Other("logical operand lane count is too large".into()))?;
            self.operand_stack.push(size);
        }
        Ok(())
    }

    /// Pops an operand, allowing a polymorphic value at an unreachable frame base.
    fn pop_expect(&mut self, expected: OperandSize) -> Result<()> {
        let frame_height = self.control_stack.last().map_or(0, |frame| frame.height);
        if self.operand_stack.len() == frame_height && self.is_unreachable() {
            return Ok(());
        }
        let actual = self
            .operand_stack
            .pop()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        if actual != expected {
            return Err(crate::ParseError::Other("logical operand width mismatch".into()));
        }
        match actual {
            OperandSize::S32 => self.lane_counts.c32 -= 1,
            OperandSize::S64 => self.lane_counts.c64 -= 1,
            OperandSize::S128 => self.lane_counts.c128 -= 1,
        }
        Ok(())
    }

    /// Applies a declared logical stack effect in WebAssembly operand order.
    fn apply_effect(&mut self, inputs: &[OperandSize], outputs: &[OperandSize]) -> Result<()> {
        inputs.iter().rev().try_for_each(|&size| self.pop_expect(size))?;
        self.push_sizes(outputs)?;
        Ok(())
    }

    /// Applies an instruction's stack effect before adding it to the bytecode.
    fn emit(&mut self, inputs: &[OperandSize], outputs: &[OperandSize], instruction: Instruction) -> Result<()> {
        self.apply_effect(inputs, outputs)?;
        self.instructions.push(instruction);
        Ok(())
    }

    /// Restores both logical operand order and lane counts to a control-frame base.
    fn reset_stack(&mut self, height: usize, base: ValueCounts) {
        self.operand_stack.truncate(height);
        self.lane_counts = base;
    }

    /// Marks the current path unreachable and restores its entry stack.
    fn mark_unreachable(&mut self) {
        if let Some(frame) = self.control_stack.last_mut() {
            frame.unreachable = true;
            let height = frame.height;
            let base = frame.base;
            self.reset_stack(height, base);
        }
    }

    /// Enters a control frame with its parameters restored above the saved base.
    fn push_control(&mut self, kind: BlockKind, ty: wasmparser::BlockType, initial_jump: Option<usize>) -> Result<()> {
        let signature = match ty {
            wasmparser::BlockType::Empty => Signature { params: Vec::new(), results: Vec::new() },
            wasmparser::BlockType::Type(ty) => {
                Signature { params: Vec::new(), results: alloc::vec![OperandSize::from(ty)] }
            }
            wasmparser::BlockType::FuncType(idx) => self.metadata.signature(idx)?.clone(),
        };
        for &size in signature.params.iter().rev() {
            self.pop_expect(size)?;
        }
        let height = self.operand_stack.len();
        let base = self.lane_counts;
        self.push_sizes(&signature.params)?;
        let entry_unreachable = self.is_unreachable();
        self.control_stack.push(ControlFrame {
            kind,
            has_else: false,
            start_ip: self.instructions.len(),
            branch_jumps: initial_jump.into_iter().collect(),
            height,
            base,
            params: signature.params,
            results: signature.results,
            unreachable: entry_unreachable,
            entry_unreachable,
            end_reachable: false,
        });
        Ok(())
    }

    /// Emits the stack-shaping instruction required by a branch.
    fn emit_dropkeep(&mut self, base: ValueCounts, keep: ValueCounts) {
        let target = ValueCounts { c32: base.c32 + keep.c32, c64: base.c64 + keep.c64, c128: base.c128 + keep.c128 };
        if self.lane_counts == target {
            return;
        }
        self.instructions.push(Instruction::DropKeep((base, keep).into()));
    }

    fn patch_jump(&mut self, jump_ip: usize, target: usize) {
        match &mut self.instructions[jump_ip] {
            Instruction::Jump(ip) | Instruction::JumpIfZero32(ip) | Instruction::JumpIfNonZero32(ip) => {
                *ip = target as u32;
            }
            _ => {}
        }
    }

    fn value_counts(sizes: &[OperandSize]) -> ValueCounts {
        let mut counts = ValueCounts::default();
        for size in sizes {
            match size {
                OperandSize::S32 => counts.c32 += 1,
                OperandSize::S64 => counts.c64 += 1,
                OperandSize::S128 => counts.c128 += 1,
            }
        }
        counts
    }

    /// Shapes stack lanes to the values consumed by a branch target.
    fn emit_dropkeep_to_label(&mut self, label_depth: u32) -> Result<()> {
        if self.is_unreachable() {
            return Ok(());
        }
        let frame = &self.control_stack[self.get_ctx_idx(label_depth)?];
        let base = frame.base;
        let label_types = if matches!(frame.kind, BlockKind::Loop) { &frame.params } else { &frame.results };
        self.emit_dropkeep(base, Self::value_counts(label_types));
        Ok(())
    }

    fn emit_branch_jump_or_return(&mut self, depth: u32) -> Result<()> {
        let ctx_idx = self.get_ctx_idx(depth)?;
        match self.control_stack[ctx_idx].kind {
            BlockKind::Function => self.instructions.push(Instruction::Return),
            BlockKind::Loop => self.instructions.push(Instruction::Jump(self.control_stack[ctx_idx].start_ip as u32)),
            BlockKind::Block | BlockKind::If => {
                self.control_stack[ctx_idx].branch_jumps.push(self.instructions.len());
                self.control_stack[ctx_idx].end_reachable = true;
                self.instructions.push(Instruction::Jump(0));
            }
        }
        Ok(())
    }

    /// Resolves all jumps owned by a completed control frame.
    fn patch_end_jumps(&mut self, ctx: ControlFrame, end_ip: usize) {
        let target = if matches!(ctx.kind, BlockKind::Loop) { ctx.start_ip } else { end_ip };
        let mut jumps = ctx.branch_jumps.as_slice();
        if matches!(ctx.kind, BlockKind::If)
            && let Some((cond_jump, branch_jumps)) = jumps.split_first()
        {
            if !ctx.has_else {
                self.patch_jump(*cond_jump, end_ip);
            }
            jumps = branch_jumps;
        }
        for &jump in jumps {
            self.patch_jump(jump, target);
        }
    }
}
