#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use super::no_std_floats::NoStdFloatExt;

use alloc::{format, rc::Rc, string::ToString};
use core::ops::{ControlFlow, IndexMut, Shl, Shr};

use interpreter::stack::CallFrame;
use tinywasm_types::*;

#[cfg(feature = "simd")]
mod simd {
    #[cfg(feature = "std")]
    pub(super) use crate::std::simd::StdFloat;
    pub(super) use core::simd::cmp::{SimdOrd, SimdPartialEq, SimdPartialOrd};
    pub(super) use core::simd::num::{SimdFloat, SimdInt, SimdUint};
    pub(super) use core::simd::*;
}
#[cfg(feature = "simd")]
use simd::*;

use super::num_helpers::*;
use super::stack::{BlockFrame, BlockType, Stack};
use super::values::*;
use crate::*;

pub(crate) struct Executor<'store, 'stack> {
    pub(crate) cf: CallFrame,
    pub(crate) module: ModuleInstance,
    pub(crate) store: &'store mut Store,
    pub(crate) stack: &'stack mut Stack,
}

impl<'store, 'stack> Executor<'store, 'stack> {
    pub(crate) fn new(store: &'store mut Store, stack: &'stack mut Stack) -> Result<Self> {
        let current_frame = stack.call_stack.pop().expect("no call frame, this is a bug");
        let current_module = store.get_module_instance_raw(current_frame.module_addr());
        Ok(Self { cf: current_frame, module: current_module, stack, store })
    }

    #[inline(always)]
    pub(crate) fn run_to_completion(&mut self) -> Result<()> {
        loop {
            if let ControlFlow::Break(res) = self.exec_next() {
                return match res {
                    Some(e) => Err(e),
                    None => Ok(()),
                };
            }
        }
    }

    #[inline(always)]
    fn exec_next(&mut self) -> ControlFlow<Option<Error>> {
        use tinywasm_types::Instruction::*;

        #[rustfmt::skip]
        match self.cf.fetch_instr() {
            Nop | BrLabel(_) | I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64 => {}
            Unreachable => self.exec_unreachable()?,

            Drop32 => self.stack.values.drop::<Value32>(),
            Drop64 => self.stack.values.drop::<Value64>(),
            Drop128 => self.stack.values.drop::<Value128>(),
            DropRef => self.stack.values.drop::<ValueRef>(),

            Select32 => self.stack.values.select::<Value32>(),
            Select64 => self.stack.values.select::<Value64>(),
            Select128 => self.stack.values.select::<Value128>(),
            SelectRef => self.stack.values.select::<ValueRef>(),

            Call(v) => return self.exec_call_direct::<false>(*v),
            CallIndirect(ty, table) => return self.exec_call_indirect::<false>(*ty, *table),

            ReturnCall(v) => return self.exec_call_direct::<true>(*v),
            ReturnCallIndirect(ty, table) => return self.exec_call_indirect::<true>(*ty, *table),

            If(end, el) => self.exec_if(*end, *el, (StackHeight::default(), StackHeight::default())),
            IfWithType(ty, end, el) => self.exec_if(*end, *el, (StackHeight::default(), (*ty).into())),
            IfWithFuncType(ty, end, el) => self.exec_if(*end, *el, self.resolve_functype(*ty)),
            Else(end_offset) => self.exec_else(*end_offset),
            Loop(end) => self.enter_block(*end, BlockType::Loop, (StackHeight::default(), StackHeight::default())),
            LoopWithType(ty, end) => self.enter_block(*end, BlockType::Loop, (StackHeight::default(), (*ty).into())),
            LoopWithFuncType(ty, end) => self.enter_block(*end, BlockType::Loop, self.resolve_functype(*ty)),
            Block(end) => self.enter_block(*end, BlockType::Block, (StackHeight::default(), StackHeight::default())),
            BlockWithType(ty, end) => self.enter_block(*end, BlockType::Block, (StackHeight::default(), (*ty).into())),
            BlockWithFuncType(ty, end) => self.enter_block(*end, BlockType::Block, self.resolve_functype(*ty)),
            Br(v) => return self.exec_br(*v),
            BrIf(v) => return self.exec_br_if(*v),
            BrTable(default, len) => return self.exec_brtable(*default, *len),
            Return => return self.exec_return(),
            EndBlockFrame => self.exec_end_block(),

            LocalGet32(local_index) => self.exec_local_get::<Value32>(*local_index),
            LocalGet64(local_index) => self.exec_local_get::<Value64>(*local_index),
            LocalGet128(local_index) => self.exec_local_get::<Value128>(*local_index),
            LocalGetRef(local_index) => self.exec_local_get::<ValueRef>(*local_index),

            LocalSet32(local_index) => self.exec_local_set::<Value32>(*local_index),
            LocalSet64(local_index) => self.exec_local_set::<Value64>(*local_index),
            LocalSet128(local_index) => self.exec_local_set::<Value128>(*local_index),
            LocalSetRef(local_index) => self.exec_local_set::<ValueRef>(*local_index),

            LocalTee32(local_index) => self.exec_local_tee::<Value32>(*local_index),
            LocalTee64(local_index) => self.exec_local_tee::<Value64>(*local_index),
            LocalTee128(local_index) => self.exec_local_tee::<Value128>(*local_index),
            LocalTeeRef(local_index) => self.exec_local_tee::<ValueRef>(*local_index),

            GlobalGet(global_index) => self.exec_global_get(*global_index),
            GlobalSet32(global_index) => self.exec_global_set::<Value32>(*global_index),
            GlobalSet64(global_index) => self.exec_global_set::<Value64>(*global_index),
            GlobalSet128(global_index) => self.exec_global_set::<Value128>(*global_index),
            GlobalSetRef(global_index) => self.exec_global_set::<ValueRef>(*global_index),

            I32Const(val) => self.exec_const(*val),
            I64Const(val) => self.exec_const(*val),
            F32Const(val) => self.exec_const(*val),
            F64Const(val) => self.exec_const(*val),
            RefFunc(func_idx) => self.exec_const::<ValueRef>(Some(*func_idx)),
            RefNull(_) => self.exec_const::<ValueRef>(None),
            RefIsNull => self.exec_ref_is_null(),

            MemorySize(addr) => self.exec_memory_size(*addr),
            MemoryGrow(addr) => self.exec_memory_grow(*addr),

            // Bulk memory operations
            MemoryCopy(from, to) => self.exec_memory_copy(*from, *to).to_cf()?,
            MemoryFill(addr) => self.exec_memory_fill(*addr).to_cf()?,
            MemoryInit(data_idx, mem_idx) => self.exec_memory_init(*data_idx, *mem_idx).to_cf()?,
            DataDrop(data_index) => self.exec_data_drop(*data_index),
            ElemDrop(elem_index) => self.exec_elem_drop(*elem_index),
            TableCopy { from, to } => self.exec_table_copy(*from, *to).to_cf()?,

            I32Store(m) => self.exec_mem_store::<i32, i32, 4>(m.mem_addr(), m.offset(), |v| v)?,
            I64Store(m) => self.exec_mem_store::<i64, i64, 8>(m.mem_addr(), m.offset(), |v| v)?,
            F32Store(m) => self.exec_mem_store::<f32, f32, 4>(m.mem_addr(), m.offset(), |v| v)?,
            F64Store(m) => self.exec_mem_store::<f64, f64, 8>(m.mem_addr(), m.offset(), |v| v)?,
            I32Store8(m) => self.exec_mem_store::<i32, i8, 1>(m.mem_addr(), m.offset(), |v| v as i8)?,
            I32Store16(m) => self.exec_mem_store::<i32, i16, 2>(m.mem_addr(), m.offset(), |v| v as i16)?,
            I64Store8(m) => self.exec_mem_store::<i64, i8, 1>(m.mem_addr(), m.offset(), |v| v as i8)?,
            I64Store16(m) => self.exec_mem_store::<i64, i16, 2>(m.mem_addr(), m.offset(), |v| v as i16)?,
            I64Store32(m) => self.exec_mem_store::<i64, i32, 4>(m.mem_addr(), m.offset(), |v| v as i32)?,

            I32Load(m) => self.exec_mem_load::<i32, 4, _>(m.mem_addr(), m.offset(), |v| v)?,
            I64Load(m) => self.exec_mem_load::<i64, 8, _>(m.mem_addr(), m.offset(), |v| v)?,
            F32Load(m) => self.exec_mem_load::<f32, 4, _>(m.mem_addr(), m.offset(), |v| v)?,
            F64Load(m) => self.exec_mem_load::<f64, 8, _>(m.mem_addr(), m.offset(), |v| v)?,
            I32Load8S(m) => self.exec_mem_load::<i8, 1, _>(m.mem_addr(), m.offset(), |v| v as i32)?,
            I32Load8U(m) => self.exec_mem_load::<u8, 1, _>(m.mem_addr(), m.offset(), |v| v as i32)?,
            I32Load16S(m) => self.exec_mem_load::<i16, 2, _>(m.mem_addr(), m.offset(), |v| v as i32)?,
            I32Load16U(m) => self.exec_mem_load::<u16, 2, _>(m.mem_addr(), m.offset(), |v| v as i32)?,
            I64Load8S(m) => self.exec_mem_load::<i8, 1, _>(m.mem_addr(), m.offset(), |v| v as i64)?,
            I64Load8U(m) => self.exec_mem_load::<u8, 1, _>(m.mem_addr(), m.offset(), |v| v as i64)?,
            I64Load16S(m) => self.exec_mem_load::<i16, 2, _>(m.mem_addr(), m.offset(), |v| v as i64)?,
            I64Load16U(m) => self.exec_mem_load::<u16, 2, _>(m.mem_addr(), m.offset(), |v| v as i64)?,
            I64Load32S(m) => self.exec_mem_load::<i32, 4, _>(m.mem_addr(), m.offset(), |v| v as i64)?,
            I64Load32U(m) => self.exec_mem_load::<u32, 4, _>(m.mem_addr(), m.offset(), |v| v as i64)?,

            I64Eqz => self.stack.values.replace_top::<i64, _>(|v| Ok(i32::from(v == 0))).to_cf()?,
            I32Eqz => self.stack.values.replace_top_same::<i32>(|v| Ok(i32::from(v == 0))).to_cf()?,
            I32Eq => self.stack.values.calculate_same::<i32>(|a, b| Ok(i32::from(a == b))).to_cf()?,
            I64Eq => self.stack.values.calculate::<i64, _>(|a, b| Ok(i32::from(a == b))).to_cf()?,
            F32Eq => self.stack.values.calculate::<f32, _>(|a, b| Ok(i32::from(a == b))).to_cf()?,
            F64Eq => self.stack.values.calculate::<f64, _>(|a, b| Ok(i32::from(a == b))).to_cf()?,

            I32Ne => self.stack.values.calculate_same::<i32>(|a, b| Ok(i32::from(a != b))).to_cf()?,
            I64Ne => self.stack.values.calculate::<i64, _>(|a, b| Ok(i32::from(a != b))).to_cf()?,
            F32Ne => self.stack.values.calculate::<f32, _>(|a, b| Ok(i32::from(a != b))).to_cf()?,
            F64Ne => self.stack.values.calculate::<f64, _>(|a, b| Ok(i32::from(a != b))).to_cf()?,

            I32LtS => self.stack.values.calculate_same::<i32>(|a, b| Ok(i32::from(a < b))).to_cf()?,
            I64LtS => self.stack.values.calculate::<i64, _>(|a, b| Ok(i32::from(a < b))).to_cf()?,
            I32LtU => self.stack.values.calculate::<u32, _>(|a, b| Ok(i32::from(a < b))).to_cf()?,
            I64LtU => self.stack.values.calculate::<u64, _>(|a, b| Ok(i32::from(a < b))).to_cf()?,
            F32Lt => self.stack.values.calculate::<f32, _>(|a, b| Ok(i32::from(a < b))).to_cf()?,
            F64Lt => self.stack.values.calculate::<f64, _>(|a, b| Ok(i32::from(a < b))).to_cf()?,

            I32LeS => self.stack.values.calculate_same::<i32>(|a, b| Ok(i32::from(a <= b))).to_cf()?,
            I64LeS => self.stack.values.calculate::<i64, _>(|a, b| Ok(i32::from(a <= b))).to_cf()?,
            I32LeU => self.stack.values.calculate::<u32, _>(|a, b| Ok(i32::from(a <= b))).to_cf()?,
            I64LeU => self.stack.values.calculate::<u64, _>(|a, b| Ok(i32::from(a <= b))).to_cf()?,
            F32Le => self.stack.values.calculate::<f32, _>(|a, b| Ok(i32::from(a <= b))).to_cf()?,
            F64Le => self.stack.values.calculate::<f64, _>(|a, b| Ok(i32::from(a <= b))).to_cf()?,

            I32GeS => self.stack.values.calculate_same::<i32>(|a, b| Ok(i32::from(a >= b))).to_cf()?,
            I64GeS => self.stack.values.calculate::<i64, _>(|a, b| Ok(i32::from(a >= b))).to_cf()?,
            I32GeU => self.stack.values.calculate::<u32, _>(|a, b| Ok(i32::from(a >= b))).to_cf()?,
            I64GeU => self.stack.values.calculate::<u64, _>(|a, b| Ok(i32::from(a >= b))).to_cf()?,
            F32Ge => self.stack.values.calculate::<f32, _>(|a, b| Ok(i32::from(a >= b))).to_cf()?,
            F64Ge => self.stack.values.calculate::<f64, _>(|a, b| Ok(i32::from(a >= b))).to_cf()?,

            I32GtS => self.stack.values.calculate_same::<i32>(|a, b| Ok(i32::from(a > b))).to_cf()?,
            I64GtS => self.stack.values.calculate::<i64, _>(|a, b| Ok(i32::from(a > b))).to_cf()?,
            I32GtU => self.stack.values.calculate::<u32, _>(|a, b| Ok(i32::from(a > b))).to_cf()?,
            I64GtU => self.stack.values.calculate::<u64, _>(|a, b| Ok(i32::from(a > b))).to_cf()?,
            F32Gt => self.stack.values.calculate::<f32, _>(|a, b| Ok(i32::from(a > b))).to_cf()?,
            F64Gt => self.stack.values.calculate::<f64, _>(|a, b| Ok(i32::from(a > b))).to_cf()?,

            I32Add => self.stack.values.calculate_same::<i32>(|a, b| Ok(a.wrapping_add(b))).to_cf()?,
            I64Add => self.stack.values.calculate_same::<i64>(|a, b| Ok(a.wrapping_add(b))).to_cf()?,
            F32Add => self.stack.values.calculate_same::<f32>(|a, b| Ok(a + b)).to_cf()?,
            F64Add => self.stack.values.calculate_same::<f64>(|a, b| Ok(a + b)).to_cf()?,

            I32Sub => self.stack.values.calculate_same::<i32>(|a, b| Ok(a.wrapping_sub(b))).to_cf()?,
            I64Sub => self.stack.values.calculate_same::<i64>(|a, b| Ok(a.wrapping_sub(b))).to_cf()?,
            F32Sub => self.stack.values.calculate_same::<f32>(|a, b| Ok(a - b)).to_cf()?,
            F64Sub => self.stack.values.calculate_same::<f64>(|a, b| Ok(a - b)).to_cf()?,

            F32Div => self.stack.values.calculate_same::<f32>(|a, b| Ok(a / b)).to_cf()?,
            F64Div => self.stack.values.calculate_same::<f64>(|a, b| Ok(a / b)).to_cf()?,

            I32Mul => self.stack.values.calculate_same::<i32>(|a, b| Ok(a.wrapping_mul(b))).to_cf()?,
            I64Mul => self.stack.values.calculate_same::<i64>(|a, b| Ok(a.wrapping_mul(b))).to_cf()?,
            F32Mul => self.stack.values.calculate_same::<f32>(|a, b| Ok(a * b)).to_cf()?,
            F64Mul => self.stack.values.calculate_same::<f64>(|a, b| Ok(a * b)).to_cf()?,

            I32DivS => self.stack.values.calculate_same::<i32>(|a, b| a.wasm_checked_div(b)).to_cf()?,
            I64DivS => self.stack.values.calculate_same::<i64>(|a, b| a.wasm_checked_div(b)).to_cf()?,
            I32DivU => self.stack.values.calculate_same::<u32>(|a, b| a.checked_div(b).ok_or_else(trap_0)).to_cf()?,
            I64DivU => self.stack.values.calculate_same::<u64>(|a, b| a.checked_div(b).ok_or_else(trap_0)).to_cf()?,
            I32RemS => self.stack.values.calculate_same::<i32>(|a, b| a.checked_wrapping_rem(b)).to_cf()?,
            I64RemS => self.stack.values.calculate_same::<i64>(|a, b| a.checked_wrapping_rem(b)).to_cf()?,
            I32RemU => self.stack.values.calculate_same::<u32>(|a, b| a.checked_wrapping_rem(b)).to_cf()?,
            I64RemU => self.stack.values.calculate_same::<u64>(|a, b| a.checked_wrapping_rem(b)).to_cf()?,

            I32And => self.stack.values.calculate_same::<i32>(|a, b| Ok(a & b)).to_cf()?,
            I64And => self.stack.values.calculate_same::<i64>(|a, b| Ok(a & b)).to_cf()?,
            I32Or => self.stack.values.calculate_same::<i32>(|a, b| Ok(a | b)).to_cf()?,
            I64Or => self.stack.values.calculate_same::<i64>(|a, b| Ok(a | b)).to_cf()?,
            I32Xor => self.stack.values.calculate_same::<i32>(|a, b| Ok(a ^ b)).to_cf()?,
            I64Xor => self.stack.values.calculate_same::<i64>(|a, b| Ok(a ^ b)).to_cf()?,
            I32Shl => self.stack.values.calculate_same::<i32>(|a, b| Ok(a.wasm_shl(b))).to_cf()?,
            I64Shl => self.stack.values.calculate_same::<i64>(|a, b| Ok(a.wasm_shl(b))).to_cf()?,
            I32ShrS => self.stack.values.calculate_same::<i32>(|a, b| Ok(a.wasm_shr(b))).to_cf()?,
            I64ShrS => self.stack.values.calculate_same::<i64>(|a, b| Ok(a.wasm_shr(b))).to_cf()?,
            I32ShrU => self.stack.values.calculate_same::<u32>(|a, b| Ok(a.wasm_shr(b))).to_cf()?,
            I64ShrU => self.stack.values.calculate_same::<u64>(|a, b| Ok(a.wasm_shr(b))).to_cf()?,
            I32Rotl => self.stack.values.calculate_same::<i32>(|a, b| Ok(a.wasm_rotl(b))).to_cf()?,
            I64Rotl => self.stack.values.calculate_same::<i64>(|a, b| Ok(a.wasm_rotl(b))).to_cf()?,
            I32Rotr => self.stack.values.calculate_same::<i32>(|a, b| Ok(a.wasm_rotr(b))).to_cf()?,
            I64Rotr => self.stack.values.calculate_same::<i64>(|a, b| Ok(a.wasm_rotr(b))).to_cf()?,

            I32Clz => self.stack.values.replace_top_same::<i32>(|v| Ok(v.leading_zeros() as i32)).to_cf()?,
            I64Clz => self.stack.values.replace_top_same::<i64>(|v| Ok(v.leading_zeros() as i64)).to_cf()?,
            I32Ctz => self.stack.values.replace_top_same::<i32>(|v| Ok(v.trailing_zeros() as i32)).to_cf()?,
            I64Ctz => self.stack.values.replace_top_same::<i64>(|v| Ok(v.trailing_zeros() as i64)).to_cf()?,
            I32Popcnt => self.stack.values.replace_top_same::<i32>(|v| Ok(v.count_ones() as i32)).to_cf()?,
            I64Popcnt => self.stack.values.replace_top_same::<i64>(|v| Ok(v.count_ones() as i64)).to_cf()?,

            F32ConvertI32S => self.stack.values.replace_top::<i32, _>(|v| Ok(v as f32)).to_cf()?,
            F32ConvertI64S => self.stack.values.replace_top::<i64, _>(|v| Ok(v as f32)).to_cf()?,
            F64ConvertI32S => self.stack.values.replace_top::<i32, _>(|v| Ok(v as f64)).to_cf()?,
            F64ConvertI64S => self.stack.values.replace_top::<i64, _>(|v| Ok(v as f64)).to_cf()?,
            F32ConvertI32U => self.stack.values.replace_top::<u32, _>(|v| Ok(v as f32)).to_cf()?,
            F32ConvertI64U => self.stack.values.replace_top::<u64, _>(|v| Ok(v as f32)).to_cf()?,
            F64ConvertI32U => self.stack.values.replace_top::<u32, _>(|v| Ok(v as f64)).to_cf()?,
            F64ConvertI64U => self.stack.values.replace_top::<u64, _>(|v| Ok(v as f64)).to_cf()?,

            I32Extend8S => self.stack.values.replace_top_same::<i32>(|v| Ok((v as i8) as i32)).to_cf()?,
            I32Extend16S => self.stack.values.replace_top_same::<i32>(|v| Ok((v as i16) as i32)).to_cf()?,
            I64Extend8S => self.stack.values.replace_top_same::<i64>(|v| Ok((v as i8) as i64)).to_cf()?,
            I64Extend16S => self.stack.values.replace_top_same::<i64>(|v| Ok((v as i16) as i64)).to_cf()?,
            I64Extend32S => self.stack.values.replace_top_same::<i64>(|v| Ok((v as i32) as i64)).to_cf()?,
            I64ExtendI32U => self.stack.values.replace_top::<u32, _>(|v| Ok(v as i64)).to_cf()?,
            I64ExtendI32S => self.stack.values.replace_top::<i32, _>(|v| Ok(v as i64)).to_cf()?,
            I32WrapI64 => self.stack.values.replace_top::<i64, _>(|v| Ok(v as i32)).to_cf()?,

            F32DemoteF64 => self.stack.values.replace_top::<f64, _>(|v| Ok(v as f32)).to_cf()?,
            F64PromoteF32 => self.stack.values.replace_top::<f32, _>(|v| Ok(v as f64)).to_cf()?,

            F32Abs => self.stack.values.replace_top_same::<f32>(|v| Ok(v.abs())).to_cf()?,
            F64Abs => self.stack.values.replace_top_same::<f64>(|v| Ok(v.abs())).to_cf()?,
            F32Neg => self.stack.values.replace_top_same::<f32>(|v| Ok(-v)).to_cf()?,
            F64Neg => self.stack.values.replace_top_same::<f64>(|v| Ok(-v)).to_cf()?,
            F32Ceil => self.stack.values.replace_top_same::<f32>(|v| Ok(v.ceil())).to_cf()?,
            F64Ceil => self.stack.values.replace_top_same::<f64>(|v| Ok(v.ceil())).to_cf()?,
            F32Floor => self.stack.values.replace_top_same::<f32>(|v| Ok(v.floor())).to_cf()?,
            F64Floor => self.stack.values.replace_top_same::<f64>(|v| Ok(v.floor())).to_cf()?,
            F32Trunc => self.stack.values.replace_top_same::<f32>(|v| Ok(v.trunc())).to_cf()?,
            F64Trunc => self.stack.values.replace_top_same::<f64>(|v| Ok(v.trunc())).to_cf()?,
            F32Nearest => self.stack.values.replace_top_same::<f32>(|v| Ok(v.tw_nearest())).to_cf()?,
            F64Nearest => self.stack.values.replace_top_same::<f64>(|v| Ok(v.tw_nearest())).to_cf()?,
            F32Sqrt => self.stack.values.replace_top_same::<f32>(|v| Ok(v.sqrt())).to_cf()?,
            F64Sqrt => self.stack.values.replace_top_same::<f64>(|v| Ok(v.sqrt())).to_cf()?,
            F32Min => self.stack.values.calculate_same::<f32>(|a, b| Ok(a.tw_minimum(b))).to_cf()?,
            F64Min => self.stack.values.calculate_same::<f64>(|a, b| Ok(a.tw_minimum(b))).to_cf()?,
            F32Max => self.stack.values.calculate_same::<f32>(|a, b| Ok(a.tw_maximum(b))).to_cf()?,
            F64Max => self.stack.values.calculate_same::<f64>(|a, b| Ok(a.tw_maximum(b))).to_cf()?,
            F32Copysign => self.stack.values.calculate_same::<f32>(|a, b| Ok(a.copysign(b))).to_cf()?,
            F64Copysign => self.stack.values.calculate_same::<f64>(|a, b| Ok(a.copysign(b))).to_cf()?,

            I32TruncF32S => checked_conv_float!(f32, i32, self),
            I32TruncF64S => checked_conv_float!(f64, i32, self),
            I32TruncF32U => checked_conv_float!(f32, u32, i32, self),
            I32TruncF64U => checked_conv_float!(f64, u32, i32, self),
            I64TruncF32S => checked_conv_float!(f32, i64, self),
            I64TruncF64S => checked_conv_float!(f64, i64, self),
            I64TruncF32U => checked_conv_float!(f32, u64, i64, self),
            I64TruncF64U => checked_conv_float!(f64, u64, i64, self),

            TableGet(table_idx) => self.exec_table_get(*table_idx).to_cf()?,
            TableSet(table_idx) => self.exec_table_set(*table_idx).to_cf()?,
            TableSize(table_idx) => self.exec_table_size(*table_idx).to_cf()?,
            TableInit(elem_idx, table_idx) => self.exec_table_init(*elem_idx, *table_idx).to_cf()?,
            TableGrow(table_idx) => self.exec_table_grow(*table_idx).to_cf()?,
            TableFill(table_idx) => self.exec_table_fill(*table_idx).to_cf()?,

            I32TruncSatF32S => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc() as i32)).to_cf()?,
            I32TruncSatF32U => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc() as u32)).to_cf()?,
            I32TruncSatF64S => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc() as i32)).to_cf()?,
            I32TruncSatF64U => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc() as u32)).to_cf()?,
            I64TruncSatF32S => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc() as i64)).to_cf()?,
            I64TruncSatF32U => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc() as u64)).to_cf()?,
            I64TruncSatF64S => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc() as i64)).to_cf()?,
            I64TruncSatF64U => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc() as u64)).to_cf()?,

            LocalCopy32(from, to) => self.exec_local_copy::<Value32>(*from, *to),
            LocalCopy64(from, to) => self.exec_local_copy::<Value64>(*from, *to),
            LocalCopy128(from, to) => self.exec_local_copy::<Value128>(*from, *to),
            LocalCopyRef(from, to) => self.exec_local_copy::<ValueRef>(*from, *to),

            V128Not => self.stack.values.replace_top_same::<Value128>(|v| Ok(!v)).to_cf()?,
            V128And => self.stack.values.calculate_same::<Value128>(|a, b| Ok(a & b)).to_cf()?,
            V128AndNot => self.stack.values.calculate_same::<Value128>(|a, b| Ok(a & (!b))).to_cf()?,
            V128Or => self.stack.values.calculate_same::<Value128>(|a, b| Ok(a | b)).to_cf()?,
            V128Xor => self.stack.values.calculate_same::<Value128>(|a, b| Ok(a ^ b)).to_cf()?,
            V128Bitselect => self.stack.values.calculate_same_3::<Value128>(|v1, v2, c| Ok((v1 & c) | (v2 & !c))).to_cf()?,
            V128AnyTrue => self.stack.values.replace_top::<Value128, i32>(|v| Ok((v.reduce_or() != 0) as i32)).to_cf()?,
            I8x16Swizzle => self.stack.values.calculate_same::<Value128>(|a, s| Ok(a.swizzle_dyn(s))).to_cf()?,
            V128Load(arg) => self.exec_mem_load::<Value128, 16, _>(arg.mem_addr(), arg.offset(), |v| v)?,
            V128Store(arg) => self.exec_mem_store::<Value128, Value128, 16>(arg.mem_addr(), arg.offset(), |v| v)?,
            V128Const(arg) => self.exec_const::<Value128>( self.cf.data().v128_constants[*arg as usize].to_le_bytes().into()),

            I8x16ExtractLaneS(lane) => self.stack.values.replace_top::<i8x16, i32>(|v| Ok(v[*lane as usize] as i32)).to_cf()?,
            I8x16ExtractLaneU(lane) => self.stack.values.replace_top::<u8x16, i32>(|v| Ok(v[*lane as usize] as i32)).to_cf()?,
            I16x8ExtractLaneS(lane) => self.stack.values.replace_top::<i16x8, i32>(|v| Ok(v[*lane as usize] as i32)).to_cf()?,
            I16x8ExtractLaneU(lane) => self.stack.values.replace_top::<u16x8, i32>(|v| Ok(v[*lane as usize] as i32)).to_cf()?,
            I32x4ExtractLane(lane) => self.stack.values.replace_top::<i32x4, i32>(|v| Ok(v[*lane as usize])).to_cf()?,
            I64x2ExtractLane(lane) => self.stack.values.replace_top::<i64x2, i64>(|v| Ok(v[*lane as usize])).to_cf()?,
            F32x4ExtractLane(lane) => self.stack.values.replace_top::<f32x4, f32>(|v| Ok(v[*lane as usize])).to_cf()?,
            F64x2ExtractLane(lane) => self.stack.values.replace_top::<f64x2, f64>(|v| Ok(v[*lane as usize])).to_cf()?,

            V128Load8Lane(arg, lane) => self.exec_mem_load_lane::<i8, i8x16, 1>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load16Lane(arg, lane) => self.exec_mem_load_lane::<i16, i16x8, 2>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load32Lane(arg, lane) => self.exec_mem_load_lane::<i32, i32x4, 4>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load64Lane(arg, lane) => self.exec_mem_load_lane::<i64, i64x2, 8>(arg.mem_addr(), arg.offset(), *lane)?,

            I8x16Splat => self.stack.values.replace_top::<i32, i8x16>(|v| Ok(Simd::<i8, 16>::splat(v as i8))).to_cf()?,
            I16x8Splat => self.stack.values.replace_top::<i32, i16x8>(|v| Ok(Simd::<i16, 8>::splat(v as i16))).to_cf()?,
            I32x4Splat => self.stack.values.replace_top::<i32, i32x4>(|v| Ok(Simd::<i32, 4>::splat(v))).to_cf()?,
            I64x2Splat => self.stack.values.replace_top::<i64, i64x2>(|v| Ok(Simd::<i64, 2>::splat(v))).to_cf()?,
            F32x4Splat => self.stack.values.replace_top::<f32, f32x4>(|v| Ok(Simd::<f32, 4>::splat(v))).to_cf()?,
            F64x2Splat => self.stack.values.replace_top::<f64, f64x2>(|v| Ok(Simd::<f64, 2>::splat(v))).to_cf()?,

            I8x16Eq => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_eq(b).to_int())).to_cf()?,
            I16x8Eq => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_eq(b).to_int())).to_cf()?,
            I32x4Eq => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_eq(b).to_int())).to_cf()?,
            F32x4Eq => self.stack.values.calculate::<f32x4, _>(|a, b| Ok(a.simd_eq(b).to_int())).to_cf()?,
            F64x2Eq => self.stack.values.calculate::<f64x2, _>(|a, b| Ok(a.simd_eq(b).to_int())).to_cf()?,

            I8x16Ne => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_ne(b).to_int())).to_cf()?,
            I16x8Ne => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_ne(b).to_int())).to_cf()?,
            I32x4Ne => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_ne(b).to_int())).to_cf()?,
            F32x4Ne => self.stack.values.calculate::<f32x4, _>(|a, b| Ok(a.simd_ne(b).to_int())).to_cf()?,
            F64x2Ne => self.stack.values.calculate::<f64x2, _>(|a, b| Ok(a.simd_ne(b).to_int())).to_cf()?,

            I8x16LtS => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,
            I16x8LtS => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,
            I32x4LtS => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,
            I64x2LtS => self.stack.values.calculate_same::<i64x2>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,
            I8x16LtU => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,
            I16x8LtU => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,
            I32x4LtU => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,
            F32x4Lt => self.stack.values.calculate::<f32x4, _>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,
            F64x2Lt => self.stack.values.calculate::<f64x2, _>(|a, b| Ok(a.simd_lt(b).to_int())).to_cf()?,

            I64x2GtS => self.stack.values.calculate_same::<i64x2>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,
            F32x4Gt => self.stack.values.calculate::<f32x4, _>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,
            F64x2Gt => self.stack.values.calculate::<f64x2, _>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,

            I8x16GtS => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,
            I16x8GtS => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,
            I32x4GtS => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,
            I64x2LeS => self.stack.values.calculate_same::<i64x2>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,
            F32x4Le => self.stack.values.calculate::<f32x4,_>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,
            F64x2Le => self.stack.values.calculate::<f64x2,_>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,

            I8x16GtU => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,
            I16x8GtU => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,
            I32x4GtU => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_gt(b).to_int())).to_cf()?,
            I64x2GeS => self.stack.values.calculate_same::<i64x2>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,
            F32x4Ge => self.stack.values.calculate::<f32x4,_>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,
            F64x2Ge => self.stack.values.calculate::<f64x2,_>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,

            I8x16LeS => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,
            I16x8LeS => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,
            I32x4LeS => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,

            I8x16LeU => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,
            I16x8LeU => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,
            I32x4LeU => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_le(b).to_int())).to_cf()?,

            I8x16GeS => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,
            I16x8GeS => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,
            I32x4GeS => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,

            I8x16GeU => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,
            I16x8GeU => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,
            I32x4GeU => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_ge(b).to_int())).to_cf()?,

            I8x16Abs => self.stack.values.replace_top_same::<i8x16>(|a| Ok(a.abs())).to_cf()?,
            I16x8Abs => self.stack.values.replace_top_same::<i16x8>(|a| Ok(a.abs())).to_cf()?,
            I32x4Abs => self.stack.values.replace_top_same::<i32x4>(|a| Ok(a.abs())).to_cf()?,
            I64x2Abs => self.stack.values.replace_top_same::<i64x2>(|a| Ok(a.abs())).to_cf()?,

            I8x16Neg => self.stack.values.replace_top_same::<i8x16>(|a| Ok(-a)).to_cf()?,
            I16x8Neg => self.stack.values.replace_top_same::<i16x8>(|a| Ok(-a)).to_cf()?,
            I32x4Neg => self.stack.values.replace_top_same::<i32x4>(|a| Ok(-a)).to_cf()?,
            I64x2Neg => self.stack.values.replace_top_same::<i64x2>(|a| Ok(-a)).to_cf()?,

            I8x16AllTrue => self.stack.values.replace_top::<i8x16, i32>(|v| Ok((v.simd_ne(Simd::splat(0)).all()) as i32)).to_cf()?,
            I16x8AllTrue => self.stack.values.replace_top::<i16x8, i32>(|v| Ok((v.simd_ne(Simd::splat(0)).all()) as i32)).to_cf()?,
            I32x4AllTrue => self.stack.values.replace_top::<i32x4, i32>(|v| Ok((v.simd_ne(Simd::splat(0)).all()) as i32)).to_cf()?,
            I64x2AllTrue => self.stack.values.replace_top::<i64x2, i32>(|v| Ok((v.simd_ne(Simd::splat(0)).all()) as i32)).to_cf()?,

            I8x16Bitmask => self.stack.values.replace_top::<i8x16, i32>(|v| Ok(v.simd_lt(Simd::splat(0)).to_bitmask() as i32)).to_cf()?,
            I16x8Bitmask => self.stack.values.replace_top::<i16x8, i32>(|v| Ok(v.simd_lt(Simd::splat(0)).to_bitmask() as i32)).to_cf()?,
            I32x4Bitmask => self.stack.values.replace_top::<i32x4, i32>(|v| Ok(v.simd_lt(Simd::splat(0)).to_bitmask() as i32)).to_cf()?,
            I64x2Bitmask => self.stack.values.replace_top::<i64x2, i32>(|v| Ok(v.simd_lt(Simd::splat(0)).to_bitmask() as i32)).to_cf()?,

            I8x16Shl => self.stack.values.calculate_diff::<i32, i8x16, i8x16>(|a, b| Ok(b.shl(a as i8))).to_cf()?,
            I16x8Shl => self.stack.values.calculate_diff::<i32, i16x8, i16x8>(|a, b| Ok(b.shl(a as i16))).to_cf()?,
            I32x4Shl => self.stack.values.calculate_diff::<i32, i32x4, i32x4>(|a, b| Ok(b.shl(a as i32))).to_cf()?,
            I64x2Shl => self.stack.values.calculate_diff::<i32, i64x2, i64x2>(|a, b| Ok(b.shl(a as i64))).to_cf()?,

            I8x16ShrS => self.stack.values.calculate_diff::<i32, i8x16, i8x16>(|a, b| Ok(b.shr(a as i8))).to_cf()?,
            I16x8ShrS => self.stack.values.calculate_diff::<i32, i16x8, i16x8>(|a, b| Ok(b.shr(a as i16))).to_cf()?,
            I32x4ShrS => self.stack.values.calculate_diff::<i32, i32x4, i32x4>(|a, b| Ok(b.shr(a as i32))).to_cf()?,
            I64x2ShrS => self.stack.values.calculate_diff::<i32, i64x2, i64x2>(|a, b| Ok(b.shr(a as i64))).to_cf()?,

            I8x16ShrU => self.stack.values.calculate_diff::<i32, u8x16, u8x16>(|a, b| Ok(b.shr(a as u8))).to_cf()?,
            I16x8ShrU => self.stack.values.calculate_diff::<i32, u16x8, u16x8>(|a, b| Ok(b.shr(a as u16))).to_cf()?,
            I32x4ShrU => self.stack.values.calculate_diff::<i32, u32x4, u32x4>(|a, b| Ok(b.shr(a as u32))).to_cf()?,
            I64x2ShrU => self.stack.values.calculate_diff::<i32, u64x2, u64x2>(|a, b| Ok(b.shr(a as u64))).to_cf()?,

            I8x16Add => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a + b)).to_cf()?,
            I16x8Add => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a + b)).to_cf()?,
            I32x4Add => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a + b)).to_cf()?,
            I64x2Add => self.stack.values.calculate_same::<i64x2>(|a, b| Ok(a + b)).to_cf()?,

            I8x16Sub => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a - b)).to_cf()?,
            I16x8Sub => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a - b)).to_cf()?,
            I32x4Sub => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a - b)).to_cf()?,
            I64x2Sub => self.stack.values.calculate_same::<i64x2>(|a, b| Ok(a - b)).to_cf()?,

            I8x16MinS => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_min(b))).to_cf()?,
            I16x8MinS => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_min(b))).to_cf()?,
            I32x4MinS => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_min(b))).to_cf()?,

            I8x16MinU => self.stack.values.calculate_same::<u8x16>(|a, b| Ok(a.simd_min(b))).to_cf()?,
            I16x8MinU => self.stack.values.calculate_same::<u16x8>(|a, b| Ok(a.simd_min(b))).to_cf()?,
            I32x4MinU => self.stack.values.calculate_same::<u32x4>(|a, b| Ok(a.simd_min(b))).to_cf()?,

            I8x16MaxS => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.simd_max(b))).to_cf()?,
            I16x8MaxS => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.simd_max(b))).to_cf()?,
            I32x4MaxS => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a.simd_max(b))).to_cf()?,

            I8x16MaxU => self.stack.values.calculate_same::<u8x16>(|a, b| Ok(a.simd_max(b))).to_cf()?,
            I16x8MaxU => self.stack.values.calculate_same::<u16x8>(|a, b| Ok(a.simd_max(b))).to_cf()?,
            I32x4MaxU => self.stack.values.calculate_same::<u32x4>(|a, b| Ok(a.simd_max(b))).to_cf()?,

            I64x2Mul => self.stack.values.calculate_same::<i64x2>(|a, b| Ok(a * b)).to_cf()?,
            I16x8Mul => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a * b)).to_cf()?,
            I32x4Mul => self.stack.values.calculate_same::<i32x4>(|a, b| Ok(a * b)).to_cf()?,

            I8x16NarrowI16x8S => unimplemented!(),
            I8x16NarrowI16x8U => unimplemented!(),
            I16x8NarrowI32x4S => unimplemented!(),
            I16x8NarrowI32x4U => unimplemented!(),

            I8x16AddSatS => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.saturating_add(b))).to_cf()?,
            I16x8AddSatS => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.saturating_add(b))).to_cf()?,
            I8x16AddSatU => self.stack.values.calculate_same::<u8x16>(|a, b| Ok(a.saturating_add(b))).to_cf()?,
            I16x8AddSatU => self.stack.values.calculate_same::<u16x8>(|a, b| Ok(a.saturating_add(b))).to_cf()?,
            I8x16SubSatS => self.stack.values.calculate_same::<i8x16>(|a, b| Ok(a.saturating_sub(b))).to_cf()?,
            I16x8SubSatS => self.stack.values.calculate_same::<i16x8>(|a, b| Ok(a.saturating_sub(b))).to_cf()?,
            I8x16SubSatU => self.stack.values.calculate_same::<u8x16>(|a, b| Ok(a.saturating_sub(b))).to_cf()?,
            I16x8SubSatU => self.stack.values.calculate_same::<u16x8>(|a, b| Ok(a.saturating_sub(b))).to_cf()?,

            I16x8ExtAddPairwiseI8x16S => unimplemented!(),
            I16x8ExtAddPairwiseI8x16U => unimplemented!(),
            I32x4ExtAddPairwiseI16x8S => unimplemented!(),
            I32x4ExtAddPairwiseI16x8U => unimplemented!(),

            I16x8ExtMulLowI8x16S => unimplemented!(),
            I16x8ExtMulLowI8x16U => unimplemented!(),
            I16x8ExtMulHighI8x16S => unimplemented!(),
            I16x8ExtMulHighI8x16U => unimplemented!(),
            I32x4ExtMulLowI16x8S => unimplemented!(),
            I32x4ExtMulLowI16x8U => unimplemented!(),
            I32x4ExtMulHighI16x8S => unimplemented!(),
            I32x4ExtMulHighI16x8U => unimplemented!(),
            I64x2ExtMulLowI32x4S => unimplemented!(),
            I64x2ExtMulLowI32x4U => unimplemented!(),
            I64x2ExtMulHighI32x4S => unimplemented!(),
            I64x2ExtMulHighI32x4U => unimplemented!(),

            I16x8ExtendLowI8x16S => unimplemented!(),
            I16x8ExtendLowI8x16U => unimplemented!(),
            I16x8ExtendHighI8x16S => unimplemented!(),
            I16x8ExtendHighI8x16U => unimplemented!(),
            I32x4ExtendLowI16x8S => unimplemented!(),
            I32x4ExtendLowI16x8U => unimplemented!(),
            I32x4ExtendHighI16x8S => unimplemented!(),
            I32x4ExtendHighI16x8U => unimplemented!(),
            I64x2ExtendLowI32x4S => unimplemented!(),
            I64x2ExtendLowI32x4U => unimplemented!(),
            I64x2ExtendHighI32x4S => unimplemented!(),
            I64x2ExtendHighI32x4U => unimplemented!(),

            I8x16Popcnt => self.stack.values.replace_top::<i8x16, _>(|v| Ok(v.count_ones())).to_cf()?,

            I16x8Q15MulrSatS =>  self.stack.values.calculate_same::<i16x8>(|a, b| {
                let subq15mulr = |a,b| {
                    let a = a as i32;
                    let b = b as i32;
                    let r = (a * b + 0x4000) >> 15;
                    if r > i16::MAX as i32 {
                        i16::MAX
                    } else if r < i16::MIN as i32 {
                        i16::MIN
                    } else {
                        r as i16
                    }
                };
                Ok(Simd::<i16, 8>::from_array([
                    subq15mulr(a[0], b[0]),
                    subq15mulr(a[1], b[1]),
                    subq15mulr(a[2], b[2]),
                    subq15mulr(a[3], b[3]),
                    subq15mulr(a[4], b[4]),
                    subq15mulr(a[5], b[5]),
                    subq15mulr(a[6], b[6]),
                    subq15mulr(a[7], b[7]),
                ]))
            }).to_cf()?,

            I32x4DotI16x8S => self.stack.values.calculate::<i16x8, i32x4>(|a, b| {
                Ok(Simd::<i32, 4>::from_array([
                    i32::from(a[0] * b[0] + a[1] * b[1]),
                    i32::from(a[2] * b[2] + a[3] * b[3]),
                    i32::from(a[4] * b[4] + a[5] * b[5]),
                    i32::from(a[6] * b[6] + a[7] * b[7]),
                ]))
            }).to_cf()?,

            F32x4Ceil => self.stack.values.replace_top_same::<f32x4>(|v| Ok(v.ceil())).to_cf()?,
            F64x2Ceil => self.stack.values.replace_top_same::<f64x2>(|v| Ok(v.ceil())).to_cf()?,
            F32x4Floor => self.stack.values.replace_top_same::<f32x4>(|v| Ok(v.floor())).to_cf()?,
            F64x2Floor => self.stack.values.replace_top_same::<f64x2>(|v| Ok(v.floor())).to_cf()?,
            F32x4Trunc => self.stack.values.replace_top_same::<f32x4>(|v| Ok(v.trunc())).to_cf()?,
            F64x2Trunc => self.stack.values.replace_top_same::<f64x2>(|v| Ok(v.trunc())).to_cf()?,
            F32x4Abs => self.stack.values.replace_top_same::<f32x4>(|v| Ok(v.abs())).to_cf()?,
            F64x2Abs => self.stack.values.replace_top_same::<f64x2>(|v| Ok(v.abs())).to_cf()?,
            F32x4Neg => self.stack.values.replace_top_same::<f32x4>(|v| Ok(-v)).to_cf()?,
            F64x2Neg => self.stack.values.replace_top_same::<f64x2>(|v| Ok(-v)).to_cf()?,
            F32x4Sqrt => self.stack.values.replace_top_same::<f32x4>(|v| Ok(v.sqrt())).to_cf()?,
            F64x2Sqrt => self.stack.values.replace_top_same::<f64x2>(|v| Ok(v.sqrt())).to_cf()?,
            F32x4Add => self.stack.values.calculate_same::<f32x4>(|a, b| Ok(a + b)).to_cf()?,
            F64x2Add => self.stack.values.calculate_same::<f64x2>(|a, b| Ok(a + b)).to_cf()?,
            F32x4Sub => self.stack.values.calculate_same::<f32x4>(|a, b| Ok(a - b)).to_cf()?,
            F64x2Sub => self.stack.values.calculate_same::<f64x2>(|a, b| Ok(a - b)).to_cf()?,
            F32x4Mul => self.stack.values.calculate_same::<f32x4>(|a, b| Ok(a * b)).to_cf()?,
            F64x2Mul => self.stack.values.calculate_same::<f64x2>(|a, b| Ok(a * b)).to_cf()?,
            F32x4Div => self.stack.values.calculate_same::<f32x4>(|a, b| Ok(a / b)).to_cf()?,
            F64x2Div => self.stack.values.calculate_same::<f64x2>(|a, b| Ok(a / b)).to_cf()?,
            F32x4Min => self.stack.values.calculate_same::<f32x4>(|a, b| Ok(a.simd_min(b))).to_cf()?,
            F64x2Min => self.stack.values.calculate_same::<f64x2>(|a, b| Ok(a.simd_min(b))).to_cf()?,
            F32x4Max => self.stack.values.calculate_same::<f32x4>(|a, b| Ok(a.simd_max(b))).to_cf()?,
            F64x2Max => self.stack.values.calculate_same::<f64x2>(|a, b| Ok(a.simd_max(b))).to_cf()?,

            F32x4PMin => self.stack.values.calculate_same::<f32x4>(|a, b| {
                Ok(Simd::<f32, 4>::from_array([
                    if b[0] < a[0] { b[0] } else { a[0]},
                    if b[1] < a[1] { b[1] } else { a[1]},
                    if b[2] < a[2] { b[2] } else { a[2]},
                    if b[3] < a[3] { b[3] } else { a[3]},
                ]))
            }).to_cf()?,
            F32x4PMax => self.stack.values.calculate_same::<f32x4>(|a, b| {
                Ok(Simd::<f32, 4>::from_array([
                    if b[0] > a[0] { b[0] } else { a[0]},
                    if b[1] > a[1] { b[1] } else { a[1]},
                    if b[2] > a[2] { b[2] } else { a[2]},
                    if b[3] > a[3] { b[3] } else { a[3]},
                ]))
            }).to_cf()?,
            F64x2PMin => self.stack.values.calculate_same::<f64x2>(|a, b| {
                Ok(Simd::<f64, 2>::from_array([
                    if b[0] < a[0] { b[0] } else { a[0]},
                    if b[1] < a[1] { b[1] } else { a[1]},
                ]))
            }).to_cf()?,
            F64x2PMax => self.stack.values.calculate_same::<f64x2>(|a, b| {
                Ok(Simd::<f64, 2>::from_array([
                    if b[0] > a[0] { b[0] } else { a[0]},
                    if b[1] > a[1] { b[1] } else { a[1]},
                ]))
            }).to_cf()?,

            // not correct
            I32x4TruncSatF32x4S => self.stack.values.replace_top::<f32x4, f32x4>(|v| Ok(v.trunc())).to_cf()?,
            I32x4TruncSatF32x4U => self.stack.values.replace_top::<f32x4, f32x4>(|v| Ok(v.trunc())).to_cf()?,
            F32x4ConvertI32x4S => {},
            F32x4ConvertI32x4U => {},
            F64x2ConvertLowI32x4S => {},
            F64x2ConvertLowI32x4U => {},
            F32x4DemoteF64x2Zero => {},
            F64x2PromoteLowF32x4 => {},
            I32x4TruncSatF64x2SZero => unimplemented!(),
            I32x4TruncSatF64x2UZero => unimplemented!(),

            i => return ControlFlow::Break(Some(Error::UnsupportedFeature(format!("unimplemented opcode: {i:?}")))),
        };

        self.cf.incr_instr_ptr();
        ControlFlow::Continue(())
    }

    #[cold]
    fn exec_unreachable(&self) -> ControlFlow<Option<Error>> {
        ControlFlow::Break(Some(Trap::Unreachable.into()))
    }

    fn exec_call<const IS_RETURN_CALL: bool>(
        &mut self,
        wasm_func: Rc<WasmFunction>,
        owner: ModuleInstanceAddr,
    ) -> ControlFlow<Option<Error>> {
        if !IS_RETURN_CALL {
            let locals = self.stack.values.pop_locals(wasm_func.params, wasm_func.locals);
            let new_call_frame = CallFrame::new_raw(wasm_func, owner, locals, self.stack.blocks.len() as u32);
            self.cf.incr_instr_ptr(); // skip the call instruction
            self.stack.call_stack.push(core::mem::replace(&mut self.cf, new_call_frame))?;
            self.module.swap_with(self.cf.module_addr(), self.store);
        } else {
            let locals = self.stack.values.pop_locals(wasm_func.params, wasm_func.locals);
            self.cf.reuse_for(wasm_func, locals, self.stack.blocks.len() as u32, owner);
            self.module.swap_with(self.cf.module_addr(), self.store);
        }

        ControlFlow::Continue(())
    }
    fn exec_call_host(&mut self, host_func: Rc<imports::HostFunction>) -> ControlFlow<Option<Error>> {
        let params = self.stack.values.pop_params(&host_func.ty.params);
        let res = host_func
            .clone()
            .call(FuncContext { store: self.store, module_addr: self.module.id() }, &params)
            .to_cf()?;
        self.stack.values.extend_from_wasmvalues(&res);
        self.cf.incr_instr_ptr();
        ControlFlow::Continue(())
    }
    fn exec_call_direct<const IS_RETURN_CALL: bool>(&mut self, v: u32) -> ControlFlow<Option<Error>> {
        let func_inst = self.store.get_func(self.module.resolve_func_addr(v));
        match func_inst.func.clone() {
            crate::Function::Wasm(wasm_func) => self.exec_call::<IS_RETURN_CALL>(wasm_func, func_inst.owner),
            crate::Function::Host(host_func) => self.exec_call_host(host_func),
        }
    }
    fn exec_call_indirect<const IS_RETURN_CALL: bool>(
        &mut self,
        type_addr: u32,
        table_addr: u32,
    ) -> ControlFlow<Option<Error>> {
        // verify that the table is of the right type, this should be validated by the parser already
        let func_ref = {
            let table = self.store.get_table(self.module.resolve_table_addr(table_addr));
            let table_idx: u32 = self.stack.values.pop::<i32>() as u32;
            assert!(table.kind.element_type == ValType::RefFunc, "table is not of type funcref");
            let table = table.get(table_idx).map_err(|_| Trap::UndefinedElement { index: table_idx as usize }.into());
            let table = table.to_cf()?;
            table.addr().ok_or(Trap::UninitializedElement { index: table_idx as usize }.into()).to_cf()?
        };

        let func_inst = self.store.get_func(func_ref);
        let call_ty = self.module.func_ty(type_addr);

        match func_inst.func.clone() {
            crate::Function::Wasm(wasm_func) => {
                if unlikely(wasm_func.ty != *call_ty) {
                    return ControlFlow::Break(Some(
                        Trap::IndirectCallTypeMismatch { actual: wasm_func.ty.clone(), expected: call_ty.clone() }
                            .into(),
                    ));
                }

                self.exec_call::<IS_RETURN_CALL>(wasm_func, func_inst.owner)
            }
            crate::Function::Host(host_func) => {
                if unlikely(host_func.ty != *call_ty) {
                    return ControlFlow::Break(Some(
                        Trap::IndirectCallTypeMismatch { actual: host_func.ty.clone(), expected: call_ty.clone() }
                            .into(),
                    ));
                }

                self.exec_call_host(host_func)
            }
        }
    }

    fn exec_if(&mut self, else_offset: u32, end_offset: u32, (params, results): (StackHeight, StackHeight)) {
        // truthy value is on the top of the stack, so enter the then block
        if self.stack.values.pop::<i32>() != 0 {
            self.enter_block(end_offset, BlockType::If, (params, results));
            return;
        }

        // falsy value is on the top of the stack
        if else_offset == 0 {
            self.cf.jump(end_offset as usize);
            return;
        }

        self.cf.jump(else_offset as usize);
        self.enter_block(end_offset - else_offset, BlockType::Else, (params, results));
    }
    fn exec_else(&mut self, end_offset: u32) {
        self.exec_end_block();
        self.cf.jump(end_offset as usize);
    }
    fn resolve_functype(&self, idx: u32) -> (StackHeight, StackHeight) {
        let ty = self.module.func_ty(idx);
        ((&*ty.params).into(), (&*ty.results).into())
    }
    fn enter_block(&mut self, end_instr_offset: u32, ty: BlockType, (params, results): (StackHeight, StackHeight)) {
        self.stack.blocks.push(BlockFrame {
            instr_ptr: self.cf.instr_ptr(),
            end_instr_offset,
            stack_ptr: self.stack.values.height(),
            results,
            params,
            ty,
        });
    }
    fn exec_br(&mut self, to: u32) -> ControlFlow<Option<Error>> {
        if self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks).is_none() {
            return self.exec_return();
        }

        self.cf.incr_instr_ptr();
        ControlFlow::Continue(())
    }
    fn exec_br_if(&mut self, to: u32) -> ControlFlow<Option<Error>> {
        if self.stack.values.pop::<i32>() != 0
            && self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks).is_none()
        {
            return self.exec_return();
        }
        self.cf.incr_instr_ptr();
        ControlFlow::Continue(())
    }
    fn exec_brtable(&mut self, default: u32, len: u32) -> ControlFlow<Option<Error>> {
        let start = self.cf.instr_ptr() + 1;
        let end = start + len as usize;
        if end > self.cf.instructions().len() {
            return ControlFlow::Break(Some(Error::Other(format!(
                "br_table out of bounds: {} >= {}",
                end,
                self.cf.instructions().len()
            ))));
        }

        let idx = self.stack.values.pop::<i32>();
        let to = match self.cf.instructions()[start..end].get(idx as usize) {
            None => default,
            Some(Instruction::BrLabel(to)) => *to,
            _ => return ControlFlow::Break(Some(Error::Other("br_table out of bounds".to_string()))),
        };

        if self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks).is_none() {
            return self.exec_return();
        }

        self.cf.incr_instr_ptr();
        ControlFlow::Continue(())
    }
    fn exec_return(&mut self) -> ControlFlow<Option<Error>> {
        let old = self.cf.block_ptr();
        match self.stack.call_stack.pop() {
            None => return ControlFlow::Break(None),
            Some(cf) => self.cf = cf,
        }

        if old > self.cf.block_ptr() {
            self.stack.blocks.truncate(old);
        }

        self.module.swap_with(self.cf.module_addr(), self.store);
        ControlFlow::Continue(())
    }
    fn exec_end_block(&mut self) {
        let block = self.stack.blocks.pop();
        self.stack.values.truncate_keep(block.stack_ptr, block.results);
    }
    fn exec_local_get<T: InternalValue>(&mut self, local_index: u16) {
        let v = self.cf.locals.get::<T>(local_index);
        self.stack.values.push(v);
    }
    fn exec_local_set<T: InternalValue>(&mut self, local_index: u16) {
        let v = self.stack.values.pop::<T>();
        self.cf.locals.set(local_index, v);
    }
    fn exec_local_tee<T: InternalValue>(&mut self, local_index: u16) {
        let v = self.stack.values.peek::<T>();
        self.cf.locals.set(local_index, v);
    }

    fn exec_global_get(&mut self, global_index: u32) {
        self.stack.values.push_dyn(self.store.get_global_val(self.module.resolve_global_addr(global_index)));
    }
    fn exec_global_set<T: InternalValue>(&mut self, global_index: u32) {
        self.store.set_global_val(self.module.resolve_global_addr(global_index), self.stack.values.pop::<T>().into());
    }
    fn exec_const<T: InternalValue>(&mut self, val: T) {
        self.stack.values.push(val);
    }
    fn exec_ref_is_null(&mut self) {
        let is_null = self.stack.values.pop::<ValueRef>().is_none() as i32;
        self.stack.values.push::<i32>(is_null);
    }

    fn exec_memory_size(&mut self, addr: u32) {
        let mem = self.store.get_mem(self.module.resolve_mem_addr(addr));

        match mem.is_64bit() {
            true => self.stack.values.push::<i64>(mem.page_count as i64),
            false => self.stack.values.push::<i32>(mem.page_count as i32),
        }
    }
    fn exec_memory_grow(&mut self, addr: u32) {
        let mem = self.store.get_mem_mut(self.module.resolve_mem_addr(addr));
        let prev_size = mem.page_count;

        let pages_delta = match mem.is_64bit() {
            true => self.stack.values.pop::<i64>(),
            false => self.stack.values.pop::<i32>() as i64,
        };

        match (
            mem.is_64bit(),
            match mem.grow(pages_delta) {
                Some(_) => prev_size as i64,
                None => -1_i64,
            },
        ) {
            (true, size) => self.stack.values.push::<i64>(size),
            (false, size) => self.stack.values.push::<i32>(size as i32),
        };
    }

    fn exec_memory_copy(&mut self, from: u32, to: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop();
        let src: i32 = self.stack.values.pop();
        let dst: i32 = self.stack.values.pop();

        if from == to {
            let mem_from = self.store.get_mem_mut(self.module.resolve_mem_addr(from));
            // copy within the same memory
            mem_from.copy_within(dst as usize, src as usize, size as usize)?;
        } else {
            // copy between two memories
            let (mem_from, mem_to) =
                self.store.get_mems_mut(self.module.resolve_mem_addr(from), self.module.resolve_mem_addr(to))?;

            mem_from.copy_from_slice(dst as usize, mem_to.load(src as usize, size as usize)?)?;
        }
        Ok(())
    }
    fn exec_memory_fill(&mut self, addr: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop();
        let val: i32 = self.stack.values.pop();
        let dst: i32 = self.stack.values.pop();

        let mem = self.store.get_mem_mut(self.module.resolve_mem_addr(addr));
        mem.fill(dst as usize, size as usize, val as u8)
    }
    fn exec_memory_init(&mut self, data_index: u32, mem_index: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop();
        let offset: i32 = self.stack.values.pop();
        let dst: i32 = self.stack.values.pop();

        let data = self
            .store
            .data
            .datas
            .get(self.module.resolve_data_addr(data_index) as usize)
            .ok_or_else(|| Error::Other("data not found".to_string()))?;

        let mem = self
            .store
            .data
            .memories
            .get_mut(self.module.resolve_mem_addr(mem_index) as usize)
            .ok_or_else(|| Error::Other("memory not found".to_string()))?;

        let data_len = data.data.as_ref().map_or(0, |d| d.len());

        if unlikely(((size + offset) as usize > data_len) || ((dst + size) as usize > mem.len())) {
            return Err(Trap::MemoryOutOfBounds { offset: offset as usize, len: size as usize, max: data_len }.into());
        }

        if size == 0 {
            return Ok(());
        }

        let Some(data) = &data.data else { return Err(Trap::MemoryOutOfBounds { offset: 0, len: 0, max: 0 }.into()) };
        mem.store(dst as usize, size as usize, &data[offset as usize..((offset + size) as usize)])
    }
    fn exec_data_drop(&mut self, data_index: u32) {
        self.store.get_data_mut(self.module.resolve_data_addr(data_index)).drop();
    }
    fn exec_elem_drop(&mut self, elem_index: u32) {
        self.store.get_elem_mut(self.module.resolve_elem_addr(elem_index)).drop();
    }
    fn exec_table_copy(&mut self, from: u32, to: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop();
        let src: i32 = self.stack.values.pop();
        let dst: i32 = self.stack.values.pop();

        if from == to {
            // copy within the same memory
            self.store.get_table_mut(self.module.resolve_table_addr(from)).copy_within(
                dst as usize,
                src as usize,
                size as usize,
            )
        } else {
            // copy between two memories
            let (table_from, table_to) =
                self.store.get_tables_mut(self.module.resolve_table_addr(from), self.module.resolve_table_addr(to))?;
            table_to.copy_from_slice(dst as usize, table_from.load(src as usize, size as usize)?)
        }
    }

    fn exec_mem_load_lane<
        LOAD: MemLoadable<LOAD_SIZE>,
        INTO: InternalValue + IndexMut<usize, Output = LOAD>,
        const LOAD_SIZE: usize,
    >(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        lanes: u8,
    ) -> ControlFlow<Option<Error>> {
        let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_addr));
        let mut imm = self.stack.values.pop::<INTO>();
        let val = self.stack.values.pop::<i32>() as u64;
        let Some(Ok(addr)) = offset.checked_add(val).map(TryInto::try_into) else {
            cold();
            return ControlFlow::Break(Some(Error::Trap(Trap::MemoryOutOfBounds {
                offset: val as usize,
                len: LOAD_SIZE,
                max: 0,
            })));
        };
        let val = mem.load_as::<LOAD_SIZE, LOAD>(addr).to_cf()?;
        imm[lanes as usize] = val;
        self.stack.values.push(imm);
        ControlFlow::Continue(())
    }

    // fn mem_load<LOAD: MemLoadable<LOAD_SIZE>, const LOAD_SIZE: usize, TARGET: InternalValue>(
    //     &mut self,
    //     mem_addr: tinywasm_types::MemAddr,
    //     offset: u64,
    // ) -> Result<LOAD, Error> {
    //     let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_addr));
    //     let val = self.stack.values.pop::<i32>() as u64;
    //     let Some(Ok(addr)) = offset.checked_add(val).map(TryInto::try_into) else {
    //         cold();
    //         return Err(Error::Trap(Trap::MemoryOutOfBounds { offset: val as usize, len: LOAD_SIZE, max: 0 }));
    //     };
    //     mem.load_as::<LOAD_SIZE, LOAD>(addr)
    // }

    fn exec_mem_load<LOAD: MemLoadable<LOAD_SIZE>, const LOAD_SIZE: usize, TARGET: InternalValue>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        cast: fn(LOAD) -> TARGET,
    ) -> ControlFlow<Option<Error>> {
        let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_addr));

        let addr = match mem.is_64bit() {
            true => self.stack.values.pop::<i64>() as u64,
            false => self.stack.values.pop::<i32>() as u32 as u64,
        };

        let Some(Ok(addr)) = offset.checked_add(addr).map(TryInto::try_into) else {
            cold();
            return ControlFlow::Break(Some(Error::Trap(Trap::MemoryOutOfBounds {
                offset: addr as usize,
                len: LOAD_SIZE,
                max: 0,
            })));
        };
        let val = mem.load_as::<LOAD_SIZE, LOAD>(addr).to_cf()?;
        self.stack.values.push(cast(val));
        ControlFlow::Continue(())
    }
    fn exec_mem_store<T: InternalValue, U: MemStorable<N>, const N: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        cast: fn(T) -> U,
    ) -> ControlFlow<Option<Error>> {
        let mem = self.store.get_mem_mut(self.module.resolve_mem_addr(mem_addr));
        let val = self.stack.values.pop::<T>();
        let val = (cast(val)).to_mem_bytes();

        let addr = match mem.is_64bit() {
            true => self.stack.values.pop::<i64>() as u64,
            false => self.stack.values.pop::<i32>() as u32 as u64,
        };

        if let Err(e) = mem.store((offset + addr) as usize, val.len(), &val) {
            return ControlFlow::Break(Some(e));
        }

        ControlFlow::Continue(())
    }

    fn exec_table_get(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index));
        let idx: i32 = self.stack.values.pop::<i32>();
        let v = table.get_wasm_val(idx as u32)?;
        self.stack.values.push_dyn(v.into());
        Ok(())
    }
    fn exec_table_set(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table_mut(self.module.resolve_table_addr(table_index));
        let val = self.stack.values.pop::<ValueRef>();
        let idx = self.stack.values.pop::<i32>() as u32;
        table.set(idx, val.into())
    }
    fn exec_table_size(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index));
        self.stack.values.push_dyn(table.size().into());
        Ok(())
    }
    fn exec_table_init(&mut self, elem_index: u32, table_index: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop(); // n
        let offset: i32 = self.stack.values.pop(); // s
        let dst: i32 = self.stack.values.pop(); // d

        let elem = self
            .store
            .data
            .elements
            .get(self.module.resolve_elem_addr(elem_index) as usize)
            .ok_or_else(|| Error::Other("element not found".to_string()))?;

        let table = self
            .store
            .data
            .tables
            .get_mut(self.module.resolve_table_addr(table_index) as usize)
            .ok_or_else(|| Error::Other("table not found".to_string()))?;

        let elem_len = elem.items.as_ref().map_or(0, alloc::vec::Vec::len);
        let table_len = table.size();

        if unlikely(size < 0 || ((size + offset) as usize > elem_len) || ((dst + size) > table_len)) {
            return Err(Trap::TableOutOfBounds { offset: offset as usize, len: size as usize, max: elem_len }.into());
        }

        if size == 0 {
            return Ok(());
        }

        if let ElementKind::Active { .. } = elem.kind {
            return Err(Error::Other("table.init with active element".to_string()));
        }

        let Some(items) = elem.items.as_ref() else {
            return Err(Trap::TableOutOfBounds { offset: 0, len: 0, max: 0 }.into());
        };

        table.init(dst as i64, &items[offset as usize..(offset + size) as usize])
    }
    fn exec_table_grow(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table_mut(self.module.resolve_table_addr(table_index));
        let sz = table.size();

        let n = self.stack.values.pop::<i32>();
        let val = self.stack.values.pop::<ValueRef>();

        match table.grow(n, val.into()) {
            Ok(_) => self.stack.values.push(sz),
            Err(_) => self.stack.values.push(-1_i32),
        }

        Ok(())
    }
    fn exec_table_fill(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table_mut(self.module.resolve_table_addr(table_index));

        let n = self.stack.values.pop::<i32>();
        let val = self.stack.values.pop::<ValueRef>();
        let i = self.stack.values.pop::<i32>();

        if unlikely(i + n > table.size()) {
            return Err(Error::Trap(Trap::TableOutOfBounds {
                offset: i as usize,
                len: n as usize,
                max: table.size() as usize,
            }));
        }

        if n == 0 {
            return Ok(());
        }

        table.fill(self.module.func_addrs(), i as usize, n as usize, val.into())
    }

    fn exec_local_copy<T: InternalValue>(&mut self, from: u16, to: u16) {
        let v = self.cf.locals.get::<T>(from);
        self.cf.locals.set(to, v);
    }
}
