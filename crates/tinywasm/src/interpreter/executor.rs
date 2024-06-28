#[cfg(not(feature = "std"))]
mod no_std_floats;

use interpreter::CallFrame;
#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use no_std_floats::NoStdFloatExt;

use alloc::{format, rc::Rc, string::ToString};
use core::ops::ControlFlow;
use tinywasm_types::*;

use super::num_helpers::*;
use super::stack::{values::StackHeight, BlockFrame, BlockType, Stack};
use super::values::*;
use crate::*;

pub(super) struct Executor<'store, 'stack> {
    cf: CallFrame,
    module: ModuleInstance,
    store: &'store mut Store,
    stack: &'stack mut Stack,
}

impl<'store, 'stack> Executor<'store, 'stack> {
    pub(crate) fn new(store: &'store mut Store, stack: &'stack mut Stack) -> Result<Self> {
        let current_frame = stack.call_stack.pop().ok_or_else(|| Error::CallStackUnderflow)?;
        let current_module = store.get_module_instance_raw(current_frame.module_addr());
        Ok(Self { cf: current_frame, module: current_module, stack, store })
    }

    #[inline]
    pub(crate) fn run_to_completion(&mut self) -> Result<()> {
        loop {
            match self.exec_next()? {
                ControlFlow::Break(..) => return Ok(()),
                ControlFlow::Continue(..) => continue,
            };
        }
    }

    #[inline(always)]
    fn exec_next(&mut self) -> Result<ControlFlow<()>> {
        use tinywasm_types::Instruction::*;
        match self.cf.fetch_instr() {
            Nop => self.exec_noop(),
            Unreachable => self.exec_unreachable()?,

            Drop32 => self.stack.values.drop::<Value32>()?,
            Drop64 => self.stack.values.drop::<Value64>()?,
            Drop128 => self.stack.values.drop::<Value128>()?,
            DropRef => self.stack.values.drop::<ValueRef>()?,

            Select32 => self.stack.values.select::<Value32>()?,
            Select64 => self.stack.values.select::<Value64>()?,
            Select128 => self.stack.values.select::<Value128>()?,
            SelectRef => self.stack.values.select::<ValueRef>()?,

            Call(v) => return self.exec_call_direct(*v),
            CallIndirect(ty, table) => return self.exec_call_indirect(*ty, *table),

            If(end, el) => self.exec_if(*end, *el, (Default::default(), Default::default()))?,
            IfWithType(ty, end, el) => self.exec_if(*end, *el, (Default::default(), (*ty).into()))?,
            IfWithFuncType(ty, end, el) => self.exec_if(*end, *el, self.resolve_functype(*ty))?,
            Else(end_offset) => self.exec_else(*end_offset)?,
            Loop(end) => {
                self.enter_block(self.cf.instr_ptr(), *end, BlockType::Loop, (Default::default(), Default::default()))
            }
            LoopWithType(ty, end) => {
                self.enter_block(self.cf.instr_ptr(), *end, BlockType::Loop, (Default::default(), (*ty).into()))
            }
            LoopWithFuncType(ty, end) => {
                self.enter_block(self.cf.instr_ptr(), *end, BlockType::Loop, self.resolve_functype(*ty))
            }
            Block(end) => {
                self.enter_block(self.cf.instr_ptr(), *end, BlockType::Block, (Default::default(), Default::default()))
            }
            BlockWithType(ty, end) => {
                self.enter_block(self.cf.instr_ptr(), *end, BlockType::Block, (Default::default(), (*ty).into()))
            }
            BlockWithFuncType(ty, end) => {
                self.enter_block(self.cf.instr_ptr(), *end, BlockType::Block, self.resolve_functype(*ty))
            }
            Br(v) => return self.exec_br(*v),
            BrIf(v) => return self.exec_br_if(*v),
            BrTable(default, len) => return self.exec_brtable(*default, *len),
            BrLabel(_) => {}
            Return => return self.exec_return(),
            EndBlockFrame => self.exec_end_block()?,

            LocalGet32(local_index) => {
                self.cf.locals.get::<Value32>(*local_index).map(|v| self.stack.values.push(v))?
            }
            LocalGet64(local_index) => {
                self.cf.locals.get::<Value64>(*local_index).map(|v| self.stack.values.push(v))?
            }
            LocalGet128(local_index) => {
                self.cf.locals.get::<Value128>(*local_index).map(|v| self.stack.values.push(v))?
            }
            LocalGetRef(local_index) => {
                self.cf.locals.get::<ValueRef>(*local_index).map(|v| self.stack.values.push(v))?
            }

            LocalSet32(local_index) => self.cf.locals.set(*local_index, self.stack.values.pop::<Value32>()?)?,
            LocalSet64(local_index) => self.cf.locals.set(*local_index, self.stack.values.pop::<Value64>()?)?,
            LocalSet128(local_index) => self.cf.locals.set(*local_index, self.stack.values.pop::<Value128>()?)?,
            LocalSetRef(local_index) => self.cf.locals.set(*local_index, self.stack.values.pop::<ValueRef>()?)?,

            LocalTee32(local_index) => self.cf.locals.set(*local_index, self.stack.values.peek::<Value32>()?)?,
            LocalTee64(local_index) => self.cf.locals.set(*local_index, self.stack.values.peek::<Value64>()?)?,
            LocalTee128(local_index) => self.cf.locals.set(*local_index, self.stack.values.peek::<Value128>()?)?,
            LocalTeeRef(local_index) => self.cf.locals.set(*local_index, self.stack.values.peek::<ValueRef>()?)?,

            GlobalGet(global_index) => self.exec_global_get(*global_index)?,
            GlobalSet32(global_index) => self.exec_global_set::<Value32>(*global_index)?,
            GlobalSet64(global_index) => self.exec_global_set::<Value64>(*global_index)?,
            GlobalSet128(global_index) => self.exec_global_set::<Value128>(*global_index)?,
            GlobalSetRef(global_index) => self.exec_global_set::<ValueRef>(*global_index)?,

            I32Const(val) => self.stack.values.push(*val),
            I64Const(val) => self.stack.values.push(*val),
            F32Const(val) => self.stack.values.push::<i32>(val.to_bits() as i32),
            F64Const(val) => self.stack.values.push(val.to_bits() as i64),
            RefFunc(func_idx) => self.stack.values.push(Some(*func_idx)), // do we need to resolve the function index?
            RefNull(_) => self.stack.values.push(None),
            RefIsNull => self.exec_ref_is_null()?,

            MemorySize(addr) => self.exec_memory_size(*addr)?,
            MemoryGrow(addr) => self.exec_memory_grow(*addr)?,

            // Bulk memory operations
            MemoryCopy(from, to) => self.exec_memory_copy(*from, *to)?,
            MemoryFill(addr) => self.exec_memory_fill(*addr)?,
            MemoryInit(data_idx, mem_idx) => self.exec_memory_init(*data_idx, *mem_idx)?,
            DataDrop(data_index) => self.exec_data_drop(*data_index)?,
            ElemDrop(elem_index) => self.exec_elem_drop(*elem_index)?,
            TableCopy { from, to } => self.exec_table_copy(*from, *to)?,

            I32Store { mem_addr, offset } => {
                let v = self.stack.values.pop::<i32>()?;
                self.exec_mem_store::<i32, 4>(v, *mem_addr, *offset)?
            }
            I64Store { mem_addr, offset } => {
                let v = self.stack.values.pop::<i64>()?;
                self.exec_mem_store::<i64, 8>(v, *mem_addr, *offset)?
            }
            F32Store { mem_addr, offset } => {
                let v = self.stack.values.pop::<f32>()?;
                self.exec_mem_store::<f32, 4>(v, *mem_addr, *offset)?
            }
            F64Store { mem_addr, offset } => {
                let v = self.stack.values.pop::<f64>()?;
                self.exec_mem_store::<f64, 8>(v, *mem_addr, *offset)?
            }
            I32Store8 { mem_addr, offset } => {
                let v = self.stack.values.pop::<i32>()? as i8;
                self.exec_mem_store::<i8, 1>(v, *mem_addr, *offset)?
            }
            I32Store16 { mem_addr, offset } => {
                let v = self.stack.values.pop::<i32>()? as i16;
                self.exec_mem_store::<i16, 2>(v, *mem_addr, *offset)?
            }
            I64Store8 { mem_addr, offset } => {
                let v = self.stack.values.pop::<i64>()? as i8;
                self.exec_mem_store::<i8, 1>(v, *mem_addr, *offset)?
            }
            I64Store16 { mem_addr, offset } => {
                let v = self.stack.values.pop::<i64>()? as i16;
                self.exec_mem_store::<i16, 2>(v, *mem_addr, *offset)?
            }
            I64Store32 { mem_addr, offset } => {
                let v = self.stack.values.pop::<i64>()? as i32;
                self.exec_mem_store::<i32, 4>(v, *mem_addr, *offset)?
            }

            I32Load { mem_addr, offset } => self.exec_mem_load::<i32, 4, _>(|v| v, *mem_addr, *offset)?,
            I64Load { mem_addr, offset } => self.exec_mem_load::<i64, 8, _>(|v| v, *mem_addr, *offset)?,
            F32Load { mem_addr, offset } => self.exec_mem_load::<f32, 4, _>(|v| v, *mem_addr, *offset)?,
            F64Load { mem_addr, offset } => self.exec_mem_load::<f64, 8, _>(|v| v, *mem_addr, *offset)?,
            I32Load8S { mem_addr, offset } => self.exec_mem_load::<i8, 1, _>(|v| v as i32, *mem_addr, *offset)?,
            I32Load8U { mem_addr, offset } => self.exec_mem_load::<u8, 1, _>(|v| v as i32, *mem_addr, *offset)?,
            I32Load16S { mem_addr, offset } => self.exec_mem_load::<i16, 2, _>(|v| v as i32, *mem_addr, *offset)?,
            I32Load16U { mem_addr, offset } => self.exec_mem_load::<u16, 2, _>(|v| v as i32, *mem_addr, *offset)?,
            I64Load8S { mem_addr, offset } => self.exec_mem_load::<i8, 1, _>(|v| v as i64, *mem_addr, *offset)?,
            I64Load8U { mem_addr, offset } => self.exec_mem_load::<u8, 1, _>(|v| v as i64, *mem_addr, *offset)?,
            I64Load16S { mem_addr, offset } => self.exec_mem_load::<i16, 2, _>(|v| v as i64, *mem_addr, *offset)?,
            I64Load16U { mem_addr, offset } => self.exec_mem_load::<u16, 2, _>(|v| v as i64, *mem_addr, *offset)?,
            I64Load32S { mem_addr, offset } => self.exec_mem_load::<i32, 4, _>(|v| v as i64, *mem_addr, *offset)?,
            I64Load32U { mem_addr, offset } => self.exec_mem_load::<u32, 4, _>(|v| v as i64, *mem_addr, *offset)?,

            I64Eqz => self.stack.values.replace_top::<i64, _>(|v| Ok((v == 0) as i32))?,
            I32Eqz => self.stack.values.replace_top::<i32, _>(|v| Ok((v == 0) as i32))?,
            I32Eq => self.stack.values.calculate::<i32, _>(|a, b| Ok((a == b) as i32))?,
            I64Eq => self.stack.values.calculate::<i64, _>(|a, b| Ok((a == b) as i32))?,
            F32Eq => self.stack.values.calculate::<f32, _>(|a, b| Ok((a == b) as i32))?,
            F64Eq => self.stack.values.calculate::<f64, _>(|a, b| Ok((a == b) as i32))?,

            I32Ne => self.stack.values.calculate::<i32, _>(|a, b| Ok((a != b) as i32))?,
            I64Ne => self.stack.values.calculate::<i64, _>(|a, b| Ok((a != b) as i32))?,
            F32Ne => self.stack.values.calculate::<f32, _>(|a, b| Ok((a != b) as i32))?,
            F64Ne => self.stack.values.calculate::<f64, _>(|a, b| Ok((a != b) as i32))?,

            I32LtS => self.stack.values.calculate::<i32, _>(|a, b| Ok((a < b) as i32))?,
            I64LtS => self.stack.values.calculate::<i64, _>(|a, b| Ok((a < b) as i32))?,
            I32LtU => self.stack.values.calculate::<u32, _>(|a, b| Ok((a < b) as i32))?,
            I64LtU => self.stack.values.calculate::<u64, _>(|a, b| Ok((a < b) as i32))?,
            F32Lt => self.stack.values.calculate::<f32, _>(|a, b| Ok((a < b) as i32))?,
            F64Lt => self.stack.values.calculate::<f64, _>(|a, b| Ok((a < b) as i32))?,

            I32LeS => self.stack.values.calculate::<i32, _>(|a, b| Ok((a <= b) as i32))?,
            I64LeS => self.stack.values.calculate::<i64, _>(|a, b| Ok((a <= b) as i32))?,
            I32LeU => self.stack.values.calculate::<u32, _>(|a, b| Ok((a <= b) as i32))?,
            I64LeU => self.stack.values.calculate::<u64, _>(|a, b| Ok((a <= b) as i32))?,
            F32Le => self.stack.values.calculate::<f32, _>(|a, b| Ok((a <= b) as i32))?,
            F64Le => self.stack.values.calculate::<f64, _>(|a, b| Ok((a <= b) as i32))?,

            I32GeS => self.stack.values.calculate::<i32, _>(|a, b| Ok((a >= b) as i32))?,
            I64GeS => self.stack.values.calculate::<i64, _>(|a, b| Ok((a >= b) as i32))?,
            I32GeU => self.stack.values.calculate::<u32, _>(|a, b| Ok((a >= b) as i32))?,
            I64GeU => self.stack.values.calculate::<u64, _>(|a, b| Ok((a >= b) as i32))?,
            F32Ge => self.stack.values.calculate::<f32, _>(|a, b| Ok((a >= b) as i32))?,
            F64Ge => self.stack.values.calculate::<f64, _>(|a, b| Ok((a >= b) as i32))?,

            I32GtS => self.stack.values.calculate::<i32, _>(|a, b| Ok((a > b) as i32))?,
            I64GtS => self.stack.values.calculate::<i64, _>(|a, b| Ok((a > b) as i32))?,
            I32GtU => self.stack.values.calculate::<u32, _>(|a, b| Ok((a > b) as i32))?,
            I64GtU => self.stack.values.calculate::<u64, _>(|a, b| Ok((a > b) as i32))?,
            F32Gt => self.stack.values.calculate::<f32, _>(|a, b| Ok((a > b) as i32))?,
            F64Gt => self.stack.values.calculate::<f64, _>(|a, b| Ok((a > b) as i32))?,

            I32Add => self.stack.values.calculate::<i32, _>(|a, b| Ok(a.wrapping_add(b)))?,
            I64Add => self.stack.values.calculate::<i64, _>(|a, b| Ok(a.wrapping_add(b)))?,
            F32Add => self.stack.values.calculate::<f32, _>(|a, b| Ok(a + b))?,
            F64Add => self.stack.values.calculate::<f64, _>(|a, b| Ok(a + b))?,

            I32Sub => self.stack.values.calculate::<i32, _>(|a, b| Ok(a.wrapping_sub(b)))?,
            I64Sub => self.stack.values.calculate::<i64, _>(|a, b| Ok(a.wrapping_sub(b)))?,
            F32Sub => self.stack.values.calculate::<f32, _>(|a, b| Ok(a - b))?,
            F64Sub => self.stack.values.calculate::<f64, _>(|a, b| Ok(a - b))?,

            F32Div => self.stack.values.calculate::<f32, _>(|a, b| Ok(a / b))?,
            F64Div => self.stack.values.calculate::<f64, _>(|a, b| Ok(a / b))?,

            I32Mul => self.stack.values.calculate::<i32, _>(|a, b| Ok(a.wrapping_mul(b)))?,
            I64Mul => self.stack.values.calculate::<i64, _>(|a, b| Ok(a.wrapping_mul(b)))?,
            F32Mul => self.stack.values.calculate::<f32, _>(|a, b| Ok(a * b))?,
            F64Mul => self.stack.values.calculate::<f64, _>(|a, b| Ok(a * b))?,

            // these can trap
            I32DivS => self.stack.values.calculate::<i32, _>(|a, b| {
                if unlikely(b == 0) {
                    return Err(Error::Trap(Trap::DivisionByZero));
                }
                a.checked_div(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
            })?,
            I64DivS => self.stack.values.calculate::<i64, _>(|a, b| {
                if unlikely(b == 0) {
                    return Err(Error::Trap(Trap::DivisionByZero));
                }
                a.checked_div(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
            })?,
            I32DivU => self.stack.values.calculate::<u32, _>(|a, b| {
                if unlikely(b == 0) {
                    return Err(Error::Trap(Trap::DivisionByZero));
                }
                a.checked_div(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
            })?,
            I64DivU => self.stack.values.calculate::<u64, _>(|a, b| {
                if unlikely(b == 0) {
                    return Err(Error::Trap(Trap::DivisionByZero));
                }
                a.checked_div(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
            })?,

            I32RemS => self.stack.values.calculate::<i32, _>(|a, b| {
                if unlikely(b == 0) {
                    return Err(Error::Trap(Trap::DivisionByZero));
                }
                a.checked_wrapping_rem(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
            })?,
            I64RemS => self.stack.values.calculate::<i64, _>(|a, b| {
                if unlikely(b == 0) {
                    return Err(Error::Trap(Trap::DivisionByZero));
                }
                a.checked_wrapping_rem(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
            })?,
            I32RemU => self.stack.values.calculate::<u32, _>(|a, b| {
                if unlikely(b == 0) {
                    return Err(Error::Trap(Trap::DivisionByZero));
                }
                a.checked_wrapping_rem(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
            })?,
            I64RemU => self.stack.values.calculate::<u64, _>(|a, b| {
                if unlikely(b == 0) {
                    return Err(Error::Trap(Trap::DivisionByZero));
                }
                a.checked_wrapping_rem(b).ok_or_else(|| Error::Trap(crate::Trap::IntegerOverflow))
            })?,

            I32And => self.stack.values.calculate::<i32, _>(|a, b| Ok(a & b))?,
            I64And => self.stack.values.calculate::<i64, _>(|a, b| Ok(a & b))?,
            I32Or => self.stack.values.calculate::<i32, _>(|a, b| Ok(a | b))?,
            I64Or => self.stack.values.calculate::<i64, _>(|a, b| Ok(a | b))?,
            I32Xor => self.stack.values.calculate::<i32, _>(|a, b| Ok(a ^ b))?,
            I64Xor => self.stack.values.calculate::<i64, _>(|a, b| Ok(a ^ b))?,
            I32Shl => self.stack.values.calculate::<i32, _>(|a, b| Ok(a.wasm_shl(b)))?,
            I64Shl => self.stack.values.calculate::<i64, _>(|a, b| Ok(a.wasm_shl(b)))?,
            I32ShrS => self.stack.values.calculate::<i32, _>(|a, b| Ok(a.wasm_shr(b)))?,
            I64ShrS => self.stack.values.calculate::<i64, _>(|a, b| Ok(a.wasm_shr(b)))?,
            I32ShrU => self.stack.values.calculate::<u32, _>(|a, b| Ok(a.wasm_shr(b)))?,
            I64ShrU => self.stack.values.calculate::<u64, _>(|a, b| Ok(a.wasm_shr(b)))?,
            I32Rotl => self.stack.values.calculate::<i32, _>(|a, b| Ok(a.wasm_rotl(b)))?,
            I64Rotl => self.stack.values.calculate::<i64, _>(|a, b| Ok(a.wasm_rotl(b)))?,
            I32Rotr => self.stack.values.calculate::<i32, _>(|a, b| Ok(a.wasm_rotr(b)))?,
            I64Rotr => self.stack.values.calculate::<i64, _>(|a, b| Ok(a.wasm_rotr(b)))?,

            I32Clz => self.stack.values.replace_top::<i32, _>(|v| Ok(v.leading_zeros() as i32))?,
            I64Clz => self.stack.values.replace_top::<i64, _>(|v| Ok(v.leading_zeros() as i64))?,
            I32Ctz => self.stack.values.replace_top::<i32, _>(|v| Ok(v.trailing_zeros() as i32))?,
            I64Ctz => self.stack.values.replace_top::<i64, _>(|v| Ok(v.trailing_zeros() as i64))?,
            I32Popcnt => self.stack.values.replace_top::<i32, _>(|v| Ok(v.count_ones() as i32))?,
            I64Popcnt => self.stack.values.replace_top::<i64, _>(|v| Ok(v.count_ones() as i64))?,

            F32ConvertI32S => self.stack.values.replace_top::<i32, _>(|v| Ok(v as f32))?,
            F32ConvertI64S => self.stack.values.replace_top::<i64, _>(|v| Ok(v as f32))?,
            F64ConvertI32S => self.stack.values.replace_top::<i32, _>(|v| Ok(v as f64))?,
            F64ConvertI64S => self.stack.values.replace_top::<i64, _>(|v| Ok(v as f64))?,
            F32ConvertI32U => self.stack.values.replace_top::<u32, _>(|v| Ok(v as f32))?,
            F32ConvertI64U => self.stack.values.replace_top::<u64, _>(|v| Ok(v as f32))?,
            F64ConvertI32U => self.stack.values.replace_top::<u32, _>(|v| Ok(v as f64))?,
            F64ConvertI64U => self.stack.values.replace_top::<u64, _>(|v| Ok(v as f64))?,

            I32Extend8S => self.stack.values.replace_top::<i32, _>(|v| Ok((v as i8) as i32))?,
            I32Extend16S => self.stack.values.replace_top::<i32, _>(|v| Ok((v as i16) as i32))?,
            I64Extend8S => self.stack.values.replace_top::<i64, _>(|v| Ok((v as i8) as i64))?,
            I64Extend16S => self.stack.values.replace_top::<i64, _>(|v| Ok((v as i16) as i64))?,
            I64Extend32S => self.stack.values.replace_top::<i64, _>(|v| Ok((v as i32) as i64))?,
            I64ExtendI32U => self.stack.values.replace_top::<u32, _>(|v| Ok(v as i64))?,
            I64ExtendI32S => self.stack.values.replace_top::<i32, _>(|v| Ok(v as i64))?,
            I32WrapI64 => self.stack.values.replace_top::<i64, _>(|v| Ok(v as i32))?,

            F32DemoteF64 => self.stack.values.replace_top::<f64, _>(|v| Ok(v as f32))?,
            F64PromoteF32 => self.stack.values.replace_top::<f32, _>(|v| Ok(v as f64))?,

            F32Abs => self.stack.values.replace_top::<f32, _>(|v| Ok(v.abs()))?,
            F64Abs => self.stack.values.replace_top::<f64, _>(|v| Ok(v.abs()))?,
            F32Neg => self.stack.values.replace_top::<f32, _>(|v| Ok(-v))?,
            F64Neg => self.stack.values.replace_top::<f64, _>(|v| Ok(-v))?,
            F32Ceil => self.stack.values.replace_top::<f32, _>(|v| Ok(v.ceil()))?,
            F64Ceil => self.stack.values.replace_top::<f64, _>(|v| Ok(v.ceil()))?,
            F32Floor => self.stack.values.replace_top::<f32, _>(|v| Ok(v.floor()))?,
            F64Floor => self.stack.values.replace_top::<f64, _>(|v| Ok(v.floor()))?,
            F32Trunc => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc()))?,
            F64Trunc => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc()))?,
            F32Nearest => self.stack.values.replace_top::<f32, _>(|v| Ok(v.tw_nearest()))?,
            F64Nearest => self.stack.values.replace_top::<f64, _>(|v| Ok(v.tw_nearest()))?,
            F32Sqrt => self.stack.values.replace_top::<f32, _>(|v| Ok(v.sqrt()))?,
            F64Sqrt => self.stack.values.replace_top::<f64, _>(|v| Ok(v.sqrt()))?,
            F32Min => self.stack.values.calculate::<f32, _>(|a, b| Ok(a.tw_minimum(b)))?,
            F64Min => self.stack.values.calculate::<f64, _>(|a, b| Ok(a.tw_minimum(b)))?,
            F32Max => self.stack.values.calculate::<f32, _>(|a, b| Ok(a.tw_maximum(b)))?,
            F64Max => self.stack.values.calculate::<f64, _>(|a, b| Ok(a.tw_maximum(b)))?,
            F32Copysign => self.stack.values.calculate::<f32, _>(|a, b| Ok(a.copysign(b)))?,
            F64Copysign => self.stack.values.calculate::<f64, _>(|a, b| Ok(a.copysign(b)))?,

            // no-op instructions since types are erased at runtime
            I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64 => {}

            I32TruncF32S => checked_conv_float!(f32, i32, self),
            I32TruncF64S => checked_conv_float!(f64, i32, self),
            I32TruncF32U => checked_conv_float!(f32, u32, i32, self),
            I32TruncF64U => checked_conv_float!(f64, u32, i32, self),
            I64TruncF32S => checked_conv_float!(f32, i64, self),
            I64TruncF64S => checked_conv_float!(f64, i64, self),
            I64TruncF32U => checked_conv_float!(f32, u64, i64, self),
            I64TruncF64U => checked_conv_float!(f64, u64, i64, self),

            TableGet(table_idx) => self.exec_table_get(*table_idx)?,
            TableSet(table_idx) => self.exec_table_set(*table_idx)?,
            TableSize(table_idx) => self.exec_table_size(*table_idx)?,
            TableInit(elem_idx, table_idx) => self.exec_table_init(*elem_idx, *table_idx)?,
            TableGrow(table_idx) => self.exec_table_grow(*table_idx)?,
            TableFill(table_idx) => self.exec_table_fill(*table_idx)?,

            I32TruncSatF32S => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc() as i32))?,
            I32TruncSatF32U => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc() as u32))?,
            I32TruncSatF64S => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc() as i32))?,
            I32TruncSatF64U => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc() as u32))?,
            I64TruncSatF32S => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc() as i64))?,
            I64TruncSatF32U => self.stack.values.replace_top::<f32, _>(|v| Ok(v.trunc() as u64))?,
            I64TruncSatF64S => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc() as i64))?,
            I64TruncSatF64U => self.stack.values.replace_top::<f64, _>(|v| Ok(v.trunc() as u64))?,
            // custom instructions
            // LocalGet2(a, b) => self.exec_local_get2(*a, *b),
            // LocalGet3(a, b, c) => self.exec_local_get3(*a, *b, *c),
            // LocalTeeGet(a, b) => self.exec_local_tee_get(*a, *b)?,
            // LocalGetSet(a, b) => self.exec_local_get_set(*a, *b),
            // I64XorConstRotl(rotate_by) => self.exec_i64_xor_const_rotl(*rotate_by)?,
            // I32LocalGetConstAdd(local, val) => self.exec_i32_local_get_const_add(*local, *val),
            // I32ConstStoreLocal { local, const_i32, offset, mem_addr } => {
            //     self.exec_i32_const_store_local(*local, *const_i32, *offset, *mem_addr)?
            // }
            // I32StoreLocal { local_a, local_b, offset, mem_addr } => {
            //     self.exec_i32_store_local(*local_a, *local_b, *offset, *mem_addr)?
            // }
        };

        self.cf.incr_instr_ptr();
        Ok(ControlFlow::Continue(()))
    }

    fn exec_noop(&self) {}
    #[cold]
    fn exec_unreachable(&self) -> Result<()> {
        Err(Error::Trap(Trap::Unreachable))
    }

    fn exec_call(&mut self, wasm_func: Rc<WasmFunction>, owner: ModuleInstanceAddr) -> Result<ControlFlow<()>> {
        let params = self.stack.values.pop_many_raw(&wasm_func.ty.params)?;
        let new_call_frame =
            CallFrame::new_raw(wasm_func, owner, params.into_iter().rev(), self.stack.blocks.len() as u32);
        self.cf.incr_instr_ptr(); // skip the call instruction
        self.stack.call_stack.push(core::mem::replace(&mut self.cf, new_call_frame))?;
        self.module.swap_with(self.cf.module_addr(), self.store);
        Ok(ControlFlow::Continue(()))
    }
    fn exec_call_direct(&mut self, v: u32) -> Result<ControlFlow<()>> {
        let func_inst = self.store.get_func(self.module.resolve_func_addr(v)?)?;
        let wasm_func = match &func_inst.func {
            crate::Function::Wasm(wasm_func) => wasm_func,
            crate::Function::Host(host_func) => {
                let func = &host_func.clone();
                let params = self.stack.values.pop_params(&host_func.ty.params)?;
                let res = (func.func)(FuncContext { store: self.store, module_addr: self.module.id() }, &params)?;
                self.stack.values.extend_from_wasmvalues(&res);
                self.cf.incr_instr_ptr();
                return Ok(ControlFlow::Continue(()));
            }
        };

        self.exec_call(wasm_func.clone(), func_inst._owner)
    }
    fn exec_call_indirect(&mut self, type_addr: u32, table_addr: u32) -> Result<ControlFlow<()>> {
        // verify that the table is of the right type, this should be validated by the parser already
        let func_ref = {
            let table = self.store.get_table(self.module.resolve_table_addr(table_addr)?)?;
            let table_idx: u32 = self.stack.values.pop::<i32>()? as u32;
            let table = table.borrow();
            assert!(table.kind.element_type == ValType::RefFunc, "table is not of type funcref");
            table
                .get(table_idx)
                .map_err(|_| Error::Trap(Trap::UndefinedElement { index: table_idx as usize }))?
                .addr()
                .ok_or(Trap::UninitializedElement { index: table_idx as usize })?
        };

        let func_inst = self.store.get_func(func_ref)?;
        let call_ty = self.module.func_ty(type_addr);
        let wasm_func = match &func_inst.func {
            crate::Function::Wasm(f) => f,
            crate::Function::Host(host_func) => {
                if unlikely(host_func.ty != *call_ty) {
                    return Err(Error::Trap(Trap::IndirectCallTypeMismatch {
                        actual: host_func.ty.clone(),
                        expected: call_ty.clone(),
                    }));
                }

                let host_func = host_func.clone();
                let params = self.stack.values.pop_params(&host_func.ty.params)?;
                let res = (host_func.func)(FuncContext { store: self.store, module_addr: self.module.id() }, &params)?;
                self.stack.values.extend_from_wasmvalues(&res);
                self.cf.incr_instr_ptr();
                return Ok(ControlFlow::Continue(()));
            }
        };

        if wasm_func.ty == *call_ty {
            return self.exec_call(wasm_func.clone(), func_inst._owner);
        }

        cold();
        Err(Trap::IndirectCallTypeMismatch { actual: wasm_func.ty.clone(), expected: call_ty.clone() }.into())
    }

    fn exec_if(
        &mut self,
        else_offset: u32,
        end_offset: u32,
        (params, results): (StackHeight, StackHeight),
    ) -> Result<()> {
        // truthy value is on the top of the stack, so enter the then block
        if self.stack.values.pop::<i32>()? != 0 {
            self.enter_block(self.cf.instr_ptr(), end_offset, BlockType::If, (params, results));
            return Ok(());
        }

        // falsy value is on the top of the stack
        if else_offset == 0 {
            *self.cf.instr_ptr_mut() += end_offset as usize;
            return Ok(());
        }

        let old = self.cf.instr_ptr();
        *self.cf.instr_ptr_mut() += else_offset as usize;
        self.enter_block(old + else_offset as usize, end_offset - else_offset, BlockType::Else, (params, results));
        Ok(())
    }
    fn exec_else(&mut self, end_offset: u32) -> Result<()> {
        self.exec_end_block()?;
        *self.cf.instr_ptr_mut() += end_offset as usize;
        Ok(())
    }
    fn resolve_functype(&self, idx: u32) -> (StackHeight, StackHeight) {
        let ty = self.module.func_ty(idx);
        ((&*ty.params).into(), (&*ty.results).into())
    }
    fn enter_block(
        &mut self,
        instr_ptr: usize,
        end_instr_offset: u32,
        ty: BlockType,
        (params, results): (StackHeight, StackHeight),
    ) {
        self.stack.blocks.push(BlockFrame {
            instr_ptr,
            end_instr_offset,
            stack_ptr: self.stack.values.height(),
            results,
            params,
            ty,
        });
    }
    fn exec_br(&mut self, to: u32) -> Result<ControlFlow<()>> {
        if self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks).is_none() {
            return self.exec_return();
        }

        self.cf.incr_instr_ptr();
        Ok(ControlFlow::Continue(()))
    }
    fn exec_br_if(&mut self, to: u32) -> Result<ControlFlow<()>> {
        if self.stack.values.pop::<i32>()? != 0
            && self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks).is_none()
        {
            return self.exec_return();
        }
        self.cf.incr_instr_ptr();
        Ok(ControlFlow::Continue(()))
    }
    fn exec_brtable(&mut self, default: u32, len: u32) -> Result<ControlFlow<()>> {
        let start = self.cf.instr_ptr() + 1;
        let end = start + len as usize;
        if end > self.cf.instructions().len() {
            return Err(Error::Other(format!("br_table out of bounds: {} >= {}", end, self.cf.instructions().len())));
        }

        let idx = self.stack.values.pop::<i32>()?;
        let to = match self.cf.instructions()[start..end].get(idx as usize) {
            None => default,
            Some(Instruction::BrLabel(to)) => *to,
            _ => return Err(Error::Other("br_table with invalid label".to_string())),
        };

        if self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks).is_none() {
            return self.exec_return();
        }

        self.cf.incr_instr_ptr();
        Ok(ControlFlow::Continue(()))
    }
    fn exec_return(&mut self) -> Result<ControlFlow<()>> {
        let old = self.cf.block_ptr();
        match self.stack.call_stack.pop() {
            None => return Ok(ControlFlow::Break(())),
            Some(cf) => self.cf = cf,
        }

        if old > self.cf.block_ptr() {
            self.stack.blocks.truncate(old);
        }

        self.module.swap_with(self.cf.module_addr(), self.store);
        Ok(ControlFlow::Continue(()))
    }
    fn exec_end_block(&mut self) -> Result<()> {
        let block = self.stack.blocks.pop()?;
        self.stack.values.truncate_keep(&block.stack_ptr, &block.results);
        Ok(())
    }

    fn exec_global_get(&mut self, global_index: u32) -> Result<()> {
        self.stack.values.push_dyn(self.store.get_global_val(self.module.resolve_global_addr(global_index)?)?);
        Ok(())
    }
    fn exec_global_set<T: InternalValue>(&mut self, global_index: u32) -> Result<()>
    where
        TinyWasmValue: From<T>,
    {
        self.store.set_global_val(self.module.resolve_global_addr(global_index)?, self.stack.values.pop::<T>()?.into())
    }
    fn exec_ref_is_null(&mut self) -> Result<()> {
        let is_null = self.stack.values.pop::<ValueRef>()?.is_none() as i32;
        self.stack.values.push::<i32>(is_null);
        Ok(())
    }

    fn exec_memory_size(&mut self, addr: u32) -> Result<()> {
        let mem = self.store.get_mem(self.module.resolve_mem_addr(addr)?)?;
        self.stack.values.push::<i32>(mem.borrow().page_count() as i32);
        Ok(())
    }
    fn exec_memory_grow(&mut self, addr: u32) -> Result<()> {
        let mut mem = self.store.get_mem(self.module.resolve_mem_addr(addr)?)?.borrow_mut();
        let prev_size = mem.page_count() as i32;
        let pages_delta = self.stack.values.pop::<i32>()?;
        self.stack.values.push::<i32>(match mem.grow(pages_delta) {
            Some(_) => prev_size,
            None => -1,
        });
        Ok(())
    }

    fn exec_memory_copy(&mut self, from: u32, to: u32) -> Result<()> {
        let size = self.stack.values.pop::<i32>()?;
        let src = self.stack.values.pop::<i32>()?;
        let dst = self.stack.values.pop::<i32>()?;

        if from == to {
            let mut mem_from = self.store.get_mem(self.module.resolve_mem_addr(from)?)?.borrow_mut();
            // copy within the same memory
            mem_from.copy_within(dst as usize, src as usize, size as usize)?;
        } else {
            // copy between two memories
            let mem_from = self.store.get_mem(self.module.resolve_mem_addr(from)?)?.borrow();
            let mut mem_to = self.store.get_mem(self.module.resolve_mem_addr(to)?)?.borrow_mut();
            mem_to.copy_from_slice(dst as usize, mem_from.load(src as usize, size as usize)?)?;
        }
        Ok(())
    }
    fn exec_memory_fill(&mut self, addr: u32) -> Result<()> {
        let size = self.stack.values.pop::<i32>()?;
        let val = self.stack.values.pop::<i32>()?;
        let dst = self.stack.values.pop::<i32>()?;

        let mem = self.store.get_mem(self.module.resolve_mem_addr(addr)?)?;
        mem.borrow_mut().fill(dst as usize, size as usize, val as u8)?;
        Ok(())
    }
    fn exec_memory_init(&mut self, data_index: u32, mem_index: u32) -> Result<()> {
        let size = self.stack.values.pop::<i32>()?; // n
        let offset = self.stack.values.pop::<i32>()?; // s
        let dst = self.stack.values.pop::<i32>()?; // d

        let data = self.store.get_data(self.module.resolve_data_addr(data_index)?)?;
        let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_index)?)?;

        let data_len = data.data.as_ref().map(|d| d.len()).unwrap_or(0);

        if unlikely(((size + offset) as usize > data_len) || ((dst + size) as usize > mem.borrow().len())) {
            return Err(Trap::MemoryOutOfBounds { offset: offset as usize, len: size as usize, max: data_len }.into());
        }

        if size == 0 {
            return Ok(());
        }

        let data = match &data.data {
            Some(data) => data,
            None => return Err(Trap::MemoryOutOfBounds { offset: 0, len: 0, max: 0 }.into()),
        };

        mem.borrow_mut().store(dst as usize, size as usize, &data[offset as usize..((offset + size) as usize)])?;
        Ok(())
    }
    fn exec_data_drop(&mut self, data_index: u32) -> Result<()> {
        self.store.get_data_mut(self.module.resolve_data_addr(data_index)?).map(|d| d.drop())
    }
    fn exec_elem_drop(&mut self, elem_index: u32) -> Result<()> {
        self.store.get_elem_mut(self.module.resolve_elem_addr(elem_index)?).map(|e| e.drop())
    }
    fn exec_table_copy(&mut self, from: u32, to: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop::<i32>()?;
        let src: i32 = self.stack.values.pop::<i32>()?;
        let dst: i32 = self.stack.values.pop::<i32>()?;

        if from == to {
            let mut table_from = self.store.get_table(self.module.resolve_table_addr(from)?)?.borrow_mut();
            // copy within the same memory
            table_from.copy_within(dst as usize, src as usize, size as usize)?;
        } else {
            // copy between two memories
            let table_from = self.store.get_table(self.module.resolve_table_addr(from)?)?.borrow();
            let mut table_to = self.store.get_table(self.module.resolve_table_addr(to)?)?.borrow_mut();
            table_to.copy_from_slice(dst as usize, table_from.load(src as usize, size as usize)?)?;
        }
        Ok(())
    }

    fn exec_mem_load<LOAD: MemLoadable<LOAD_SIZE>, const LOAD_SIZE: usize, TARGET: InternalValue>(
        &mut self,
        cast: fn(LOAD) -> TARGET,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
    ) -> Result<()> {
        let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_addr)?)?;
        let val = self.stack.values.pop::<i32>()? as u64;
        let Some(Ok(addr)) = offset.checked_add(val).map(|a| a.try_into()) else {
            cold();
            return Err(Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset: offset as usize,
                len: LOAD_SIZE,
                max: mem.borrow().max_pages(),
            }));
        };
        let val = mem.borrow().load_as::<LOAD_SIZE, LOAD>(addr)?;
        self.stack.values.push(cast(val));
        Ok(())
    }
    fn exec_mem_store<T: MemStorable<N>, const N: usize>(
        &mut self,
        val: T,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
    ) -> Result<()> {
        let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_addr)?)?;
        let val = val.to_mem_bytes();
        let addr = self.stack.values.pop::<i32>()? as u64;
        mem.borrow_mut().store((offset + addr) as usize, val.len(), &val)?;
        Ok(())
    }

    fn exec_table_get(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index)?)?;
        let idx: i32 = self.stack.values.pop::<i32>()?;
        let v = table.borrow().get_wasm_val(idx as u32)?;
        self.stack.values.push_dyn(v.into());
        Ok(())
    }
    fn exec_table_set(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index)?)?;
        let val = self.stack.values.pop::<ValueRef>()?;
        let idx = self.stack.values.pop::<i32>()? as u32;
        table.borrow_mut().set(idx, val.into())?;
        Ok(())
    }
    fn exec_table_size(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index)?)?;
        self.stack.values.push_dyn(table.borrow().size().into());
        Ok(())
    }
    fn exec_table_init(&mut self, elem_index: u32, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index)?)?;
        let table_len = table.borrow().size();
        let elem = self.store.get_elem(self.module.resolve_elem_addr(elem_index)?)?;
        let elem_len = elem.items.as_ref().map(|items| items.len()).unwrap_or(0);

        let size: i32 = self.stack.values.pop::<i32>()?; // n
        let offset: i32 = self.stack.values.pop::<i32>()?; // s
        let dst: i32 = self.stack.values.pop::<i32>()?; // d

        if unlikely(((size + offset) as usize > elem_len) || ((dst + size) > table_len)) {
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

        table.borrow_mut().init(self.module.func_addrs(), dst, &items[offset as usize..(offset + size) as usize])?;
        Ok(())
    }
    fn exec_table_grow(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index)?)?;
        let sz = table.borrow().size();

        let n = self.stack.values.pop::<i32>()?;
        let val = self.stack.values.pop::<ValueRef>()?;

        match table.borrow_mut().grow(n, val.into()) {
            Ok(_) => self.stack.values.push_dyn(sz.into()),
            Err(_) => self.stack.values.push_dyn((-1_i32).into()),
        }

        Ok(())
    }
    fn exec_table_fill(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index)?)?;

        let n = self.stack.values.pop::<i32>()?;
        let val = self.stack.values.pop::<ValueRef>()?;
        let i = self.stack.values.pop::<i32>()?;

        if unlikely(i + n > table.borrow().size()) {
            return Err(Error::Trap(Trap::TableOutOfBounds {
                offset: i as usize,
                len: n as usize,
                max: table.borrow().size() as usize,
            }));
        }

        if n == 0 {
            return Ok(());
        }

        table.borrow_mut().fill(self.module.func_addrs(), i as usize, n as usize, val.into())?;
        Ok(())
    }
}
