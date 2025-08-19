#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use super::no_std_floats::NoStdFloatExt;

use alloc::boxed::Box;
use alloc::{format, rc::Rc, string::ToString};
use core::ops::ControlFlow;
use coro::SuspendReason;
use interpreter::simd::exec_next_simd;
use interpreter::stack::CallFrame;
use tinywasm_types::*;

use super::num_helpers::*;
use super::stack::{BlockFrame, BlockType, Stack};
use super::values::*;
use crate::*;

pub(crate) enum ReasonToBreak {
    Errored(Error),
    Suspended(SuspendReason),
    Finished,
}

impl From<ReasonToBreak> for ControlFlow<ReasonToBreak> {
    fn from(value: ReasonToBreak) -> Self {
        ControlFlow::Break(value)
    }
}

#[derive(Debug)]
pub(crate) struct SuspendedHostCoroState {
    pub(crate) coro_state: Box<dyn HostCoroState>,
    // plug into used in store.get_func to get original function
    // can be used for checking returned types
    #[allow(dead_code)] // not implemented yet, but knowing context is useful
    pub(crate) coro_orig_function: u32,
}

#[derive(Debug)]
pub(crate) struct Executor<'store, 'stack> {
    pub(crate) cf: CallFrame,
    pub(crate) module: ModuleInstance,
    pub(crate) suspended_host_coro: Option<SuspendedHostCoroState>,
    pub(crate) store: &'store mut Store,
    pub(crate) stack: &'stack mut Stack,
}

pub(crate) type ExecOutcome = coro::CoroStateResumeResult<()>;

impl<'store, 'stack> Executor<'store, 'stack> {
    pub(crate) fn new(store: &'store mut Store, stack: &'stack mut Stack) -> Result<Self> {
        let current_frame = stack.call_stack.pop().expect("no call frame, this is a bug");
        let current_module = store.get_module_instance_raw(current_frame.module_addr());
        Ok(Self { cf: current_frame, module: current_module, suspended_host_coro: None, stack, store })
    }

    #[inline(always)]
    pub(crate) fn run_to_suspension(&mut self) -> Result<ExecOutcome> {
        loop {
            if let ControlFlow::Break(res) = self.exec_next() {
                return match res {
                    ReasonToBreak::Errored(e) => Err(e),
                    ReasonToBreak::Suspended(suspend_reason) => Ok(ExecOutcome::Suspended(suspend_reason)),
                    ReasonToBreak::Finished => Ok(ExecOutcome::Return(())),
                };
            }
        }
    }

    #[inline(always)]
    pub(crate) fn resume(&mut self, res_arg: ResumeArgument) -> Result<ExecOutcome> {
        if let Some(coro_state) = self.suspended_host_coro.as_mut() {
            let ctx = FuncContext { store: self.store, module_addr: self.module.id() };
            let host_res = coro_state.coro_state.resume(ctx, res_arg)?;
            let res = match host_res {
                CoroStateResumeResult::Return(res) => res,
                CoroStateResumeResult::Suspended(suspend_reason) => {
                    return Ok(ExecOutcome::Suspended(suspend_reason));
                }
            };
            self.stack.values.extend_from_wasmvalues(&res);
            self.suspended_host_coro = None;

            // we don't know how much time we spent in host function
            if let ControlFlow::Break(ReasonToBreak::Suspended(reason)) = self.check_should_suspend() {
                return Ok(ExecOutcome::Suspended(reason));
            }
        }

        loop {
            if let ControlFlow::Break(res) = self.exec_next() {
                return match res {
                    ReasonToBreak::Errored(e) => Err(e),
                    ReasonToBreak::Suspended(suspend_reason) => Ok(ExecOutcome::Suspended(suspend_reason)),
                    ReasonToBreak::Finished => Ok(ExecOutcome::Return(())),
                };
            }
        }
    }

    /// for controlling how long execution spends in wasm
    /// called when execution loops back, because that might happen indefinite amount of times
    /// and before and after function calls, because even without loops or infinite recursion, wasm function calls
    /// can mutliply time spent in execution
    /// execution may not be suspended in the middle of execution the funcion:
    /// so only do it as the last thing or first thing in the intsruction execution
    #[must_use = "If this returns ControlFlow::Break, the caller should propagate it"]
    fn check_should_suspend(&mut self) -> ControlFlow<ReasonToBreak> {
        if let Some(flag) = &self.store.suspend_cond.suspend_flag {
            if flag.load(core::sync::atomic::Ordering::Acquire) {
                return ReasonToBreak::Suspended(SuspendReason::SuspendedFlag).into();
            }
        }

        #[cfg(feature = "std")]
        if let Some(when) = &self.store.suspend_cond.timeout_instant {
            if crate::std::time::Instant::now() >= *when {
                return ReasonToBreak::Suspended(SuspendReason::SuspendedEpoch).into();
            }
        }

        if let Some(mut cb) = self.store.suspend_cond.suspend_cb.take() {
            let should_suspend = matches!(cb(self.store), ControlFlow::Break(()));
            self.store.suspend_cond.suspend_cb = Some(cb); // put it back
            if should_suspend {
                return ReasonToBreak::Suspended(SuspendReason::SuspendedCallback).into();
            }
        }

        ControlFlow::Continue(())
    }

    #[inline(always)]
    fn exec_next(&mut self) -> ControlFlow<ReasonToBreak> {
        use tinywasm_types::Instruction::*;
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

            Call(v) => return self.exec_call_direct(*v),
            CallIndirect(ty, table) => return self.exec_call_indirect(*ty, *table),

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

            Simd(op) => exec_next_simd(self, *op).to_cf()?,
        };

        self.cf.incr_instr_ptr();
        ControlFlow::Continue(())
    }

    #[cold]
    fn exec_unreachable(&self) -> ControlFlow<ReasonToBreak> {
        ReasonToBreak::Errored(Trap::Unreachable.into()).into()
    }

    fn exec_call(&mut self, wasm_func: Rc<WasmFunction>, owner: ModuleInstanceAddr) -> ControlFlow<ReasonToBreak> {
        let locals = self.stack.values.pop_locals(wasm_func.params, wasm_func.locals);
        let new_call_frame = CallFrame::new_raw(wasm_func, owner, locals, self.stack.blocks.len() as u32);
        self.cf.incr_instr_ptr(); // skip the call instruction
        self.stack.call_stack.push(core::mem::replace(&mut self.cf, new_call_frame))?;
        self.module.swap_with(self.cf.module_addr(), self.store);
        ControlFlow::Continue(())
    }
    fn exec_call_host(&mut self, host_func: Rc<HostFunction>, func_ref: u32) -> ControlFlow<ReasonToBreak> {
        let params = self.stack.values.pop_params(&host_func.ty.params);
        let res = host_func.call(FuncContext { store: self.store, module_addr: self.module.id() }, &params).to_cf()?;
        match res {
            PotentialCoroCallResult::Return(res) => {
                self.stack.values.extend_from_wasmvalues(&res);
                self.cf.incr_instr_ptr();
                self.check_should_suspend()?; // who knows how long we've spent in host function
                ControlFlow::Continue(())
            }
            PotentialCoroCallResult::Suspended(suspend_reason, state) => {
                self.suspended_host_coro =
                    Some(SuspendedHostCoroState { coro_state: state, coro_orig_function: func_ref });
                self.cf.incr_instr_ptr();
                ReasonToBreak::Suspended(suspend_reason).into()
            }
        }
    }
    fn exec_call_direct(&mut self, v: u32) -> ControlFlow<ReasonToBreak> {
        self.check_should_suspend()?; // don't commit to function if we should be stopping now
        let func_ref = self.module.resolve_func_addr(v);
        let func_inst = self.store.get_func(func_ref);
        let wasm_func = match &func_inst.func {
            crate::Function::Wasm(wasm_func) => wasm_func,
            crate::Function::Host(host_func) => {
                return self.exec_call_host(host_func.clone(), func_ref);
            }
        };

        self.exec_call(wasm_func.clone(), func_inst.owner)
    }
    fn exec_call_indirect(&mut self, type_addr: u32, table_addr: u32) -> ControlFlow<ReasonToBreak> {
        self.check_should_suspend()?; // check if we should suspend now before commiting to function
                                      // verify that the table is of the right type, this should be validated by the parser already
        let func_ref = {
            let table = self.store.get_table(self.module.resolve_table_addr(table_addr));
            let table_idx: u32 = self.stack.values.pop::<i32>() as u32;
            assert!(table.kind.element_type == ValType::RefFunc, "table is not of type funcref");
            table
                .get(table_idx)
                .map_err(|_| Error::Trap(Trap::UndefinedElement { index: table_idx as usize }))
                .to_cf()?
                .addr()
                .ok_or(Error::Trap(Trap::UninitializedElement { index: table_idx as usize }))
                .to_cf()?
        };

        let func_inst = self.store.get_func(func_ref);
        let call_ty = self.module.func_ty(type_addr);
        let wasm_func = match &func_inst.func {
            crate::Function::Wasm(f) => f,
            crate::Function::Host(host_func) => {
                if unlikely(host_func.ty != *call_ty) {
                    return ReasonToBreak::Errored(
                        Trap::IndirectCallTypeMismatch { actual: host_func.ty.clone(), expected: call_ty.clone() }
                            .into(),
                    )
                    .into();
                }
                return self.exec_call_host(host_func.clone(), func_ref);
            }
        };

        if unlikely(wasm_func.ty != *call_ty) {
            return ReasonToBreak::Errored(
                Trap::IndirectCallTypeMismatch { actual: wasm_func.ty.clone(), expected: call_ty.clone() }.into(),
            )
            .into();
        }

        self.exec_call(wasm_func.clone(), func_inst.owner)
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
    fn exec_br(&mut self, to: u32) -> ControlFlow<ReasonToBreak> {
        let block_ty = self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks);
        if block_ty.is_none() {
            return self.exec_return();
        }

        self.cf.incr_instr_ptr();

        if matches!(block_ty, Some(BlockType::Loop)) {
            self.check_should_suspend()?;
        }
        ControlFlow::Continue(())
    }
    fn exec_br_if(&mut self, to: u32) -> ControlFlow<ReasonToBreak> {
        let should_check_suspend = if self.stack.values.pop::<i32>() != 0 {
            // condition says we should break
            let block_ty = self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks);
            if block_ty.is_none() {
                return self.exec_return();
            }
            matches!(block_ty, Some(BlockType::Loop))
        } else {
            // condition says we shouldn't break
            false
        };

        self.cf.incr_instr_ptr();

        if should_check_suspend {
            self.check_should_suspend()?;
        }
        ControlFlow::Continue(())
    }
    fn exec_brtable(&mut self, default: u32, len: u32) -> ControlFlow<ReasonToBreak> {
        let start = self.cf.instr_ptr() + 1;
        let end = start + len as usize;
        if end > self.cf.instructions().len() {
            return ReasonToBreak::Errored(Error::Other(format!(
                "br_table out of bounds: {} >= {}",
                end,
                self.cf.instructions().len()
            )))
            .into();
        }

        let idx = self.stack.values.pop::<i32>();
        let to = match self.cf.instructions()[start..end].get(idx as usize) {
            None => default,
            Some(Instruction::BrLabel(to)) => *to,
            _ => return ReasonToBreak::Errored(Error::Other("br_table out of bounds".to_string())).into(),
        };

        let block_ty = self.cf.break_to(to, &mut self.stack.values, &mut self.stack.blocks);
        if block_ty.is_none() {
            return self.exec_return();
        }

        self.cf.incr_instr_ptr();

        if matches!(block_ty, Some(BlockType::Loop)) {
            self.check_should_suspend()?;
        }
        ControlFlow::Continue(())
    }
    fn exec_return(&mut self) -> ControlFlow<ReasonToBreak> {
        let old = self.cf.block_ptr();
        match self.stack.call_stack.pop() {
            None => return ReasonToBreak::Finished.into(),
            Some(cf) => self.cf = cf,
        }

        if old > self.cf.block_ptr() {
            self.stack.blocks.truncate(old);
        }

        self.module.swap_with(self.cf.module_addr(), self.store);

        self.check_should_suspend()?;
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
        self.stack.values.push::<i32>(mem.page_count as i32);
    }
    fn exec_memory_grow(&mut self, addr: u32) {
        let mem = self.store.get_mem_mut(self.module.resolve_mem_addr(addr));
        let prev_size = mem.page_count as i32;
        let pages_delta = self.stack.values.pop::<i32>();
        self.stack.values.push::<i32>(match mem.grow(pages_delta) {
            Some(_) => prev_size,
            None => -1,
        });
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
            )?;
        } else {
            // copy between two memories
            let (table_from, table_to) =
                self.store.get_tables_mut(self.module.resolve_table_addr(from), self.module.resolve_table_addr(to))?;
            table_to.copy_from_slice(dst as usize, table_from.load(src as usize, size as usize)?)?;
        }
        Ok(())
    }

    fn exec_mem_load<LOAD: MemLoadable<LOAD_SIZE>, const LOAD_SIZE: usize, TARGET: InternalValue>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        cast: fn(LOAD) -> TARGET,
    ) -> ControlFlow<ReasonToBreak> {
        let mem = self.store.get_mem(self.module.resolve_mem_addr(mem_addr));
        let val = self.stack.values.pop::<i32>() as u64;
        let Some(Ok(addr)) = offset.checked_add(val).map(TryInto::try_into) else {
            cold();
            return ReasonToBreak::Errored(Error::Trap(Trap::MemoryOutOfBounds {
                offset: val as usize,
                len: LOAD_SIZE,
                max: 0,
            }))
            .into();
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
    ) -> ControlFlow<ReasonToBreak> {
        let mem = self.store.get_mem_mut(self.module.resolve_mem_addr(mem_addr));
        let val = self.stack.values.pop::<T>();
        let val = (cast(val)).to_mem_bytes();
        let addr = self.stack.values.pop::<i32>() as u64;
        if let Err(e) = mem.store((offset + addr) as usize, val.len(), &val) {
            return ReasonToBreak::Errored(e).into();
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

        table.init(dst, &items[offset as usize..(offset + size) as usize])
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
