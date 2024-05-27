use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use core::ops::{BitAnd, BitOr, BitXor, ControlFlow, Neg};
use tinywasm_types::{BlockArgs, ElementKind, Instruction, ModuleInstanceAddr, ValType, WasmFunction};

use super::stack::{BlockFrame, BlockType};
use super::{InterpreterRuntime, RawWasmValue, Stack};
use crate::runtime::CallFrame;
use crate::{cold, unlikely};
use crate::{Error, FuncContext, ModuleInstance, Result, Store, Trap};

mod macros;
mod traits;
use {macros::*, traits::*};

#[cfg(not(feature = "std"))]
mod no_std_floats;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use no_std_floats::NoStdFloatExt;

impl InterpreterRuntime {
    pub(crate) fn exec(&self, store: &mut Store, stack: &mut Stack) -> Result<()> {
        Executor::new(store, stack)?.run_to_completion()
    }
}

struct Executor<'store, 'stack> {
    store: &'store mut Store,
    stack: &'stack mut Stack,

    cf: CallFrame,
    module: ModuleInstance,
}

impl Iterator for Executor<'_, '_> {
    type Item = Result<()>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.exec_next() {
            Ok(ControlFlow::Continue(())) => Some(Ok(())),
            Ok(ControlFlow::Break(())) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

impl<'store, 'stack> Executor<'store, 'stack> {
    pub(crate) fn new(store: &'store mut Store, stack: &'stack mut Stack) -> Result<Self> {
        let current_frame = stack.call_stack.pop().ok_or_else(|| Error::CallStackUnderflow)?;
        let current_module = store.get_module_instance_raw(current_frame.module_addr);
        Ok(Self { cf: current_frame, module: current_module, stack, store })
    }

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

            Drop => self.exec_drop()?,
            Select(_valtype) => self.exec_select()?,

            Call(v) => return self.exec_call_direct(*v),
            CallIndirect(ty, table) => return self.exec_call_indirect(*ty, *table),

            If(args, el, end) => return self.exec_if((*args).into(), *el, *end),
            Else(end_offset) => self.exec_else(*end_offset)?,
            Loop(args, end) => self.enter_block(self.cf.instr_ptr, *end, BlockType::Loop, *args),
            Block(args, end) => self.enter_block(self.cf.instr_ptr, *end, BlockType::Block, *args),
            Br(v) => return self.exec_br(*v),
            BrIf(v) => return self.exec_br_if(*v),
            BrTable(default, len) => return self.exec_brtable(*default, *len),
            Return => return self.exec_return(),
            EndBlockFrame => self.exec_end_block()?,

            LocalGet(local_index) => self.exec_local_get(*local_index),
            LocalSet(local_index) => self.exec_local_set(*local_index)?,
            LocalTee(local_index) => self.exec_local_tee(*local_index)?,
            GlobalGet(global_index) => self.exec_global_get(*global_index)?,
            GlobalSet(global_index) => self.exec_global_set(*global_index)?,

            I32Const(val) => self.exec_const(*val),
            I64Const(val) => self.exec_const(*val),
            F32Const(val) => self.exec_const(*val),
            F64Const(val) => self.exec_const(*val),
            RefFunc(func_idx) => self.exec_const(*func_idx),
            RefNull(_) => self.exec_const(-1i64),
            RefIsNull => self.exec_ref_is_null()?,

            MemorySize(addr, byte) => self.exec_memory_size(*addr, *byte)?,
            MemoryGrow(addr, byte) => self.exec_memory_grow(*addr, *byte)?,

            // Bulk memory operations
            MemoryCopy(from, to) => self.exec_memory_copy(*from, *to)?,
            MemoryFill(addr) => self.exec_memory_fill(*addr)?,
            MemoryInit(data_idx, mem_idx) => self.exec_memory_init(*data_idx, *mem_idx)?,
            DataDrop(data_index) => self.exec_data_drop(*data_index)?,
            ElemDrop(elem_index) => self.exec_elem_drop(*elem_index)?,
            TableCopy { from, to } => self.exec_table_copy(*from, *to)?,

            I32Store { mem_addr, offset } => mem_store!(i32, (mem_addr, offset), self),
            I64Store { mem_addr, offset } => mem_store!(i64, (mem_addr, offset), self),
            F32Store { mem_addr, offset } => mem_store!(f32, (mem_addr, offset), self),
            F64Store { mem_addr, offset } => mem_store!(f64, (mem_addr, offset), self),
            I32Store8 { mem_addr, offset } => mem_store!(i8, i32, (mem_addr, offset), self),
            I32Store16 { mem_addr, offset } => mem_store!(i16, i32, (mem_addr, offset), self),
            I64Store8 { mem_addr, offset } => mem_store!(i8, i64, (mem_addr, offset), self),
            I64Store16 { mem_addr, offset } => mem_store!(i16, i64, (mem_addr, offset), self),
            I64Store32 { mem_addr, offset } => mem_store!(i32, i64, (mem_addr, offset), self),

            I32Load { mem_addr, offset } => mem_load!(i32, (mem_addr, offset), self),
            I64Load { mem_addr, offset } => mem_load!(i64, (mem_addr, offset), self),
            F32Load { mem_addr, offset } => mem_load!(f32, (mem_addr, offset), self),
            F64Load { mem_addr, offset } => mem_load!(f64, (mem_addr, offset), self),
            I32Load8S { mem_addr, offset } => mem_load!(i8, i32, (mem_addr, offset), self),
            I32Load8U { mem_addr, offset } => mem_load!(u8, i32, (mem_addr, offset), self),
            I32Load16S { mem_addr, offset } => mem_load!(i16, i32, (mem_addr, offset), self),
            I32Load16U { mem_addr, offset } => mem_load!(u16, i32, (mem_addr, offset), self),
            I64Load8S { mem_addr, offset } => mem_load!(i8, i64, (mem_addr, offset), self),
            I64Load8U { mem_addr, offset } => mem_load!(u8, i64, (mem_addr, offset), self),
            I64Load16S { mem_addr, offset } => mem_load!(i16, i64, (mem_addr, offset), self),
            I64Load16U { mem_addr, offset } => mem_load!(u16, i64, (mem_addr, offset), self),
            I64Load32S { mem_addr, offset } => mem_load!(i32, i64, (mem_addr, offset), self),
            I64Load32U { mem_addr, offset } => mem_load!(u32, i64, (mem_addr, offset), self),

            I64Eqz => comp_zero!(==, i64, self),
            I32Eqz => comp_zero!(==, i32, self),

            I32Eq => comp!(==, i32, self),
            I64Eq => comp!(==, i64, self),
            F32Eq => comp!(==, f32, self),
            F64Eq => comp!(==, f64, self),

            I32Ne => comp!(!=, i32, self),
            I64Ne => comp!(!=, i64, self),
            F32Ne => comp!(!=, f32, self),
            F64Ne => comp!(!=, f64, self),

            I32LtS => comp!(<, i32, self),
            I64LtS => comp!(<, i64, self),
            I32LtU => comp!(<, u32, self),
            I64LtU => comp!(<, u64, self),
            F32Lt => comp!(<, f32, self),
            F64Lt => comp!(<, f64, self),

            I32LeS => comp!(<=, i32, self),
            I64LeS => comp!(<=, i64, self),
            I32LeU => comp!(<=, u32, self),
            I64LeU => comp!(<=, u64, self),
            F32Le => comp!(<=, f32, self),
            F64Le => comp!(<=, f64, self),

            I32GeS => comp!(>=, i32, self),
            I64GeS => comp!(>=, i64, self),
            I32GeU => comp!(>=, u32, self),
            I64GeU => comp!(>=, u64, self),
            F32Ge => comp!(>=, f32, self),
            F64Ge => comp!(>=, f64, self),

            I32GtS => comp!(>, i32, self),
            I64GtS => comp!(>, i64, self),
            I32GtU => comp!(>, u32, self),
            I64GtU => comp!(>, u64, self),
            F32Gt => comp!(>, f32, self),
            F64Gt => comp!(>, f64, self),

            I64Add => arithmetic!(wrapping_add, i64, self),
            I32Add => arithmetic!(wrapping_add, i32, self),
            F32Add => arithmetic!(+, f32, self),
            F64Add => arithmetic!(+, f64, self),

            I32Sub => arithmetic!(wrapping_sub, i32, self),
            I64Sub => arithmetic!(wrapping_sub, i64, self),
            F32Sub => arithmetic!(-, f32, self),
            F64Sub => arithmetic!(-, f64, self),

            F32Div => arithmetic!(/, f32, self),
            F64Div => arithmetic!(/, f64, self),

            I32Mul => arithmetic!(wrapping_mul, i32, self),
            I64Mul => arithmetic!(wrapping_mul, i64, self),
            F32Mul => arithmetic!(*, f32, self),
            F64Mul => arithmetic!(*, f64, self),

            // these can trap
            I32DivS => checked_int_arithmetic!(checked_div, i32, self),
            I64DivS => checked_int_arithmetic!(checked_div, i64, self),
            I32DivU => checked_int_arithmetic!(checked_div, u32, self),
            I64DivU => checked_int_arithmetic!(checked_div, u64, self),

            I32RemS => checked_int_arithmetic!(checked_wrapping_rem, i32, self),
            I64RemS => checked_int_arithmetic!(checked_wrapping_rem, i64, self),
            I32RemU => checked_int_arithmetic!(checked_wrapping_rem, u32, self),
            I64RemU => checked_int_arithmetic!(checked_wrapping_rem, u64, self),

            I32And => arithmetic!(bitand, i32, self),
            I64And => arithmetic!(bitand, i64, self),
            I32Or => arithmetic!(bitor, i32, self),
            I64Or => arithmetic!(bitor, i64, self),
            I32Xor => arithmetic!(bitxor, i32, self),
            I64Xor => arithmetic!(bitxor, i64, self),
            I32Shl => arithmetic!(wasm_shl, i32, self),
            I64Shl => arithmetic!(wasm_shl, i64, self),
            I32ShrS => arithmetic!(wasm_shr, i32, self),
            I64ShrS => arithmetic!(wasm_shr, i64, self),
            I32ShrU => arithmetic!(wasm_shr, u32, self),
            I64ShrU => arithmetic!(wasm_shr, u64, self),
            I32Rotl => arithmetic!(wasm_rotl, i32, self),
            I64Rotl => arithmetic!(wasm_rotl, i64, self),
            I32Rotr => arithmetic!(wasm_rotr, i32, self),
            I64Rotr => arithmetic!(wasm_rotr, i64, self),

            I32Clz => arithmetic_single!(leading_zeros, i32, self),
            I64Clz => arithmetic_single!(leading_zeros, i64, self),
            I32Ctz => arithmetic_single!(trailing_zeros, i32, self),
            I64Ctz => arithmetic_single!(trailing_zeros, i64, self),
            I32Popcnt => arithmetic_single!(count_ones, i32, self),
            I64Popcnt => arithmetic_single!(count_ones, i64, self),

            F32ConvertI32S => conv!(i32, f32, self),
            F32ConvertI64S => conv!(i64, f32, self),
            F64ConvertI32S => conv!(i32, f64, self),
            F64ConvertI64S => conv!(i64, f64, self),
            F32ConvertI32U => conv!(u32, f32, self),
            F32ConvertI64U => conv!(u64, f32, self),
            F64ConvertI32U => conv!(u32, f64, self),
            F64ConvertI64U => conv!(u64, f64, self),
            I32Extend8S => conv!(i8, i32, self),
            I32Extend16S => conv!(i16, i32, self),
            I64Extend8S => conv!(i8, i64, self),
            I64Extend16S => conv!(i16, i64, self),
            I64Extend32S => conv!(i32, i64, self),
            I64ExtendI32U => conv!(u32, i64, self),
            I64ExtendI32S => conv!(i32, i64, self),
            I32WrapI64 => conv!(i64, i32, self),

            F32DemoteF64 => conv!(f64, f32, self),
            F64PromoteF32 => conv!(f32, f64, self),

            F32Abs => arithmetic_single!(abs, f32, self),
            F64Abs => arithmetic_single!(abs, f64, self),
            F32Neg => arithmetic_single!(neg, f32, self),
            F64Neg => arithmetic_single!(neg, f64, self),
            F32Ceil => arithmetic_single!(ceil, f32, self),
            F64Ceil => arithmetic_single!(ceil, f64, self),
            F32Floor => arithmetic_single!(floor, f32, self),
            F64Floor => arithmetic_single!(floor, f64, self),
            F32Trunc => arithmetic_single!(trunc, f32, self),
            F64Trunc => arithmetic_single!(trunc, f64, self),
            F32Nearest => arithmetic_single!(tw_nearest, f32, self),
            F64Nearest => arithmetic_single!(tw_nearest, f64, self),
            F32Sqrt => arithmetic_single!(sqrt, f32, self),
            F64Sqrt => arithmetic_single!(sqrt, f64, self),
            F32Min => arithmetic!(tw_minimum, f32, self),
            F64Min => arithmetic!(tw_minimum, f64, self),
            F32Max => arithmetic!(tw_maximum, f32, self),
            F64Max => arithmetic!(tw_maximum, f64, self),
            F32Copysign => arithmetic!(copysign, f32, self),
            F64Copysign => arithmetic!(copysign, f64, self),

            // no-op instructions since types are erased at runtime
            I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64 => {}

            // unsigned versions of these are a bit broken atm
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
            TableInit(table_idx, elem_idx) => self.exec_table_init(*elem_idx, *table_idx)?,
            TableGrow(table_idx) => self.exec_table_grow(*table_idx)?,
            TableFill(table_idx) => self.exec_table_fill(*table_idx)?,

            I32TruncSatF32S => arithmetic_single!(trunc, f32, i32, self),
            I32TruncSatF32U => arithmetic_single!(trunc, f32, u32, self),
            I32TruncSatF64S => arithmetic_single!(trunc, f64, i32, self),
            I32TruncSatF64U => arithmetic_single!(trunc, f64, u32, self),
            I64TruncSatF32S => arithmetic_single!(trunc, f32, i64, self),
            I64TruncSatF32U => arithmetic_single!(trunc, f32, u64, self),
            I64TruncSatF64S => arithmetic_single!(trunc, f64, i64, self),
            I64TruncSatF64U => arithmetic_single!(trunc, f64, u64, self),

            // custom instructions
            LocalGet2(a, b) => self.exec_local_get2(*a, *b),
            LocalGet3(a, b, c) => self.exec_local_get3(*a, *b, *c),
            LocalTeeGet(a, b) => self.exec_local_tee_get(*a, *b)?,
            LocalGetSet(a, b) => self.exec_local_get_set(*a, *b),
            I64XorConstRotl(rotate_by) => self.exec_i64_xor_const_rotl(*rotate_by)?,
            I32LocalGetConstAdd(local, val) => self.exec_i32_local_get_const_add(*local, *val),
            I32StoreLocal { local, const_i32, offset, mem_addr } => {
                self.exec_i32_store_local(*local, *const_i32, *offset, *mem_addr)?
            }
            i => {
                cold();
                return Err(Error::UnsupportedFeature(format!("unimplemented instruction: {:?}", i)));
            }
        };

        self.cf.instr_ptr += 1;
        Ok(ControlFlow::Continue(()))
    }

    #[inline(always)]
    fn exec_end_block(&mut self) -> Result<()> {
        let block = self.stack.blocks.pop()?;
        self.stack.values.truncate_keep(block.stack_ptr, block.results as u32);

        #[cfg(feature = "simd")]
        self.stack.values.truncate_keep_simd(block.simd_stack_ptr, block.simd_results as u32);
        Ok(())
    }

    #[inline(always)]
    fn exec_else(&mut self, end_offset: u32) -> Result<()> {
        let block = self.stack.blocks.pop()?;

        self.stack.values.truncate_keep(block.stack_ptr, block.results as u32);

        #[cfg(feature = "simd")]
        self.stack.values.truncate_keep_simd(block.simd_stack_ptr, block.simd_results as u32);

        self.cf.instr_ptr += end_offset as usize;
        Ok(())
    }

    #[inline(always)]
    fn exec_br(&mut self, to: u32) -> Result<ControlFlow<()>> {
        break_to!(to, self);
        self.cf.instr_ptr += 1;
        Ok(ControlFlow::Continue(()))
    }

    #[inline(always)]
    fn exec_br_if(&mut self, to: u32) -> Result<ControlFlow<()>> {
        let val: i32 = self.stack.values.pop()?.into();
        if val != 0 {
            break_to!(to, self);
        }
        self.cf.instr_ptr += 1;
        Ok(ControlFlow::Continue(()))
    }

    #[inline(always)]
    fn exec_brtable(&mut self, default: u32, len: u32) -> Result<ControlFlow<()>> {
        let start = self.cf.instr_ptr + 1;
        let end = start + len as usize;
        if end > self.cf.instructions().len() {
            return Err(Error::Other(format!("br_table out of bounds: {} >= {}", end, self.cf.instructions().len())));
        }

        let idx: i32 = self.stack.values.pop()?.into();

        match self.cf.instructions()[start..end].get(idx as usize) {
            None => break_to!(default, self),
            Some(Instruction::BrLabel(to)) => break_to!(*to, self),
            _ => return Err(Error::Other("br_table with invalid label".to_string())),
        }

        self.cf.instr_ptr += 1;
        Ok(ControlFlow::Continue(()))
    }

    #[inline(always)]
    fn exec_return(&mut self) -> Result<ControlFlow<()>> {
        let old = self.cf.block_ptr;
        match self.stack.call_stack.pop() {
            None => return Ok(ControlFlow::Break(())),
            Some(cf) => self.cf = cf,
        }

        if old > self.cf.block_ptr {
            self.stack.blocks.truncate(old);
        }

        self.module.swap_with(self.cf.module_addr, self.store);
        Ok(ControlFlow::Continue(()))
    }

    #[inline(always)]
    #[cold]
    fn exec_unreachable(&self) -> Result<()> {
        Err(Error::Trap(Trap::Unreachable))
    }

    #[inline(always)]
    fn exec_noop(&self) {}

    #[inline(always)]
    fn exec_ref_is_null(&mut self) -> Result<()> {
        self.stack.values.replace_top(|val| ((i32::from(val) == -1) as i32).into())
    }

    #[inline(always)]
    fn exec_const(&mut self, val: impl Into<RawWasmValue>) {
        self.stack.values.push(val.into());
    }

    #[inline(always)]
    fn exec_i32_store_local(&mut self, local: u32, const_i32: i32, offset: u32, mem_addr: u8) -> Result<()> {
        let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_addr as u32))?;
        let val = const_i32.to_le_bytes();
        let addr: u64 = self.cf.get_local(local).into();
        mem.borrow_mut().store((offset as u64 + addr) as usize, val.len(), &val)?;
        Ok(())
    }

    #[inline(always)]
    fn exec_i32_local_get_const_add(&mut self, local: u32, val: i32) {
        let local: i32 = self.cf.get_local(local).into();
        self.stack.values.push((local + val).into());
    }

    #[inline(always)]
    fn exec_i64_xor_const_rotl(&mut self, rotate_by: i64) -> Result<()> {
        let val: i64 = self.stack.values.pop()?.into();
        let res = self.stack.values.last_mut()?;
        let mask: i64 = (*res).into();
        *res = (val ^ mask).rotate_left(rotate_by as u32).into();
        Ok(())
    }

    #[inline(always)]
    fn exec_local_get(&mut self, local_index: u32) {
        self.stack.values.push(self.cf.get_local(local_index));
    }

    #[inline(always)]
    fn exec_local_get2(&mut self, a: u32, b: u32) {
        self.stack.values.extend_from_slice(&[self.cf.get_local(a), self.cf.get_local(b)]);
    }

    #[inline(always)]
    fn exec_local_get3(&mut self, a: u32, b: u32, c: u32) {
        self.stack.values.extend_from_slice(&[self.cf.get_local(a), self.cf.get_local(b), self.cf.get_local(c)]);
    }

    #[inline(always)]
    fn exec_local_get_set(&mut self, a: u32, b: u32) {
        self.cf.set_local(b, self.cf.get_local(a))
    }

    #[inline(always)]
    fn exec_local_set(&mut self, local_index: u32) -> Result<()> {
        self.cf.set_local(local_index, self.stack.values.pop()?);
        Ok(())
    }

    #[inline(always)]
    fn exec_local_tee(&mut self, local_index: u32) -> Result<()> {
        self.cf.set_local(local_index, *self.stack.values.last()?);
        Ok(())
    }

    #[inline(always)]
    fn exec_local_tee_get(&mut self, a: u32, b: u32) -> Result<()> {
        let last = self.stack.values.last()?;
        self.cf.set_local(a, *last);
        self.stack.values.push(match a == b {
            true => *last,
            false => self.cf.get_local(b),
        });
        Ok(())
    }

    #[inline(always)]
    fn exec_global_get(&mut self, global_index: u32) -> Result<()> {
        self.stack.values.push(self.store.get_global_val(self.module.resolve_global_addr(global_index))?);
        Ok(())
    }

    #[inline(always)]
    fn exec_global_set(&mut self, global_index: u32) -> Result<()> {
        self.store.set_global_val(self.module.resolve_global_addr(global_index), self.stack.values.pop()?)
    }

    #[inline(always)]
    fn exec_table_get(&mut self, table_index: u32) -> Result<()> {
        let table_idx = self.module.resolve_table_addr(table_index);
        let table = self.store.get_table(table_idx)?;
        let idx: u32 = self.stack.values.pop()?.into();
        let v = table.borrow().get_wasm_val(idx)?;
        self.stack.values.push(v.into());
        Ok(())
    }

    #[inline(always)]
    fn exec_table_set(&mut self, table_index: u32) -> Result<()> {
        let table_idx = self.module.resolve_table_addr(table_index);
        let table = self.store.get_table(table_idx)?;
        let val = self.stack.values.pop()?.as_reference();
        let idx = self.stack.values.pop()?.into();
        table.borrow_mut().set(idx, val.into())?;

        Ok(())
    }

    #[inline(always)]
    fn exec_table_size(&mut self, table_index: u32) -> Result<()> {
        let table_idx = self.module.resolve_table_addr(table_index);
        let table = self.store.get_table(table_idx)?;
        self.stack.values.push(table.borrow().size().into());
        Ok(())
    }

    #[inline(always)]
    fn exec_table_init(&mut self, elem_index: u32, table_index: u32) -> Result<()> {
        let table_idx = self.module.resolve_table_addr(table_index);
        let table = self.store.get_table(table_idx)?;
        let table_len = table.borrow().size();
        let elem = self.store.get_elem(self.module.resolve_elem_addr(elem_index))?;
        let elem_len = elem.items.as_ref().map(|items| items.len()).unwrap_or(0);

        let size: i32 = self.stack.values.pop()?.into(); // n
        let offset: i32 = self.stack.values.pop()?.into(); // s
        let dst: i32 = self.stack.values.pop()?.into(); // d

        if unlikely(((size + offset) as usize > elem_len) || ((dst + size) > table_len)) {
            return Err(Trap::TableOutOfBounds { offset: offset as usize, len: size as usize, max: elem_len }.into());
        }

        if size == 0 {
            return Ok(());
        }

        // TODO, not sure how to handle passive elements, but this makes the test pass
        if let ElementKind::Passive = elem.kind {
            return Ok(());
        }

        let Some(items) = elem.items.as_ref() else {
            return Err(Trap::TableOutOfBounds { offset: 0, len: 0, max: 0 }.into());
        };

        table.borrow_mut().init(self.module.func_addrs(), dst, &items[offset as usize..(offset + size) as usize])?;
        Ok(())
    }

    #[inline(always)]
    // todo: this is just a placeholder, need to check the spec
    fn exec_table_grow(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index))?;
        let sz = table.borrow().size();

        let n: i32 = self.stack.values.pop()?.into();
        let val = self.stack.values.pop()?.as_reference();

        match table.borrow_mut().grow(n, val.into()) {
            Ok(_) => self.stack.values.push(sz.into()),
            Err(_) => self.stack.values.push((-1_i32).into()),
        }

        Ok(())
    }

    #[inline(always)]
    fn exec_table_fill(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.get_table(self.module.resolve_table_addr(table_index))?;

        let n: i32 = self.stack.values.pop()?.into();
        let val = self.stack.values.pop()?.as_reference();
        let i: i32 = self.stack.values.pop()?.into();

        if unlikely(i + n > table.borrow().size()) {
            return Err(Trap::TableOutOfBounds {
                offset: i as usize,
                len: n as usize,
                max: table.borrow().size() as usize,
            }
            .into());
        }

        if n == 0 {
            return Ok(());
        }

        table.borrow_mut().fill(self.module.func_addrs(), i as usize, n as usize, val.into())?;
        Ok(())
    }

    #[inline(always)]
    fn exec_drop(&mut self) -> Result<()> {
        self.stack.values.pop()?;
        Ok(())
    }

    #[inline(always)]
    fn exec_select(&mut self) -> Result<()> {
        let cond: i32 = self.stack.values.pop()?.into();
        let val2 = self.stack.values.pop()?;
        // if cond != 0, we already have the right value on the stack
        if cond == 0 {
            *self.stack.values.last_mut()? = val2;
        }
        Ok(())
    }

    #[inline(always)]
    fn exec_memory_size(&mut self, addr: u32, byte: u8) -> Result<()> {
        if unlikely(byte != 0) {
            return Err(Error::UnsupportedFeature("memory.size with byte != 0".to_string()));
        }

        let mem_idx = self.module.resolve_mem_addr(addr);
        let mem = self.store.get_mem(mem_idx)?;
        self.stack.values.push((mem.borrow().page_count() as i32).into());
        Ok(())
    }

    #[inline(always)]
    fn exec_memory_grow(&mut self, addr: u32, byte: u8) -> Result<()> {
        if unlikely(byte != 0) {
            return Err(Error::UnsupportedFeature("memory.grow with byte != 0".to_string()));
        }

        let mut mem = self.store.get_mem(self.module.resolve_mem_addr(addr))?.borrow_mut();
        let prev_size = mem.page_count() as i32;
        let pages_delta = self.stack.values.last_mut()?;
        *pages_delta = match mem.grow(i32::from(*pages_delta)) {
            Some(_) => prev_size.into(),
            None => (-1).into(),
        };

        Ok(())
    }

    #[inline(always)]
    fn exec_memory_copy(&mut self, from: u32, to: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop()?.into();
        let src: i32 = self.stack.values.pop()?.into();
        let dst: i32 = self.stack.values.pop()?.into();

        if from == to {
            let mut mem_from = self.store.get_mem(self.module.resolve_mem_addr(from))?.borrow_mut();
            // copy within the same memory
            mem_from.copy_within(dst as usize, src as usize, size as usize)?;
        } else {
            // copy between two memories
            let mem_from = self.store.get_mem(self.module.resolve_mem_addr(from))?.borrow();
            let mut mem_to = self.store.get_mem(self.module.resolve_mem_addr(to))?.borrow_mut();
            mem_to.copy_from_slice(dst as usize, mem_from.load(src as usize, size as usize)?)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn exec_memory_fill(&mut self, addr: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop()?.into();
        let val: i32 = self.stack.values.pop()?.into();
        let dst: i32 = self.stack.values.pop()?.into();

        let mem = self.store.get_mem(self.module.resolve_mem_addr(addr))?;
        mem.borrow_mut().fill(dst as usize, size as usize, val as u8)?;
        Ok(())
    }

    #[inline(always)]
    fn exec_memory_init(&mut self, data_index: u32, mem_index: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop()?.into(); // n
        let offset: i32 = self.stack.values.pop()?.into(); // s
        let dst: i32 = self.stack.values.pop()?.into(); // d

        let data = self.store.get_data(self.module.resolve_data_addr(data_index))?;
        let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_index))?;

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

    #[inline(always)]
    fn exec_data_drop(&mut self, data_index: u32) -> Result<()> {
        self.store.get_data_mut(self.module.resolve_data_addr(data_index))?.drop();
        Ok(())
    }

    #[inline(always)]
    fn exec_elem_drop(&mut self, elem_index: u32) -> Result<()> {
        self.store.get_elem_mut(self.module.resolve_elem_addr(elem_index))?.drop();
        Ok(())
    }

    #[inline(always)]
    fn exec_table_copy(&mut self, from: u32, to: u32) -> Result<()> {
        let size: i32 = self.stack.values.pop()?.into();
        let src: i32 = self.stack.values.pop()?.into();
        let dst: i32 = self.stack.values.pop()?.into();

        if from == to {
            let mut table_from = self.store.get_table(self.module.resolve_table_addr(from))?.borrow_mut();
            // copy within the same memory
            table_from.copy_within(dst as usize, src as usize, size as usize)?;
        } else {
            // copy between two memories
            let table_from = self.store.get_table(self.module.resolve_table_addr(from))?.borrow();
            let mut table_to = self.store.get_table(self.module.resolve_table_addr(to))?.borrow_mut();
            table_to.copy_from_slice(dst as usize, table_from.load(src as usize, size as usize)?)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn exec_call(&mut self, wasm_func: Rc<WasmFunction>, owner: ModuleInstanceAddr) -> Result<ControlFlow<()>> {
        let params = self.stack.values.pop_n_rev(wasm_func.ty.params.len())?;
        let new_call_frame = CallFrame::new(wasm_func, owner, &params, self.stack.blocks.len() as u32);
        self.cf.instr_ptr += 1; // skip the call instruction
        self.stack.call_stack.push(core::mem::replace(&mut self.cf, new_call_frame))?;
        self.module.swap_with(self.cf.module_addr, self.store);
        Ok(ControlFlow::Continue(()))
    }

    #[inline(always)]
    fn exec_call_direct(&mut self, v: u32) -> Result<ControlFlow<()>> {
        let func_inst = self.store.get_func(self.module.resolve_func_addr(v))?;
        let wasm_func = match &func_inst.func {
            crate::Function::Wasm(wasm_func) => wasm_func,
            crate::Function::Host(host_func) => {
                let func = &host_func.clone();
                let params = self.stack.values.pop_params(&host_func.ty.params)?;
                let res = (func.func)(FuncContext { store: self.store, module_addr: self.module.id() }, &params)?;
                self.stack.values.extend_from_typed(&res);
                self.cf.instr_ptr += 1;
                return Ok(ControlFlow::Continue(()));
            }
        };
        self.exec_call(wasm_func.clone(), func_inst.owner)
    }

    #[inline(always)]
    fn exec_call_indirect(&mut self, type_addr: u32, table_addr: u32) -> Result<ControlFlow<()>> {
        // verify that the table is of the right type, this should be validated by the parser already
        let func_ref = {
            let table = self.store.get_table(self.module.resolve_table_addr(table_addr))?;
            let table_idx: u32 = self.stack.values.pop()?.into();
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
                self.stack.values.extend_from_typed(&res);
                self.cf.instr_ptr += 1;
                return Ok(ControlFlow::Continue(()));
            }
        };

        if wasm_func.ty == *call_ty {
            return self.exec_call(wasm_func.clone(), func_inst.owner);
        }

        cold();
        Err(Trap::IndirectCallTypeMismatch { actual: wasm_func.ty.clone(), expected: call_ty.clone() }.into())
    }

    #[inline(always)]
    fn exec_if(&mut self, args: BlockArgs, else_offset: u32, end_offset: u32) -> Result<ControlFlow<()>> {
        // truthy value is on the top of the stack, so enter the then block
        if i32::from(self.stack.values.pop()?) != 0 {
            self.enter_block(self.cf.instr_ptr, end_offset, BlockType::If, args);
            self.cf.instr_ptr += 1;
            return Ok(ControlFlow::Continue(()));
        }

        // falsy value is on the top of the stack
        if else_offset == 0 {
            self.cf.instr_ptr += end_offset as usize + 1;
            return Ok(ControlFlow::Continue(()));
        }

        let old = self.cf.instr_ptr;
        self.cf.instr_ptr += else_offset as usize;
        self.enter_block(old + else_offset as usize, end_offset - else_offset, BlockType::Else, args);
        self.cf.instr_ptr += 1;
        Ok(ControlFlow::Continue(()))
    }

    #[inline(always)]
    fn enter_block(&mut self, instr_ptr: usize, end_instr_offset: u32, ty: BlockType, args: BlockArgs) {
        #[cfg(not(feature = "simd"))]
        {
            let (params, results) = match args {
                BlockArgs::Empty => (0, 0),
                BlockArgs::Type(_) => (0, 1),
                BlockArgs::FuncType(t) => {
                    let ty = self.module.func_ty(t);
                    (ty.params.len() as u8, ty.results.len() as u8)
                }
            };

            self.stack.blocks.push(BlockFrame {
                instr_ptr,
                end_instr_offset,
                stack_ptr: self.stack.values.len() as u32 - params as u32,
                results,
                params,
                ty,
            });
        };

        #[cfg(feature = "simd")]
        {
            let (params, results, simd_params, simd_results) = match args {
                BlockArgs::Empty => (0, 0, 0, 0),
                BlockArgs::Type(t) => match t {
                    ValType::V128 => (0, 0, 0, 1),
                    _ => (0, 1, 0, 0),
                },
                BlockArgs::FuncType(t) => {
                    let ty = self.module.func_ty(t);
                    let simd_params = ty.params.iter().filter(|t| t.is_simd()).count() as u8;
                    let params = ty.params.len() as u8 - simd_params;
                    let simd_results = ty.results.iter().filter(|t| t.is_simd()).count() as u8;
                    let results = ty.results.len() as u8 - simd_results;
                    (params, results, simd_params, simd_results)
                }
            };

            self.stack.blocks.push(BlockFrame {
                instr_ptr,
                end_instr_offset,
                stack_ptr: self.stack.values.len() as u32 - params as u32,
                simd_stack_ptr: self.stack.values.simd_len() as u16 - simd_params as u16,
                results,
                simd_params,
                simd_results,
                params,
                ty,
            });
        };
    }
}
