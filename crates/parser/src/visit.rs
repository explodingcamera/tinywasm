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

macro_rules! define_operands {
    ($($name:ident, $instr:expr),*) => {$(
        fn $name(&mut self) -> Self::Output {
            self.instructions.push($instr);
        }
    )*};
}

macro_rules! define_primitive_operands {
    ($($name:ident, $instr:expr, $ty:ty),*) => {$(
        fn $name(&mut self, arg: $ty) -> Self::Output {
            self.instructions.push($instr(arg));
        }
    )*};
    ($($name:ident, $instr:expr, $ty:ty, $ty2:ty),*) => {$(
        fn $name(&mut self, arg: $ty, arg2: $ty2) -> Self::Output {
            self.instructions.push($instr(arg, arg2));
        }
    )*};
}

macro_rules! define_mem_operands {
    ($($name:ident, $instr:ident),*) => {$(
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

    define_primitive_operands! {
        visit_br, Instruction::Br, u32,
        visit_br_if, Instruction::BrIf, u32,
        visit_global_get, Instruction::GlobalGet, u32,
        visit_i32_const, Instruction::I32Const, i32,
        visit_i64_const, Instruction::I64Const, i64,
        visit_call, Instruction::Call, u32,
        visit_memory_size, Instruction::MemorySize, u32,
        visit_memory_grow, Instruction::MemoryGrow, u32
    }

    define_mem_operands! {
        visit_i32_load, I32Load,
        visit_i64_load, I64Load,
        visit_f32_load, F32Load,
        visit_f64_load, F64Load,
        visit_i32_load8_s, I32Load8S,
        visit_i32_load8_u, I32Load8U,
        visit_i32_load16_s, I32Load16S,
        visit_i32_load16_u, I32Load16U,
        visit_i64_load8_s, I64Load8S,
        visit_i64_load8_u, I64Load8U,
        visit_i64_load16_s, I64Load16S,
        visit_i64_load16_u, I64Load16U,
        visit_i64_load32_s, I64Load32S,
        visit_i64_load32_u, I64Load32U,
        // visit_i32_store, I32Store, custom implementation
        visit_i64_store, I64Store,
        visit_f32_store, F32Store,
        visit_f64_store, F64Store,
        visit_i32_store8, I32Store8,
        visit_i32_store16, I32Store16,
        visit_i64_store8, I64Store8,
        visit_i64_store16, I64Store16,
        visit_i64_store32, I64Store32
    }

    define_operands! {
        visit_unreachable, Instruction::Unreachable,
        visit_nop, Instruction::Nop,
        visit_return, Instruction::Return,
        visit_i32_eqz, Instruction::I32Eqz,
        visit_i32_eq, Instruction::I32Eq,
        visit_i32_ne, Instruction::I32Ne,
        visit_i32_lt_s, Instruction::I32LtS,
        visit_i32_lt_u, Instruction::I32LtU,
        visit_i32_gt_s, Instruction::I32GtS,
        visit_i32_gt_u, Instruction::I32GtU,
        visit_i32_le_s, Instruction::I32LeS,
        visit_i32_le_u, Instruction::I32LeU,
        visit_i32_ge_s, Instruction::I32GeS,
        visit_i32_ge_u, Instruction::I32GeU,
        visit_i64_eqz, Instruction::I64Eqz,
        visit_i64_eq, Instruction::I64Eq,
        visit_i64_ne, Instruction::I64Ne,
        visit_i64_lt_s, Instruction::I64LtS,
        visit_i64_lt_u, Instruction::I64LtU,
        visit_i64_gt_s, Instruction::I64GtS,
        visit_i64_gt_u, Instruction::I64GtU,
        visit_i64_le_s, Instruction::I64LeS,
        visit_i64_le_u, Instruction::I64LeU,
        visit_i64_ge_s, Instruction::I64GeS,
        visit_i64_ge_u, Instruction::I64GeU,
        visit_f32_eq, Instruction::F32Eq,
        visit_f32_ne, Instruction::F32Ne,
        visit_f32_lt, Instruction::F32Lt,
        visit_f32_gt, Instruction::F32Gt,
        visit_f32_le, Instruction::F32Le,
        visit_f32_ge, Instruction::F32Ge,
        visit_f64_eq, Instruction::F64Eq,
        visit_f64_ne, Instruction::F64Ne,
        visit_f64_lt, Instruction::F64Lt,
        visit_f64_gt, Instruction::F64Gt,
        visit_f64_le, Instruction::F64Le,
        visit_f64_ge, Instruction::F64Ge,
        visit_i32_clz, Instruction::I32Clz,
        visit_i32_ctz, Instruction::I32Ctz,
        visit_i32_popcnt, Instruction::I32Popcnt,
        // visit_i32_add, Instruction::I32Add, custom implementation
        visit_i32_sub, Instruction::I32Sub,
        visit_i32_mul, Instruction::I32Mul,
        visit_i32_div_s, Instruction::I32DivS,
        visit_i32_div_u, Instruction::I32DivU,
        visit_i32_rem_s, Instruction::I32RemS,
        visit_i32_rem_u, Instruction::I32RemU,
        visit_i32_and, Instruction::I32And,
        visit_i32_or, Instruction::I32Or,
        visit_i32_xor, Instruction::I32Xor,
        visit_i32_shl, Instruction::I32Shl,
        visit_i32_shr_s, Instruction::I32ShrS,
        visit_i32_shr_u, Instruction::I32ShrU,
        visit_i32_rotl, Instruction::I32Rotl,
        visit_i32_rotr, Instruction::I32Rotr,
        visit_i64_clz, Instruction::I64Clz,
        visit_i64_ctz, Instruction::I64Ctz,
        visit_i64_popcnt, Instruction::I64Popcnt,
        visit_i64_add, Instruction::I64Add,
        visit_i64_sub, Instruction::I64Sub,
        visit_i64_mul, Instruction::I64Mul,
        visit_i64_div_s, Instruction::I64DivS,
        visit_i64_div_u, Instruction::I64DivU,
        visit_i64_rem_s, Instruction::I64RemS,
        visit_i64_rem_u, Instruction::I64RemU,
        visit_i64_and, Instruction::I64And,
        visit_i64_or, Instruction::I64Or,
        visit_i64_xor, Instruction::I64Xor,
        visit_i64_shl, Instruction::I64Shl,
        visit_i64_shr_s, Instruction::I64ShrS,
        visit_i64_shr_u, Instruction::I64ShrU,
        // visit_i64_rotl, Instruction::I64Rotl, custom implementation
        visit_i64_rotr, Instruction::I64Rotr,
        visit_f32_abs, Instruction::F32Abs,
        visit_f32_neg, Instruction::F32Neg,
        visit_f32_ceil, Instruction::F32Ceil,
        visit_f32_floor, Instruction::F32Floor,
        visit_f32_trunc, Instruction::F32Trunc,
        visit_f32_nearest, Instruction::F32Nearest,
        visit_f32_sqrt, Instruction::F32Sqrt,
        visit_f32_add, Instruction::F32Add,
        visit_f32_sub, Instruction::F32Sub,
        visit_f32_mul, Instruction::F32Mul,
        visit_f32_div, Instruction::F32Div,
        visit_f32_min, Instruction::F32Min,
        visit_f32_max, Instruction::F32Max,
        visit_f32_copysign, Instruction::F32Copysign,
        visit_f64_abs, Instruction::F64Abs,
        visit_f64_neg, Instruction::F64Neg,
        visit_f64_ceil, Instruction::F64Ceil,
        visit_f64_floor, Instruction::F64Floor,
        visit_f64_trunc, Instruction::F64Trunc,
        visit_f64_nearest, Instruction::F64Nearest,
        visit_f64_sqrt, Instruction::F64Sqrt,
        visit_f64_add, Instruction::F64Add,
        visit_f64_sub, Instruction::F64Sub,
        visit_f64_mul, Instruction::F64Mul,
        visit_f64_div, Instruction::F64Div,
        visit_f64_min, Instruction::F64Min,
        visit_f64_max, Instruction::F64Max,
        visit_f64_copysign, Instruction::F64Copysign,
        visit_i32_wrap_i64, Instruction::I32WrapI64,
        visit_i32_trunc_f32_s, Instruction::I32TruncF32S,
        visit_i32_trunc_f32_u, Instruction::I32TruncF32U,
        visit_i32_trunc_f64_s, Instruction::I32TruncF64S,
        visit_i32_trunc_f64_u, Instruction::I32TruncF64U,
        visit_i64_extend_i32_s, Instruction::I64ExtendI32S,
        visit_i64_extend_i32_u, Instruction::I64ExtendI32U,
        visit_i64_trunc_f32_s, Instruction::I64TruncF32S,
        visit_i64_trunc_f32_u, Instruction::I64TruncF32U,
        visit_i64_trunc_f64_s, Instruction::I64TruncF64S,
        visit_i64_trunc_f64_u, Instruction::I64TruncF64U,
        visit_f32_convert_i32_s, Instruction::F32ConvertI32S,
        visit_f32_convert_i32_u, Instruction::F32ConvertI32U,
        visit_f32_convert_i64_s, Instruction::F32ConvertI64S,
        visit_f32_convert_i64_u, Instruction::F32ConvertI64U,
        visit_f32_demote_f64, Instruction::F32DemoteF64,
        visit_f64_convert_i32_s, Instruction::F64ConvertI32S,
        visit_f64_convert_i32_u, Instruction::F64ConvertI32U,
        visit_f64_convert_i64_s, Instruction::F64ConvertI64S,
        visit_f64_convert_i64_u, Instruction::F64ConvertI64U,
        visit_f64_promote_f32, Instruction::F64PromoteF32,
        visit_i32_reinterpret_f32, Instruction::I32ReinterpretF32,
        visit_i64_reinterpret_f64, Instruction::I64ReinterpretF64,
        visit_f32_reinterpret_i32, Instruction::F32ReinterpretI32,
        visit_f64_reinterpret_i64, Instruction::F64ReinterpretI64,

        // sign_extension
        visit_i32_extend8_s, Instruction::I32Extend8S,
        visit_i32_extend16_s, Instruction::I32Extend16S,
        visit_i64_extend8_s, Instruction::I64Extend8S,
        visit_i64_extend16_s, Instruction::I64Extend16S,
        visit_i64_extend32_s, Instruction::I64Extend32S,

        // Non-trapping Float-to-int Conversions
        visit_i32_trunc_sat_f32_s, Instruction::I32TruncSatF32S,
        visit_i32_trunc_sat_f32_u, Instruction::I32TruncSatF32U,
        visit_i32_trunc_sat_f64_s, Instruction::I32TruncSatF64S,
        visit_i32_trunc_sat_f64_u, Instruction::I32TruncSatF64U,
        visit_i64_trunc_sat_f32_s, Instruction::I64TruncSatF32S,
        visit_i64_trunc_sat_f32_u, Instruction::I64TruncSatF32U,
        visit_i64_trunc_sat_f64_s, Instruction::I64TruncSatF64S,
        visit_i64_trunc_sat_f64_u, Instruction::I64TruncSatF64U
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
        self.instructions.push(i32store)
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

    fn visit_i64_rotl(&mut self) -> Self::Output {
        self.instructions.push(Instruction::I64Rotl)
    }

    fn visit_i32_add(&mut self) -> Self::Output {
        self.instructions.push(Instruction::I32Add)
    }

    fn visit_block(&mut self, blockty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.instructions.push(match blockty {
            wasmparser::BlockType::Empty => Instruction::Block(0),
            wasmparser::BlockType::FuncType(idx) => Instruction::BlockWithFuncType(idx, 0),
            wasmparser::BlockType::Type(ty) => Instruction::BlockWithType(convert_valtype(&ty), 0),
        })
    }

    fn visit_loop(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.instructions.push(match ty {
            wasmparser::BlockType::Empty => Instruction::Loop(0),
            wasmparser::BlockType::FuncType(idx) => Instruction::LoopWithFuncType(idx, 0),
            wasmparser::BlockType::Type(ty) => Instruction::LoopWithType(convert_valtype(&ty), 0),
        })
    }

    fn visit_if(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.instructions.push(match ty {
            wasmparser::BlockType::Empty => Instruction::If(0, 0),
            wasmparser::BlockType::FuncType(idx) => Instruction::IfWithFuncType(idx, 0, 0),
            wasmparser::BlockType::Type(ty) => Instruction::IfWithType(convert_valtype(&ty), 0, 0),
        })
    }

    fn visit_else(&mut self) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.instructions.push(Instruction::Else(0))
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
            Some(Instruction::Block(end_offset))
            | Some(Instruction::BlockWithType(_, end_offset))
            | Some(Instruction::BlockWithFuncType(_, end_offset))
            | Some(Instruction::Loop(end_offset))
            | Some(Instruction::LoopWithFuncType(_, end_offset))
            | Some(Instruction::LoopWithType(_, end_offset))
            | Some(Instruction::If(_, end_offset))
            | Some(Instruction::IfWithFuncType(_, _, end_offset))
            | Some(Instruction::IfWithType(_, _, end_offset)) => {
                *end_offset = (current_instr_ptr - label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support  blocks that large");
            }
            _ => {
                unreachable!("Expected to end a block, but the last label was not a block")
            }
        };

        self.instructions.push(Instruction::EndBlockFrame)
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
        self.instructions.push(Instruction::CallIndirect(ty, table))
    }

    fn visit_f32_const(&mut self, val: wasmparser::Ieee32) -> Self::Output {
        self.instructions.push(Instruction::F32Const(f32::from_bits(val.bits())))
    }

    fn visit_f64_const(&mut self, val: wasmparser::Ieee64) -> Self::Output {
        self.instructions.push(Instruction::F64Const(f64::from_bits(val.bits())))
    }

    // Bulk Memory Operations

    define_primitive_operands! {
        visit_memory_init, Instruction::MemoryInit, u32, u32,
        visit_memory_copy, Instruction::MemoryCopy, u32, u32,
        visit_table_init, Instruction::TableInit, u32, u32
    }
    define_primitive_operands! {
        visit_memory_fill, Instruction::MemoryFill, u32,
        visit_data_drop, Instruction::DataDrop, u32,
        visit_elem_drop, Instruction::ElemDrop, u32
    }

    fn visit_table_copy(&mut self, dst_table: u32, src_table: u32) -> Self::Output {
        self.instructions.push(Instruction::TableCopy { from: src_table, to: dst_table })
    }

    // Reference Types
    fn visit_ref_null(&mut self, ty: wasmparser::HeapType) -> Self::Output {
        self.instructions.push(Instruction::RefNull(convert_heaptype(ty)))
    }

    fn visit_ref_is_null(&mut self) -> Self::Output {
        self.instructions.push(Instruction::RefIsNull)
    }

    fn visit_typed_select(&mut self, ty: wasmparser::ValType) -> Self::Output {
        self.instructions.push(match ty {
            wasmparser::ValType::I32 => Instruction::Select32,
            wasmparser::ValType::F32 => Instruction::Select32,
            wasmparser::ValType::I64 => Instruction::Select64,
            wasmparser::ValType::F64 => Instruction::Select64,
            wasmparser::ValType::V128 => Instruction::Select128,
            wasmparser::ValType::Ref(_) => Instruction::SelectRef,
        })
    }

    define_primitive_operands! {
        visit_ref_func, Instruction::RefFunc, u32,
        visit_table_fill, Instruction::TableFill, u32,
        visit_table_get, Instruction::TableGet, u32,
        visit_table_set, Instruction::TableSet, u32,
        visit_table_grow, Instruction::TableGrow, u32,
        visit_table_size, Instruction::TableSize, u32
    }
}
