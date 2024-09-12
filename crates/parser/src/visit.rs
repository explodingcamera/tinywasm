use crate::Result;

use crate::conversion::{convert_heaptype, convert_valtype};
use alloc::string::ToString;
use alloc::{boxed::Box, vec::Vec};
use tinywasm_types::{Instruction, MemoryArg};
use wasmparser::{FuncValidator, FuncValidatorAllocations, FunctionBody, VisitOperator, WasmModuleResources};

struct ValidateThenVisit<'a, R: WasmModuleResources>(usize, &'a mut FunctionBuilder<R>);
macro_rules! validate_then_visit {
    ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident)*) => {$(
        fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
            self.1.$visit($($($arg.clone()),*)?);
            self.1.validator_visitor(self.0).$visit($($($arg),*)?)?;
            Ok(())
        }
    )*};
}

impl<'a, R: WasmModuleResources> VisitOperator<'a> for ValidateThenVisit<'_, R> {
    type Output = Result<()>;
    wasmparser::for_each_operator!(validate_then_visit);
}

pub(crate) fn process_operators_and_validate<R: WasmModuleResources>(
    validator: FuncValidator<R>,
    body: FunctionBody<'_>,
    local_addr_map: Vec<u32>,
) -> Result<(Box<[Instruction]>, FuncValidatorAllocations)> {
    let mut reader = body.get_operators_reader()?;
    let remaining = reader.get_binary_reader().bytes_remaining();
    let mut builder = FunctionBuilder::new(remaining, validator, local_addr_map);

    while !reader.eof() {
        reader.visit_operator(&mut ValidateThenVisit(reader.original_position(), &mut builder))??;
    }

    builder.validator_finish(reader.original_position())?;
    if !builder.errors.is_empty() {
        return Err(builder.errors.remove(0));
    }

    Ok((builder.instructions.into_boxed_slice(), builder.validator.into_allocations()))
}

macro_rules! define_operand {
    ($name:ident($instr:ident, $ty:ty)) => {
        fn $name(&mut self, arg: $ty) -> Self::Output {
            self.instructions.push(Instruction::$instr(arg));
        }
    };

    ($name:ident($instr:ident, $ty:ty, $ty2:ty)) => {
        fn $name(&mut self, arg: $ty, arg2: $ty2) -> Self::Output {
            self.instructions.push(Instruction::$instr(arg, arg2));
        }
    };

    ($name:ident($instr:ident)) => {
        fn $name(&mut self) -> Self::Output {
            self.instructions.push(Instruction::$instr);
        }
    };
}

macro_rules! define_operands {
    ($($name:ident($instr:ident $(,$ty:ty)*)),*) => {$(
        define_operand!($name($instr $(,$ty)*));
    )*};
}

macro_rules! define_mem_operands {
    ($($name:ident($instr:ident)),*) => {$(
        fn $name(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
            self.instructions.push(Instruction::$instr {
                offset: memarg.offset,
                mem_addr: memarg.memory,
            });
        }
    )*};
}

pub(crate) struct FunctionBuilder<R: WasmModuleResources> {
    validator: FuncValidator<R>,
    instructions: Vec<Instruction>,
    label_ptrs: Vec<usize>,
    local_addr_map: Vec<u32>,
    errors: Vec<crate::ParseError>,
}

impl<R: WasmModuleResources> FunctionBuilder<R> {
    pub(crate) fn validator_visitor(
        &mut self,
        offset: usize,
    ) -> impl VisitOperator<'_, Output = Result<(), wasmparser::BinaryReaderError>> {
        self.validator.visitor(offset)
    }

    pub(crate) fn validator_finish(&mut self, offset: usize) -> Result<(), wasmparser::BinaryReaderError> {
        self.validator.finish(offset)
    }
}

impl<R: WasmModuleResources> FunctionBuilder<R> {
    pub(crate) fn new(instr_capacity: usize, validator: FuncValidator<R>, local_addr_map: Vec<u32>) -> Self {
        Self {
            validator,
            local_addr_map,
            instructions: Vec::with_capacity(instr_capacity),
            label_ptrs: Vec::with_capacity(256),
            errors: Vec::new(),
        }
    }

    fn unsupported(&mut self, name: &str) {
        self.errors.push(crate::ParseError::UnsupportedOperator(name.to_string()));
    }
}

macro_rules! impl_visit_operator {
    ($(@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident)*) => {
        $(impl_visit_operator!(@@$proposal $op $({ $($arg: $argty),* })? => $visit);)*
    };

    (@@mvp $($rest:tt)* ) => {};
    (@@reference_types $($rest:tt)* ) => {};
    (@@sign_extension $($rest:tt)* ) => {};
    (@@saturating_float_to_int $($rest:tt)* ) => {};
    (@@bulk_memory $($rest:tt)* ) => {};
    (@@tail_call $($rest:tt)* ) => {};
    // (@@simd $($rest:tt)* ) => {};
    (@@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident) => {
        #[cold]
        fn $visit(&mut self $($(,$arg: $argty)*)?) {
            self.unsupported(stringify!($visit))
        }
    };
}

impl<'a, R: WasmModuleResources> wasmparser::VisitOperator<'a> for FunctionBuilder<R> {
    type Output = ();
    wasmparser::for_each_operator!(impl_visit_operator);

    define_mem_operands! {
        visit_i32_load(I32Load), visit_i64_load(I64Load), visit_f32_load(F32Load), visit_f64_load(F64Load), visit_i32_load8_s(I32Load8S), visit_i32_load8_u(I32Load8U), visit_i32_load16_s(I32Load16S), visit_i32_load16_u(I32Load16U), visit_i64_load8_s(I64Load8S), visit_i64_load8_u(I64Load8U), visit_i64_load16_s(I64Load16S), visit_i64_load16_u(I64Load16U), visit_i64_load32_s(I64Load32S), visit_i64_load32_u(I64Load32U), /* visit_i32_store( I32Store), custom implementation */ visit_i64_store(I64Store), visit_f32_store(F32Store), visit_f64_store(F64Store), visit_i32_store8(I32Store8), visit_i32_store16(I32Store16), visit_i64_store8(I64Store8), visit_i64_store16(I64Store16), visit_i64_store32(I64Store32)
    }

    define_operands! {
        // basic instructions
        visit_br(Br, u32), visit_br_if(BrIf, u32), visit_global_get(GlobalGet, u32), visit_i32_const(I32Const, i32), visit_i64_const(I64Const, i64), visit_call(Call, u32), visit_memory_size(MemorySize, u32), visit_memory_grow(MemoryGrow, u32), visit_unreachable(Unreachable), visit_nop(Nop), visit_return(Return), visit_i32_eqz(I32Eqz), visit_i32_eq(I32Eq), visit_i32_ne(I32Ne), visit_i32_lt_s(I32LtS), visit_i32_lt_u(I32LtU), visit_i32_gt_s(I32GtS), visit_i32_gt_u(I32GtU), visit_i32_le_s(I32LeS), visit_i32_le_u(I32LeU), visit_i32_ge_s(I32GeS), visit_i32_ge_u(I32GeU), visit_i64_eqz(I64Eqz), visit_i64_eq(I64Eq), visit_i64_ne(I64Ne), visit_i64_lt_s(I64LtS), visit_i64_lt_u(I64LtU), visit_i64_gt_s(I64GtS), visit_i64_gt_u(I64GtU), visit_i64_le_s(I64LeS), visit_i64_le_u(I64LeU), visit_i64_ge_s(I64GeS), visit_i64_ge_u(I64GeU), visit_f32_eq(F32Eq), visit_f32_ne(F32Ne), visit_f32_lt(F32Lt), visit_f32_gt(F32Gt), visit_f32_le(F32Le), visit_f32_ge(F32Ge), visit_f64_eq(F64Eq), visit_f64_ne(F64Ne), visit_f64_lt(F64Lt), visit_f64_gt(F64Gt), visit_f64_le(F64Le), visit_f64_ge(F64Ge), visit_i32_clz(I32Clz), visit_i32_ctz(I32Ctz), visit_i32_popcnt(I32Popcnt), visit_i32_add(I32Add), visit_i32_sub(I32Sub), visit_i32_mul(I32Mul), visit_i32_div_s(I32DivS), visit_i32_div_u(I32DivU), visit_i32_rem_s(I32RemS), visit_i32_rem_u(I32RemU), visit_i32_and(I32And), visit_i32_or(I32Or), visit_i32_xor(I32Xor), visit_i32_shl(I32Shl), visit_i32_shr_s(I32ShrS), visit_i32_shr_u(I32ShrU), visit_i32_rotl(I32Rotl), visit_i32_rotr(I32Rotr), visit_i64_clz(I64Clz), visit_i64_ctz(I64Ctz), visit_i64_popcnt(I64Popcnt), visit_i64_add(I64Add), visit_i64_sub(I64Sub), visit_i64_mul(I64Mul), visit_i64_div_s(I64DivS), visit_i64_div_u(I64DivU), visit_i64_rem_s(I64RemS), visit_i64_rem_u(I64RemU), visit_i64_and(I64And), visit_i64_or(I64Or), visit_i64_xor(I64Xor), visit_i64_shl(I64Shl), visit_i64_shr_s(I64ShrS), visit_i64_shr_u(I64ShrU), visit_i64_rotl(I64Rotl), visit_i64_rotr(I64Rotr), visit_f32_abs(F32Abs), visit_f32_neg(F32Neg), visit_f32_ceil(F32Ceil), visit_f32_floor(F32Floor), visit_f32_trunc(F32Trunc), visit_f32_nearest(F32Nearest), visit_f32_sqrt(F32Sqrt), visit_f32_add(F32Add), visit_f32_sub(F32Sub), visit_f32_mul(F32Mul), visit_f32_div(F32Div), visit_f32_min(F32Min), visit_f32_max(F32Max), visit_f32_copysign(F32Copysign), visit_f64_abs(F64Abs), visit_f64_neg(F64Neg), visit_f64_ceil(F64Ceil), visit_f64_floor(F64Floor), visit_f64_trunc(F64Trunc), visit_f64_nearest(F64Nearest), visit_f64_sqrt(F64Sqrt), visit_f64_add(F64Add), visit_f64_sub(F64Sub), visit_f64_mul(F64Mul), visit_f64_div(F64Div), visit_f64_min(F64Min), visit_f64_max(F64Max), visit_f64_copysign(F64Copysign), visit_i32_wrap_i64(I32WrapI64), visit_i32_trunc_f32_s(I32TruncF32S), visit_i32_trunc_f32_u(I32TruncF32U), visit_i32_trunc_f64_s(I32TruncF64S), visit_i32_trunc_f64_u(I32TruncF64U), visit_i64_extend_i32_s(I64ExtendI32S), visit_i64_extend_i32_u(I64ExtendI32U), visit_i64_trunc_f32_s(I64TruncF32S), visit_i64_trunc_f32_u(I64TruncF32U), visit_i64_trunc_f64_s(I64TruncF64S), visit_i64_trunc_f64_u(I64TruncF64U), visit_f32_convert_i32_s(F32ConvertI32S), visit_f32_convert_i32_u(F32ConvertI32U), visit_f32_convert_i64_s(F32ConvertI64S), visit_f32_convert_i64_u(F32ConvertI64U), visit_f32_demote_f64(F32DemoteF64), visit_f64_convert_i32_s(F64ConvertI32S), visit_f64_convert_i32_u(F64ConvertI32U), visit_f64_convert_i64_s(F64ConvertI64S), visit_f64_convert_i64_u(F64ConvertI64U), visit_f64_promote_f32(F64PromoteF32), visit_i32_reinterpret_f32(I32ReinterpretF32), visit_i64_reinterpret_f64(I64ReinterpretF64), visit_f32_reinterpret_i32(F32ReinterpretI32), visit_f64_reinterpret_i64(F64ReinterpretI64),

        // sign_extension
        visit_i32_extend8_s(I32Extend8S), visit_i32_extend16_s(I32Extend16S), visit_i64_extend8_s(I64Extend8S), visit_i64_extend16_s(I64Extend16S), visit_i64_extend32_s(I64Extend32S),

        // Non-trapping Float-to-int Conversions
        visit_i32_trunc_sat_f32_s(I32TruncSatF32S), visit_i32_trunc_sat_f32_u(I32TruncSatF32U), visit_i32_trunc_sat_f64_s(I32TruncSatF64S), visit_i32_trunc_sat_f64_u(I32TruncSatF64U), visit_i64_trunc_sat_f32_s(I64TruncSatF32S), visit_i64_trunc_sat_f32_u(I64TruncSatF32U), visit_i64_trunc_sat_f64_s(I64TruncSatF64S), visit_i64_trunc_sat_f64_u(I64TruncSatF64U),

        // Reference Types
        visit_ref_func(RefFunc, u32), visit_table_fill(TableFill, u32), visit_table_get(TableGet, u32), visit_table_set(TableSet, u32), visit_table_grow(TableGrow, u32), visit_table_size(TableSize, u32),

        // Bulk Memory
        visit_memory_init(MemoryInit, u32, u32), visit_memory_copy(MemoryCopy, u32, u32), visit_table_init(TableInit, u32, u32), visit_memory_fill(MemoryFill, u32), visit_data_drop(DataDrop, u32), visit_elem_drop(ElemDrop, u32)

        // simd
        // visit_v128_load(V128Load), visit_v128_store(V128Store), visit_v128_const(V128Const), visit_v128_not(V128Not), visit_v128_and(V128And), visit_v128_or(V128Or), visit_v128_xor(V128Xor), visit_v128_bitselect(V128Bitselect), visit_v128_any_true(V128AnyTrue), visit_v128_all_true(V128AllTrue), visit_v128_shl(V128Shl), visit_v128_shr_s(V128ShrS), visit_v128_shr_u(V128ShrU), visit_v128_add(V128Add), visit_v128_sub(V128Sub), visit_v128_mul(V128Mul), visit_v128_div_s(V128DivS), visit_v128_div_u(V128DivU), visit_v128_min_s(V128MinS), visit_v128_min_u(V128MinU), visit_v128_max_s(V128MaxS), visit_v128_max_u(V128MaxU), visit_v128_eq(V128Eq), visit_v128_ne(V128Ne), visit_v128_lt_s(V128LtS), visit_v128_lt_u(V128LtU), visit_v128_le_s(V128LeS), visit_v128_le_u(V128LeU), visit_v128_gt_s(V128GtS), visit_v128_gt_u(V128GtU), visit_v128_ge_s(V128GeS), visit_v128_ge_u(V128GeU), visit_v128_narrow_i32x4_s(V128NarrowI32x4S), visit_v128_narrow_i32x4_u(V128NarrowI32x4U), visit_v128_widen_low_i8x16_s(V128WidenLowI8x16S), visit_v128_widen_high_i8x16_s(V128WidenHighI8x16S), visit_v128_widen_low_i8x16_u(V128WidenLowI8x16U), visit_v128_widen_high_i8x16_u(V128WidenHighI8x16U), visit_v128_widen_low_i16x8_s(V128WidenLowI16x8S), visit_v128_widen_high_i16x8_s(V128WidenHighI16x8S), visit_v128_widen_low_i16x8_u(V128WidenLowI16x8U)
    }

    fn visit_return_call(&mut self, function_index: u32) -> Self::Output {
        self.instructions.push(Instruction::ReturnCall(function_index));
    }

    fn visit_return_call_indirect(&mut self, type_index: u32, table_index: u32) -> Self::Output {
        self.instructions.push(Instruction::ReturnCallIndirect(type_index, table_index));
    }

    fn visit_global_set(&mut self, global_index: u32) -> Self::Output {
        match self.validator.get_operand_type(0) {
            Some(Some(t)) => self.instructions.push(match t {
                wasmparser::ValType::I32 => Instruction::GlobalSet32(global_index),
                wasmparser::ValType::F32 => Instruction::GlobalSet32(global_index),
                wasmparser::ValType::I64 => Instruction::GlobalSet64(global_index),
                wasmparser::ValType::F64 => Instruction::GlobalSet64(global_index),
                wasmparser::ValType::V128 => Instruction::GlobalSet128(global_index),
                wasmparser::ValType::Ref(_) => Instruction::GlobalSetRef(global_index),
            }),
            _ => self.visit_unreachable(),
        }
    }

    fn visit_drop(&mut self) -> Self::Output {
        match self.validator.get_operand_type(0) {
            Some(Some(t)) => self.instructions.push(match t {
                wasmparser::ValType::I32 => Instruction::Drop32,
                wasmparser::ValType::F32 => Instruction::Drop32,
                wasmparser::ValType::I64 => Instruction::Drop64,
                wasmparser::ValType::F64 => Instruction::Drop64,
                wasmparser::ValType::V128 => Instruction::Drop128,
                wasmparser::ValType::Ref(_) => Instruction::DropRef,
            }),
            _ => self.visit_unreachable(),
        }
    }
    fn visit_select(&mut self) -> Self::Output {
        match self.validator.get_operand_type(1) {
            Some(Some(t)) => self.visit_typed_select(t),
            _ => self.visit_unreachable(),
        }
    }
    fn visit_i32_store(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
        let arg = MemoryArg { offset: memarg.offset, mem_addr: memarg.memory };
        let i32store = Instruction::I32Store { offset: arg.offset, mem_addr: arg.mem_addr };
        self.instructions.push(i32store);
    }

    fn visit_local_get(&mut self, idx: u32) -> Self::Output {
        let Ok(resolved_idx) = self.local_addr_map[idx as usize].try_into() else {
            self.errors.push(crate::ParseError::UnsupportedOperator(
                "Local index is too large, tinywasm does not support local indexes that large".to_string(),
            ));
            return;
        };

        match self.validator.get_local_type(idx) {
            Some(t) => self.instructions.push(match t {
                wasmparser::ValType::I32 => Instruction::LocalGet32(resolved_idx),
                wasmparser::ValType::F32 => Instruction::LocalGet32(resolved_idx),
                wasmparser::ValType::I64 => Instruction::LocalGet64(resolved_idx),
                wasmparser::ValType::F64 => Instruction::LocalGet64(resolved_idx),
                wasmparser::ValType::V128 => Instruction::LocalGet128(resolved_idx),
                wasmparser::ValType::Ref(_) => Instruction::LocalGetRef(resolved_idx),
            }),
            _ => self.visit_unreachable(),
        }
    }

    fn visit_local_set(&mut self, idx: u32) -> Self::Output {
        let Ok(resolved_idx) = self.local_addr_map[idx as usize].try_into() else {
            self.errors.push(crate::ParseError::UnsupportedOperator(
                "Local index is too large, tinywasm does not support local indexes that large".to_string(),
            ));
            return;
        };

        if let Some(
            Instruction::LocalGet32(from)
            | Instruction::LocalGet64(from)
            | Instruction::LocalGet128(from)
            | Instruction::LocalGetRef(from),
        ) = self.instructions.last()
        {
            let from = *from;
            self.instructions.pop();
            // validation will ensure that the last instruction is the correct local.get
            match self.validator.get_operand_type(0) {
                Some(Some(t)) => self.instructions.push(match t {
                    wasmparser::ValType::I32 => Instruction::LocalCopy32(from, resolved_idx),
                    wasmparser::ValType::F32 => Instruction::LocalCopy32(from, resolved_idx),
                    wasmparser::ValType::I64 => Instruction::LocalCopy64(from, resolved_idx),
                    wasmparser::ValType::F64 => Instruction::LocalCopy64(from, resolved_idx),
                    wasmparser::ValType::V128 => Instruction::LocalCopy128(from, resolved_idx),
                    wasmparser::ValType::Ref(_) => Instruction::LocalCopyRef(from, resolved_idx),
                }),
                _ => self.visit_unreachable(),
            }
            return;
        }

        match self.validator.get_operand_type(0) {
            Some(Some(t)) => self.instructions.push(match t {
                wasmparser::ValType::I32 => Instruction::LocalSet32(resolved_idx),
                wasmparser::ValType::F32 => Instruction::LocalSet32(resolved_idx),
                wasmparser::ValType::I64 => Instruction::LocalSet64(resolved_idx),
                wasmparser::ValType::F64 => Instruction::LocalSet64(resolved_idx),
                wasmparser::ValType::V128 => Instruction::LocalSet128(resolved_idx),
                wasmparser::ValType::Ref(_) => Instruction::LocalSetRef(resolved_idx),
            }),
            _ => self.visit_unreachable(),
        }
    }

    fn visit_local_tee(&mut self, idx: u32) -> Self::Output {
        let Ok(resolved_idx) = self.local_addr_map[idx as usize].try_into() else {
            self.errors.push(crate::ParseError::UnsupportedOperator(
                "Local index is too large, tinywasm does not support local indexes that large".to_string(),
            ));
            return;
        };

        match self.validator.get_operand_type(0) {
            Some(Some(t)) => self.instructions.push(match t {
                wasmparser::ValType::I32 => Instruction::LocalTee32(resolved_idx),
                wasmparser::ValType::F32 => Instruction::LocalTee32(resolved_idx),
                wasmparser::ValType::I64 => Instruction::LocalTee64(resolved_idx),
                wasmparser::ValType::F64 => Instruction::LocalTee64(resolved_idx),
                wasmparser::ValType::V128 => Instruction::LocalTee128(resolved_idx),
                wasmparser::ValType::Ref(_) => Instruction::LocalTeeRef(resolved_idx),
            }),
            _ => self.visit_unreachable(),
        }
    }

    fn visit_block(&mut self, blockty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.instructions.push(match blockty {
            wasmparser::BlockType::Empty => Instruction::Block(0),
            wasmparser::BlockType::FuncType(idx) => Instruction::BlockWithFuncType(idx, 0),
            wasmparser::BlockType::Type(ty) => Instruction::BlockWithType(convert_valtype(&ty), 0),
        });
    }

    fn visit_loop(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.instructions.push(match ty {
            wasmparser::BlockType::Empty => Instruction::Loop(0),
            wasmparser::BlockType::FuncType(idx) => Instruction::LoopWithFuncType(idx, 0),
            wasmparser::BlockType::Type(ty) => Instruction::LoopWithType(convert_valtype(&ty), 0),
        });
    }

    fn visit_if(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.instructions.push(match ty {
            wasmparser::BlockType::Empty => Instruction::If(0, 0),
            wasmparser::BlockType::FuncType(idx) => Instruction::IfWithFuncType(idx, 0, 0),
            wasmparser::BlockType::Type(ty) => Instruction::IfWithType(convert_valtype(&ty), 0, 0),
        });
    }

    fn visit_else(&mut self) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.instructions.push(Instruction::Else(0));
    }

    fn visit_end(&mut self) -> Self::Output {
        let Some(label_pointer) = self.label_ptrs.pop() else {
            return self.instructions.push(Instruction::Return);
        };

        let current_instr_ptr = self.instructions.len();
        match self.instructions.get_mut(label_pointer) {
            Some(Instruction::Else(else_instr_end_offset)) => {
                *else_instr_end_offset = (current_instr_ptr - label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support if blocks that large");

                // since we're ending an else block, we need to end the if block as well
                let Some(if_label_pointer) = self.label_ptrs.pop() else {
                    self.errors.push(crate::ParseError::UnsupportedOperator(
                        "Expected to end an if block, but there was no if block to end".to_string(),
                    ));

                    return;
                };

                let if_instruction = &mut self.instructions[if_label_pointer];

                let (else_offset, end_offset) = match if_instruction {
                    Instruction::If(else_offset, end_offset)
                    | Instruction::IfWithFuncType(_, else_offset, end_offset)
                    | Instruction::IfWithType(_, else_offset, end_offset) => (else_offset, end_offset),
                    _ => {
                        self.errors.push(crate::ParseError::UnsupportedOperator(
                            "Expected to end an if block, but the last label was not an if".to_string(),
                        ));

                        return;
                    }
                };

                *else_offset = (label_pointer - if_label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support blocks that large");

                *end_offset = (current_instr_ptr - if_label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support blocks that large");
            }
            Some(
                Instruction::Block(end_offset)
                | Instruction::BlockWithType(_, end_offset)
                | Instruction::BlockWithFuncType(_, end_offset)
                | Instruction::Loop(end_offset)
                | Instruction::LoopWithFuncType(_, end_offset)
                | Instruction::LoopWithType(_, end_offset)
                | Instruction::If(_, end_offset)
                | Instruction::IfWithFuncType(_, _, end_offset)
                | Instruction::IfWithType(_, _, end_offset),
            ) => {
                *end_offset = (current_instr_ptr - label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support  blocks that large");
            }
            _ => {
                unreachable!("Expected to end a block, but the last label was not a block")
            }
        };

        self.instructions.push(Instruction::EndBlockFrame);
    }

    fn visit_br_table(&mut self, targets: wasmparser::BrTable<'_>) -> Self::Output {
        let def = targets.default();
        let instrs = targets
            .targets()
            .map(|t| t.map(Instruction::BrLabel))
            .collect::<Result<Vec<Instruction>, wasmparser::BinaryReaderError>>()
            .expect("visit_br_table: BrTable targets are invalid, this should have been caught by the validator");

        self.instructions.extend(([Instruction::BrTable(def, instrs.len() as u32)].into_iter()).chain(instrs));
    }

    fn visit_call_indirect(&mut self, ty: u32, table: u32) -> Self::Output {
        self.instructions.push(Instruction::CallIndirect(ty, table));
    }

    fn visit_f32_const(&mut self, val: wasmparser::Ieee32) -> Self::Output {
        self.instructions.push(Instruction::F32Const(f32::from_bits(val.bits())));
    }

    fn visit_f64_const(&mut self, val: wasmparser::Ieee64) -> Self::Output {
        self.instructions.push(Instruction::F64Const(f64::from_bits(val.bits())));
    }

    fn visit_table_copy(&mut self, dst_table: u32, src_table: u32) -> Self::Output {
        self.instructions.push(Instruction::TableCopy { from: src_table, to: dst_table });
    }

    // Reference Types
    fn visit_ref_null(&mut self, ty: wasmparser::HeapType) -> Self::Output {
        self.instructions.push(Instruction::RefNull(convert_heaptype(ty)));
    }

    fn visit_ref_is_null(&mut self) -> Self::Output {
        self.instructions.push(Instruction::RefIsNull);
    }

    fn visit_typed_select(&mut self, ty: wasmparser::ValType) -> Self::Output {
        self.instructions.push(match ty {
            wasmparser::ValType::I32 => Instruction::Select32,
            wasmparser::ValType::F32 => Instruction::Select32,
            wasmparser::ValType::I64 => Instruction::Select64,
            wasmparser::ValType::F64 => Instruction::Select64,
            wasmparser::ValType::V128 => Instruction::Select128,
            wasmparser::ValType::Ref(_) => Instruction::SelectRef,
        });
    }
}
