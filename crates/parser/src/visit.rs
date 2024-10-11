use crate::Result;

use crate::conversion::{convert_heaptype, convert_valtype};
use alloc::string::ToString;
use alloc::{boxed::Box, vec::Vec};
use tinywasm_types::{Instruction, MemoryArg, SimdInstruction, WasmFunctionData};
use wasmparser::{FuncValidator, FuncValidatorAllocations, FunctionBody, VisitOperator, WasmModuleResources};

struct ValidateThenVisit<'a, R: WasmModuleResources>(usize, &'a mut FunctionBuilder<R>);
macro_rules! validate_then_visit {
    ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {$(
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
) -> Result<(Box<[Instruction]>, WasmFunctionData, FuncValidatorAllocations)> {
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

    Ok((
        builder.instructions.into_boxed_slice(),
        WasmFunctionData { v128_constants: builder.v128_constants.into_boxed_slice() },
        builder.validator.into_allocations(),
    ))
}

macro_rules! define_operand {
    ($name:ident($instr:expr, $ty:ty)) => {
        fn $name(&mut self, arg: $ty) -> Self::Output {
            self.instructions.push($instr(arg).into());
        }
    };

    ($name:ident($instr:expr, $ty:ty, $ty2:ty)) => {
        fn $name(&mut self, arg: $ty, arg2: $ty2) -> Self::Output {
            self.instructions.push($instr(arg, arg2).into());
        }
    };

    ($name:ident($instr:expr)) => {
        fn $name(&mut self) -> Self::Output {
            self.instructions.push($instr.into());
        }
    };
}

macro_rules! define_operands {
    ($($name:ident($instr:ident $(,$ty:ty)*)),*) => {$(
        define_operand!($name(Instruction::$instr $(,$ty)*));
    )*};
}

macro_rules! define_operands_simd {
    ($($name:ident($instr:ident $(,$ty:ty)*)),*) => {$(
        define_operand!($name(SimdInstruction::$instr $(,$ty)*));
    )*};
}

macro_rules! define_mem_operands {
    ($($name:ident($instr:ident)),*) => {$(
        fn $name(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
            self.instructions.push(Instruction::$instr(MemoryArg::new(memarg.offset, memarg.memory)));
        }
    )*};
}

macro_rules! define_mem_operands_simd {
    ($($name:ident($instr:ident)),*) => {$(
        fn $name(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
            self.instructions.push(SimdInstruction::$instr(MemoryArg::new(memarg.offset, memarg.memory)).into());
        }
    )*};
}

macro_rules! define_mem_operands_simd_lane {
    ($($name:ident($instr:ident)),*) => {$(
        fn $name(&mut self, memarg: wasmparser::MemArg, lane: u8) -> Self::Output {
            self.instructions.push(SimdInstruction::$instr(MemoryArg::new(memarg.offset, memarg.memory), lane).into());
        }
    )*};
}

pub(crate) struct FunctionBuilder<R: WasmModuleResources> {
    validator: FuncValidator<R>,
    instructions: Vec<Instruction>,
    v128_constants: Vec<u128>,
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
            v128_constants: Vec::new(),
            label_ptrs: Vec::with_capacity(256),
            errors: Vec::new(),
        }
    }

    fn unsupported(&mut self, name: &str) {
        self.errors.push(crate::ParseError::UnsupportedOperator(name.to_string()));
    }
}

macro_rules! impl_visit_operator {
    ($(@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {
        $(impl_visit_operator!(@@$proposal $op $({ $($arg: $argty),* })? => $visit ($($ann:tt)*));)*
    };

    (@@mvp $($rest:tt)* ) => {};
    (@@reference_types $($rest:tt)* ) => {};
    (@@sign_extension $($rest:tt)* ) => {};
    (@@saturating_float_to_int $($rest:tt)* ) => {};
    (@@bulk_memory $($rest:tt)* ) => {};
    // (@@tail_call $($rest:tt)* ) => {};
    (@@simd $($rest:tt)* ) => {};
    (@@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*)) => {
        #[cold]
        fn $visit(&mut self $($(,$arg: $argty)*)?) {
            self.unsupported(stringify!($visit))
        }
    };
}

impl<R: WasmModuleResources> wasmparser::VisitOperator<'_> for FunctionBuilder<R> {
    type Output = ();
    wasmparser::for_each_operator!(impl_visit_operator);

    define_mem_operands! {
        visit_i32_load(I32Load), visit_i64_load(I64Load), visit_f32_load(F32Load), visit_f64_load(F64Load), visit_i32_load8_s(I32Load8S), visit_i32_load8_u(I32Load8U), visit_i32_load16_s(I32Load16S), visit_i32_load16_u(I32Load16U), visit_i64_load8_s(I64Load8S), visit_i64_load8_u(I64Load8U), visit_i64_load16_s(I64Load16S), visit_i64_load16_u(I64Load16U), visit_i64_load32_s(I64Load32S), visit_i64_load32_u(I64Load32U), visit_i32_store( I32Store), visit_i64_store(I64Store), visit_f32_store(F32Store), visit_f64_store(F64Store), visit_i32_store8(I32Store8), visit_i32_store16(I32Store16), visit_i64_store8(I64Store8), visit_i64_store16(I64Store16), visit_i64_store32(I64Store32)
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
    }

    // simd
    define_mem_operands_simd! {
        visit_v128_load(V128Load), visit_v128_load8x8_s(V128Load8x8S), visit_v128_load8x8_u(V128Load8x8U), visit_v128_load16x4_s(V128Load16x4S), visit_v128_load16x4_u(V128Load16x4U), visit_v128_load32x2_s(V128Load32x2S), visit_v128_load32x2_u(V128Load32x2U), visit_v128_load8_splat(V128Load8Splat), visit_v128_load16_splat(V128Load16Splat), visit_v128_load32_splat(V128Load32Splat), visit_v128_load64_splat(V128Load64Splat), visit_v128_load32_zero(V128Load32Zero), visit_v128_load64_zero(V128Load64Zero), visit_v128_store(V128Store)
    }
    define_mem_operands_simd_lane! {
        visit_v128_load8_lane(V128Load8Lane), visit_v128_load16_lane(V128Load16Lane), visit_v128_load32_lane(V128Load32Lane), visit_v128_load64_lane(V128Load64Lane),
        visit_v128_store8_lane(V128Store8Lane), visit_v128_store16_lane(V128Store16Lane), visit_v128_store32_lane(V128Store32Lane), visit_v128_store64_lane(V128Store64Lane)
    }
    define_operands_simd! {
        visit_v128_not(V128Not), visit_v128_and(V128And), visit_v128_andnot(V128AndNot), visit_v128_or(V128Or), visit_v128_xor(V128Xor), visit_v128_bitselect(V128Bitselect), visit_v128_any_true(V128AnyTrue),
        visit_i8x16_splat(I8x16Splat), visit_i8x16_swizzle(I8x16Swizzle), visit_i8x16_eq(I8x16Eq), visit_i8x16_ne(I8x16Ne), visit_i8x16_lt_s(I8x16LtS), visit_i8x16_lt_u(I8x16LtU), visit_i8x16_gt_s(I8x16GtS), visit_i8x16_gt_u(I8x16GtU), visit_i8x16_le_s(I8x16LeS), visit_i8x16_le_u(I8x16LeU), visit_i8x16_ge_s(I8x16GeS), visit_i8x16_ge_u(I8x16GeU),
        visit_i16x8_splat(I16x8Splat), visit_i16x8_eq(I16x8Eq), visit_i16x8_ne(I16x8Ne), visit_i16x8_lt_s(I16x8LtS), visit_i16x8_lt_u(I16x8LtU), visit_i16x8_gt_s(I16x8GtS), visit_i16x8_gt_u(I16x8GtU), visit_i16x8_le_s(I16x8LeS), visit_i16x8_le_u(I16x8LeU), visit_i16x8_ge_s(I16x8GeS), visit_i16x8_ge_u(I16x8GeU),
        visit_i32x4_splat(I32x4Splat), visit_i32x4_eq(I32x4Eq), visit_i32x4_ne(I32x4Ne), visit_i32x4_lt_s(I32x4LtS), visit_i32x4_lt_u(I32x4LtU), visit_i32x4_gt_s(I32x4GtS), visit_i32x4_gt_u(I32x4GtU), visit_i32x4_le_s(I32x4LeS), visit_i32x4_le_u(I32x4LeU), visit_i32x4_ge_s(I32x4GeS), visit_i32x4_ge_u(I32x4GeU),
        visit_i64x2_splat(I64x2Splat), visit_i64x2_eq(I64x2Eq), visit_i64x2_ne(I64x2Ne), visit_i64x2_lt_s(I64x2LtS), visit_i64x2_gt_s(I64x2GtS), visit_i64x2_le_s(I64x2LeS), visit_i64x2_ge_s(I64x2GeS),
        visit_f32x4_splat(F32x4Splat), visit_f32x4_eq(F32x4Eq), visit_f32x4_ne(F32x4Ne), visit_f32x4_lt(F32x4Lt), visit_f32x4_gt(F32x4Gt), visit_f32x4_le(F32x4Le), visit_f32x4_ge(F32x4Ge),
        visit_f64x2_splat(F64x2Splat), visit_f64x2_eq(F64x2Eq), visit_f64x2_ne(F64x2Ne), visit_f64x2_lt(F64x2Lt), visit_f64x2_gt(F64x2Gt), visit_f64x2_le(F64x2Le), visit_f64x2_ge(F64x2Ge),
        visit_i8x16_abs(I8x16Abs), visit_i8x16_neg(I8x16Neg), visit_i8x16_all_true(I8x16AllTrue), visit_i8x16_bitmask(I8x16Bitmask), visit_i8x16_shl(I8x16Shl), visit_i8x16_shr_s(I8x16ShrS), visit_i8x16_shr_u(I8x16ShrU), visit_i8x16_add(I8x16Add), visit_i8x16_sub(I8x16Sub), visit_i8x16_min_s(I8x16MinS), visit_i8x16_min_u(I8x16MinU), visit_i8x16_max_s(I8x16MaxS), visit_i8x16_max_u(I8x16MaxU),
        visit_i16x8_abs(I16x8Abs), visit_i16x8_neg(I16x8Neg), visit_i16x8_all_true(I16x8AllTrue), visit_i16x8_bitmask(I16x8Bitmask), visit_i16x8_shl(I16x8Shl), visit_i16x8_shr_s(I16x8ShrS), visit_i16x8_shr_u(I16x8ShrU), visit_i16x8_add(I16x8Add), visit_i16x8_sub(I16x8Sub), visit_i16x8_min_s(I16x8MinS), visit_i16x8_min_u(I16x8MinU), visit_i16x8_max_s(I16x8MaxS), visit_i16x8_max_u(I16x8MaxU),
        visit_i32x4_abs(I32x4Abs), visit_i32x4_neg(I32x4Neg), visit_i32x4_all_true(I32x4AllTrue), visit_i32x4_bitmask(I32x4Bitmask), visit_i32x4_shl(I32x4Shl), visit_i32x4_shr_s(I32x4ShrS), visit_i32x4_shr_u(I32x4ShrU), visit_i32x4_add(I32x4Add), visit_i32x4_sub(I32x4Sub), visit_i32x4_min_s(I32x4MinS), visit_i32x4_min_u(I32x4MinU), visit_i32x4_max_s(I32x4MaxS), visit_i32x4_max_u(I32x4MaxU),
        visit_i64x2_abs(I64x2Abs), visit_i64x2_neg(I64x2Neg), visit_i64x2_all_true(I64x2AllTrue), visit_i64x2_bitmask(I64x2Bitmask), visit_i64x2_shl(I64x2Shl), visit_i64x2_shr_s(I64x2ShrS), visit_i64x2_shr_u(I64x2ShrU), visit_i64x2_add(I64x2Add), visit_i64x2_sub(I64x2Sub), visit_i64x2_mul(I64x2Mul),
        visit_i8x16_narrow_i16x8_s(I8x16NarrowI16x8S), visit_i8x16_narrow_i16x8_u(I8x16NarrowI16x8U), visit_i8x16_add_sat_s(I8x16AddSatS), visit_i8x16_add_sat_u(I8x16AddSatU), visit_i8x16_sub_sat_s(I8x16SubSatS), visit_i8x16_sub_sat_u(I8x16SubSatU), visit_i8x16_avgr_u(I8x16AvgrU),
        visit_i16x8_narrow_i32x4_s(I16x8NarrowI32x4S), visit_i16x8_narrow_i32x4_u(I16x8NarrowI32x4U), visit_i16x8_add_sat_s(I16x8AddSatS), visit_i16x8_add_sat_u(I16x8AddSatU), visit_i16x8_sub_sat_s(I16x8SubSatS), visit_i16x8_sub_sat_u(I16x8SubSatU), visit_i16x8_avgr_u(I16x8AvgrU),
        visit_i16x8_extadd_pairwise_i8x16_s(I16x8ExtAddPairwiseI8x16S), visit_i16x8_extadd_pairwise_i8x16_u(I16x8ExtAddPairwiseI8x16U), visit_i16x8_mul(I16x8Mul),
        visit_i32x4_extadd_pairwise_i16x8_s(I32x4ExtAddPairwiseI16x8S), visit_i32x4_extadd_pairwise_i16x8_u(I32x4ExtAddPairwiseI16x8U), visit_i32x4_mul(I32x4Mul),
        visit_i16x8_extmul_low_i8x16_s(I16x8ExtMulLowI8x16S), visit_i16x8_extmul_low_i8x16_u(I16x8ExtMulLowI8x16U), visit_i16x8_extmul_high_i8x16_s(I16x8ExtMulHighI8x16S), visit_i16x8_extmul_high_i8x16_u(I16x8ExtMulHighI8x16U),
        visit_i32x4_extmul_low_i16x8_s(I32x4ExtMulLowI16x8S), visit_i32x4_extmul_low_i16x8_u(I32x4ExtMulLowI16x8U), visit_i32x4_extmul_high_i16x8_s(I32x4ExtMulHighI16x8S), visit_i32x4_extmul_high_i16x8_u(I32x4ExtMulHighI16x8U),
        visit_i64x2_extmul_low_i32x4_s(I64x2ExtMulLowI32x4S), visit_i64x2_extmul_low_i32x4_u(I64x2ExtMulLowI32x4U), visit_i64x2_extmul_high_i32x4_s(I64x2ExtMulHighI32x4S), visit_i64x2_extmul_high_i32x4_u(I64x2ExtMulHighI32x4U),
        visit_i16x8_extend_low_i8x16_s(I16x8ExtendLowI8x16S), visit_i16x8_extend_low_i8x16_u(I16x8ExtendLowI8x16U), visit_i16x8_extend_high_i8x16_s(I16x8ExtendHighI8x16S), visit_i16x8_extend_high_i8x16_u(I16x8ExtendHighI8x16U),
        visit_i32x4_extend_low_i16x8_s(I32x4ExtendLowI16x8S), visit_i32x4_extend_low_i16x8_u(I32x4ExtendLowI16x8U), visit_i32x4_extend_high_i16x8_s(I32x4ExtendHighI16x8S), visit_i32x4_extend_high_i16x8_u(I32x4ExtendHighI16x8U),
        visit_i64x2_extend_low_i32x4_s(I64x2ExtendLowI32x4S), visit_i64x2_extend_low_i32x4_u(I64x2ExtendLowI32x4U), visit_i64x2_extend_high_i32x4_s(I64x2ExtendHighI32x4S), visit_i64x2_extend_high_i32x4_u(I64x2ExtendHighI32x4U),
        visit_i8x16_popcnt(I8x16Popcnt), visit_i16x8_q15mulr_sat_s(I16x8Q15MulrSatS), visit_i32x4_dot_i16x8_s(I32x4DotI16x8S),
        visit_f32x4_ceil(F32x4Ceil), visit_f32x4_floor(F32x4Floor), visit_f32x4_trunc(F32x4Trunc), visit_f32x4_nearest(F32x4Nearest), visit_f32x4_abs(F32x4Abs), visit_f32x4_neg(F32x4Neg), visit_f32x4_sqrt(F32x4Sqrt), visit_f32x4_add(F32x4Add), visit_f32x4_sub(F32x4Sub), visit_f32x4_mul(F32x4Mul), visit_f32x4_div(F32x4Div), visit_f32x4_min(F32x4Min), visit_f32x4_max(F32x4Max), visit_f32x4_pmin(F32x4PMin), visit_f32x4_pmax(F32x4PMax),
        visit_f64x2_ceil(F64x2Ceil), visit_f64x2_floor(F64x2Floor), visit_f64x2_trunc(F64x2Trunc), visit_f64x2_nearest(F64x2Nearest), visit_f64x2_abs(F64x2Abs), visit_f64x2_neg(F64x2Neg), visit_f64x2_sqrt(F64x2Sqrt), visit_f64x2_add(F64x2Add), visit_f64x2_sub(F64x2Sub), visit_f64x2_mul(F64x2Mul), visit_f64x2_div(F64x2Div), visit_f64x2_min(F64x2Min), visit_f64x2_max(F64x2Max), visit_f64x2_pmin(F64x2PMin), visit_f64x2_pmax(F64x2PMax),
        visit_i32x4_trunc_sat_f32x4_s(I32x4TruncSatF32x4S), visit_i32x4_trunc_sat_f32x4_u(I32x4TruncSatF32x4U),
        visit_f32x4_convert_i32x4_s(F32x4ConvertI32x4S), visit_f32x4_convert_i32x4_u(F32x4ConvertI32x4U),
        visit_i32x4_trunc_sat_f64x2_s_zero(I32x4TruncSatF64x2SZero), visit_i32x4_trunc_sat_f64x2_u_zero(I32x4TruncSatF64x2UZero),
        visit_f64x2_convert_low_i32x4_s(F64x2ConvertLowI32x4S), visit_f64x2_convert_low_i32x4_u(F64x2ConvertLowI32x4U),
        visit_f32x4_demote_f64x2_zero(F32x4DemoteF64x2Zero), visit_f64x2_promote_low_f32x4(F64x2PromoteLowF32x4),

        visit_i8x16_extract_lane_s(I8x16ExtractLaneS, u8), visit_i8x16_extract_lane_u(I8x16ExtractLaneU, u8), visit_i8x16_replace_lane(I8x16ReplaceLane, u8),
        visit_i16x8_extract_lane_s(I16x8ExtractLaneS, u8), visit_i16x8_extract_lane_u(I16x8ExtractLaneU, u8), visit_i16x8_replace_lane(I16x8ReplaceLane, u8),
        visit_i32x4_extract_lane(I32x4ExtractLane, u8), visit_i32x4_replace_lane(I32x4ReplaceLane, u8),
        visit_i64x2_extract_lane(I64x2ExtractLane, u8), visit_i64x2_replace_lane(I64x2ReplaceLane, u8),
        visit_f32x4_extract_lane(F32x4ExtractLane, u8), visit_f32x4_replace_lane(F32x4ReplaceLane, u8),
        visit_f64x2_extract_lane(F64x2ExtractLane, u8), visit_f64x2_replace_lane(F64x2ReplaceLane, u8)
    }

    fn visit_i8x16_shuffle(&mut self, lanes: [u8; 16]) -> Self::Output {
        self.v128_constants.push(u128::from_le_bytes(lanes));
        self.instructions.push(SimdInstruction::I8x16Shuffle(self.v128_constants.len() as u32 - 1).into());
    }

    fn visit_v128_const(&mut self, value: wasmparser::V128) -> Self::Output {
        self.v128_constants.push(value.i128() as u128);
        self.instructions.push(SimdInstruction::V128Const(self.v128_constants.len() as u32 - 1).into());
    }

    // fn visit_return_call(&mut self, function_index: u32) -> Self::Output {
    //     self.instructions.push(Instruction::ReturnCall(function_index));
    // }

    // fn visit_return_call_indirect(&mut self, type_index: u32, table_index: u32) -> Self::Output {
    //     self.instructions.push(Instruction::ReturnCallIndirect(type_index, table_index));
    // }

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
