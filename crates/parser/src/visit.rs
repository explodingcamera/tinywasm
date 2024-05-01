use crate::{conversion::convert_blocktype, Result};

use crate::conversion::{convert_heaptype, convert_memarg, convert_valtype};
use alloc::string::ToString;
use alloc::{boxed::Box, format, vec::Vec};
use tinywasm_types::{BlockArgsPacked, Instruction};
use wasmparser::{FuncValidator, FunctionBody, VisitOperator, WasmModuleResources};

struct ValidateThenVisit<'a, T, U>(T, &'a mut U);
macro_rules! validate_then_visit {
    ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident)*) => {
        $(
            fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
                self.0.$visit($($($arg.clone()),*)?)?;
                Ok(self.1.$visit($($($arg),*)?))
            }
        )*
    };
}

impl<'a, T, U> VisitOperator<'a> for ValidateThenVisit<'_, T, U>
where
    T: VisitOperator<'a, Output = wasmparser::Result<()>>,
    U: VisitOperator<'a>,
{
    type Output = Result<U::Output>;
    wasmparser::for_each_operator!(validate_then_visit);
}

pub(crate) fn process_operators<R: WasmModuleResources>(
    validator: Option<&mut FuncValidator<R>>,
    body: &FunctionBody<'_>,
) -> Result<Box<[Instruction]>> {
    let mut reader = body.get_operators_reader()?;
    let remaining = reader.get_binary_reader().bytes_remaining();
    let mut builder = FunctionBuilder::new(remaining);

    if let Some(validator) = validator {
        while !reader.eof() {
            let validate = validator.visitor(reader.original_position());
            reader.visit_operator(&mut ValidateThenVisit(validate, &mut builder))???;
        }
        validator.finish(reader.original_position())?;
    } else {
        while !reader.eof() {
            reader.visit_operator(&mut builder)??;
        }
    }

    Ok(builder.instructions.into_boxed_slice())
}

macro_rules! define_operands {
    ($($name:ident, $instr:expr),*) => {
        $(
            fn $name(&mut self) -> Self::Output {
                self.instructions.push($instr);
                Ok(())
            }
        )*
    };
}

macro_rules! define_primitive_operands {
    ($($name:ident, $instr:expr, $ty:ty),*) => {
        $(
            fn $name(&mut self, arg: $ty) -> Self::Output {
                self.instructions.push($instr(arg));
                Ok(())
            }
        )*
    };
    ($($name:ident, $instr:expr, $ty:ty, $ty2:ty),*) => {
        $(
            fn $name(&mut self, arg: $ty, arg2: $ty) -> Self::Output {
                self.instructions.push($instr(arg, arg2));
                Ok(())
            }
        )*
    };
}

macro_rules! define_mem_operands {
    ($($name:ident, $instr:ident),*) => {
        $(
            fn $name(&mut self, mem_arg: wasmparser::MemArg) -> Self::Output {
                let arg = convert_memarg(mem_arg);
                self.instructions.push(Instruction::$instr {
                    offset: arg.offset,
                    mem_addr: arg.mem_addr,
                });
                Ok(())
            }
        )*
    };
}

pub(crate) struct FunctionBuilder {
    instructions: Vec<Instruction>,
    label_ptrs: Vec<usize>,
}

impl FunctionBuilder {
    pub(crate) fn new(instr_capacity: usize) -> Self {
        Self { instructions: Vec::with_capacity(instr_capacity), label_ptrs: Vec::with_capacity(256) }
    }

    #[cold]
    fn unsupported(&self, name: &str) -> Result<()> {
        Err(crate::ParseError::UnsupportedOperator(format!("Unsupported instruction: {:?}", name)))
    }

    #[inline]
    fn visit(&mut self, op: Instruction) -> Result<()> {
        self.instructions.push(op);
        Ok(())
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
        fn $visit(&mut self $($(,$arg: $argty)*)?) -> Result<()>{
            self.unsupported(stringify!($visit))
        }
    };
}

impl<'a> wasmparser::VisitOperator<'a> for FunctionBuilder {
    type Output = Result<()>;
    wasmparser::for_each_operator!(impl_visit_operator);

    define_primitive_operands! {
        visit_br, Instruction::Br, u32,
        visit_br_if, Instruction::BrIf, u32,
        visit_global_get, Instruction::GlobalGet, u32,
        visit_global_set, Instruction::GlobalSet, u32,
        visit_i32_const, Instruction::I32Const, i32,
        visit_i64_const, Instruction::I64Const, i64
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
        visit_i32_store, I32Store,
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
        visit_drop, Instruction::Drop,
        visit_select, Instruction::Select(None),
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

    fn visit_local_get(&mut self, idx: u32) -> Self::Output {
        if let Some(instruction) = self.instructions.last_mut() {
            match instruction {
                Instruction::LocalGet(a) => *instruction = Instruction::LocalGet2(*a, idx),
                Instruction::LocalGet2(a, b) => *instruction = Instruction::LocalGet3(*a, *b, idx),
                Instruction::LocalTee(a) => *instruction = Instruction::LocalTeeGet(*a, idx),
                _ => return self.visit(Instruction::LocalGet(idx)),
            };
            Ok(())
        } else {
            self.visit(Instruction::LocalGet(idx))
        }
    }

    fn visit_local_set(&mut self, idx: u32) -> Self::Output {
        self.visit(Instruction::LocalSet(idx))
        // if let Some(instruction) = self.instructions.last_mut() {
        //     match instruction {
        //         // Needs more testing, seems to make performance worse
        //         // Instruction::LocalGet(a) => *instruction = Instruction::LocalGetSet(*a, idx),
        //         _ => return self.visit(Instruction::LocalSet(idx)),
        //     };
        //     // Ok(())
        // } else {
        //     self.visit(Instruction::LocalSet(idx))
        // }
    }

    fn visit_local_tee(&mut self, idx: u32) -> Self::Output {
        self.visit(Instruction::LocalTee(idx))
    }

    fn visit_i64_rotl(&mut self) -> Self::Output {
        if self.instructions.len() < 2 {
            return self.visit(Instruction::I64Rotl);
        }

        match self.instructions[self.instructions.len() - 2..] {
            [Instruction::I64Xor, Instruction::I64Const(a)] => {
                self.instructions.pop();
                self.instructions.pop();
                self.visit(Instruction::I64XorConstRotl(a))
            }
            _ => self.visit(Instruction::I64Rotl),
        }
    }

    fn visit_i32_add(&mut self) -> Self::Output {
        self.visit(Instruction::I32Add)
        // if self.instructions.len() < 2 {
        //     return self.visit(Instruction::I32Add);
        // }

        // match self.instructions[self.instructions.len() - 2..] {
        //     // [Instruction::LocalGet(a), Instruction::I32Const(b)] => {
        //     //     self.instructions.pop();
        //     //     self.instructions.pop();
        //     //     self.visit(Instruction::I32LocalGetConstAdd(a, b))
        //     // }
        //     _ => self.visit(Instruction::I32Add),
        // }
    }

    fn visit_block(&mut self, blockty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.visit(Instruction::Block(convert_blocktype(blockty), 0))
    }

    fn visit_loop(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.visit(Instruction::Loop(convert_blocktype(ty), 0))
    }

    fn visit_if(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.visit(Instruction::If(BlockArgsPacked::new(convert_blocktype(ty)), 0, 0))
    }

    fn visit_else(&mut self) -> Self::Output {
        self.label_ptrs.push(self.instructions.len());
        self.visit(Instruction::Else(0))
    }

    fn visit_end(&mut self) -> Self::Output {
        let Some(label_pointer) = self.label_ptrs.pop() else {
            return self.visit(Instruction::Return);
        };

        let current_instr_ptr = self.instructions.len();

        match self.instructions[label_pointer] {
            Instruction::Else(ref mut else_instr_end_offset) => {
                *else_instr_end_offset = (current_instr_ptr - label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support if blocks that large");

                #[cold]
                fn error() -> crate::ParseError {
                    crate::ParseError::UnsupportedOperator(
                        "Expected to end an if block, but the last label was not an if".to_string(),
                    )
                }

                // since we're ending an else block, we need to end the if block as well
                let if_label_pointer = self.label_ptrs.pop().ok_or_else(error)?;

                let if_instruction = &mut self.instructions[if_label_pointer];
                let Instruction::If(_, ref mut else_offset, ref mut end_offset) = if_instruction else {
                    return Err(error());
                };

                *else_offset = (label_pointer - if_label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support blocks that large");

                *end_offset = (current_instr_ptr - if_label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support blocks that large");
            }
            Instruction::Block(_, ref mut end_offset)
            | Instruction::Loop(_, ref mut end_offset)
            | Instruction::If(_, _, ref mut end_offset) => {
                *end_offset = (current_instr_ptr - label_pointer)
                    .try_into()
                    .expect("else_instr_end_offset is too large, tinywasm does not support  blocks that large");
            }
            _ => {
                return Err(crate::ParseError::UnsupportedOperator(
                    "Expected to end a block, but the last label was not a block".to_string(),
                ))
            }
        };

        self.visit(Instruction::EndBlockFrame)
    }

    fn visit_br_table(&mut self, targets: wasmparser::BrTable<'_>) -> Self::Output {
        let def = targets.default();
        let instrs = targets
            .targets()
            .map(|t| t.map(Instruction::BrLabel))
            .collect::<Result<Vec<Instruction>, wasmparser::BinaryReaderError>>()
            .expect("BrTable targets are invalid, this should have been caught by the validator");

        self.instructions
            .extend(IntoIterator::into_iter([Instruction::BrTable(def, instrs.len() as u32)]).chain(instrs));

        Ok(())
    }

    fn visit_call(&mut self, idx: u32) -> Self::Output {
        self.visit(Instruction::Call(idx))
    }

    fn visit_call_indirect(&mut self, ty: u32, table: u32, _table_byte: u8) -> Self::Output {
        self.visit(Instruction::CallIndirect(ty, table))
    }

    fn visit_memory_size(&mut self, mem: u32, mem_byte: u8) -> Self::Output {
        self.visit(Instruction::MemorySize(mem, mem_byte))
    }

    fn visit_memory_grow(&mut self, mem: u32, mem_byte: u8) -> Self::Output {
        self.visit(Instruction::MemoryGrow(mem, mem_byte))
    }

    fn visit_f32_const(&mut self, val: wasmparser::Ieee32) -> Self::Output {
        self.visit(Instruction::F32Const(f32::from_bits(val.bits())))
    }

    fn visit_f64_const(&mut self, val: wasmparser::Ieee64) -> Self::Output {
        self.visit(Instruction::F64Const(f64::from_bits(val.bits())))
    }

    // Bulk Memory Operations

    define_primitive_operands! {
        visit_memory_init, Instruction::MemoryInit, u32, u32,
        visit_memory_copy, Instruction::MemoryCopy, u32, u32,
        visit_table_init, Instruction::TableInit, u32, u32
    }
    define_primitive_operands! {
        visit_memory_fill, Instruction::MemoryFill, u32,
        visit_data_drop, Instruction::DataDrop, u32
    }

    fn visit_elem_drop(&mut self, _elem_index: u32) -> Self::Output {
        self.unsupported("elem_drop")
    }

    fn visit_table_copy(&mut self, dst_table: u32, src_table: u32) -> Self::Output {
        self.visit(Instruction::TableCopy { from: src_table, to: dst_table })
    }

    // Reference Types

    fn visit_ref_null(&mut self, ty: wasmparser::HeapType) -> Self::Output {
        self.visit(Instruction::RefNull(convert_heaptype(ty)))
    }

    fn visit_ref_is_null(&mut self) -> Self::Output {
        self.visit(Instruction::RefIsNull)
    }

    fn visit_typed_select(&mut self, ty: wasmparser::ValType) -> Self::Output {
        self.visit(Instruction::Select(Some(convert_valtype(&ty))))
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
