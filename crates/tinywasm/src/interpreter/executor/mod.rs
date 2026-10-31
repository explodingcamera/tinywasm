use core::convert::identity;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use super::no_std_floats::NoStdFloatExt;

use alloc::boxed::Box;
use alloc::vec::Vec;

use interpreter::stack::{CallFrame, CallStack, ValueStack};
use tinywasm_types::*;

use super::num_helpers::*;
use super::values::*;
use crate::engine::{Config, FuelPolicy};
use crate::interpreter::Value128;
use crate::store::{Code, State};
use crate::*;

#[macro_use]
mod instructions;
mod budget;

pub(crate) use budget::Budget;

#[cfg(not(feature = "nightly-tail-calls"))]
mod dispatch;
#[cfg(feature = "nightly-tail-calls")]
mod dispatch_become;

const FUEL_COST_CALL_TOTAL: u32 = 5;

// Keep handler returns pointer-sized and trap allocation on a cold path.
struct ExecError(Box<Trap>);

type ExecResult<T> = core::result::Result<T, ExecError>;

impl From<Trap> for ExecError {
    #[cold]
    fn from(trap: Trap) -> Self {
        Self(Box::new(trap))
    }
}

const _: () = assert!(core::mem::size_of::<ExecResult<()>>() == core::mem::size_of::<usize>());

#[derive(Clone, Copy)]
enum ExecFlow {
    Next(usize),
    Jump(usize),
    Checkpoint(usize),
    Complete,
    Stop,
}

/// Owned outcome data read by the driver after the executor restores the store.
#[derive(Clone, Copy)]
pub(crate) enum ExecOutcome {
    /// The root frame returned successfully.
    Complete,
    /// The saved frame points to its next unexecuted instruction.
    Suspended(CallFrame),
    /// Invoke this callback after stack ownership is restored.
    Host(HostCall),
    /// The driver must root this reference before any operation that could collect.
    Exception(ValueRef),
}

/// A resolved host call, with no borrowed registry entries or callback allocation.
#[derive(Clone, Copy)]
pub(crate) struct HostCall {
    /// Resolved host function address.
    pub(crate) func: FuncAddr,
    /// Calling module, used to construct the callback's context.
    pub(crate) module: ModuleInstanceId,
    /// Caller frame to resume or return from after the callback.
    pub(crate) caller: CallFrame,
    /// Return from the caller after the callback instead of continuing at its next instruction.
    pub(crate) tail: bool,
}

/// Active Wasm execution, borrowing code and mutable state while owning its value stack.
pub(crate) struct Executor<'store> {
    cf: CallFrame,
    func: &'store WasmFunction,
    module: &'store ModuleInstance,
    code: &'store Code,
    state: &'store mut State,
    config: &'store Config,
    execution_fuel: &'store mut u32,
    call_stack: &'store mut CallStack,
    value_stack: ValueStack,
    stack_slot: &'store mut ValueStack,
    call_stack_base: u32,
    mem0: MemAddr,
    budget: Budget,
    segment_start: usize,
    outcome: ExecOutcome,
}

impl<'store> Executor<'store> {
    /// Borrows disjoint store fields and takes its stack, leaving an allocation-free placeholder.
    pub(crate) fn new(store: &'store mut Store, cf: CallFrame, call_stack_base: u32, budget: Budget) -> Self {
        let wasm_func = store.code.funcs.wasm(cf.func_addr);
        let module = &store.code.instances[wasm_func.owner as usize];
        let mem0 = module.mem0_addr();
        Self {
            module,
            cf,
            func: &wasm_func.func,
            code: &store.code,
            state: &mut store.state,
            config: store.engine.config(),
            execution_fuel: &mut store.execution_fuel,
            call_stack: &mut store.call_stack,
            value_stack: core::mem::take(&mut store.value_stack),
            stack_slot: &mut store.value_stack,
            call_stack_base,
            mem0,
            budget,
            segment_start: cf.instr_ptr,
            outcome: ExecOutcome::Complete,
        }
    }

    /// Resolves a module-local memory index to its store address, caching the common memory-0 case.
    #[inline(always)]
    fn mem_addr(&self, idx: MemAddr) -> MemAddr {
        if idx == 0 { self.mem0 } else { self.module.resolve_mem_addr(idx) }
    }

    /// Returns the borrowed instruction stream independently of the executor's mutable fields.
    #[inline(always)]
    fn instructions(&self) -> &'store [Instruction] {
        &self.func.instructions
    }

    #[inline(always)]
    fn charge_call_fuel(&mut self, total_fuel_cost: u32) {
        if matches!(self.budget, Budget::Fuel) {
            let extra = match self.config.fuel_policy {
                FuelPolicy::PerInstruction => 0,
                FuelPolicy::Weighted => total_fuel_cost.saturating_sub(1),
            };

            *self.execution_fuel = self.execution_fuel.saturating_sub(extra);
        }
    }

    #[inline(always)]
    fn exec_jump_if_ref<const ON_NULL: bool>(&mut self) -> bool {
        let is_null = ValueRef::stack_peek(&self.value_stack).is_null();
        if is_null {
            ValueRef::stack_pop(&mut self.value_stack);
        }
        is_null == ON_NULL
    }

    #[inline(always)]
    fn exec_br_on_cast<const ON_MATCH: bool>(&self, index: Operand64Idx<(u32, u32)>) -> Option<usize> {
        let operand = index.resolve(&self.func.data);
        let ty = RefType::from_bits(operand.b()).expect("invalid ref type");
        (self.exec_ref_matches(ty) == ON_MATCH).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_if_local<T: InternalValue + Default + PartialEq, const ON_ZERO: bool>(
        &self,
        local: LocalAddr,
    ) -> bool {
        (T::local_get(&self.value_stack, &self.cf, local) == T::default()) == ON_ZERO
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_const32(&mut self, packed: PackedOp64<CmpOp, (u32, i32)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        packed.op.cmp(i32::stack_pop(&mut self.value_stack), operand.b()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_const64(&mut self, packed: PackedOp128<CmpOp, (u32, i64)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        packed.op.cmp(i64::stack_pop(&mut self.value_stack), operand.b()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_local32(&mut self, packed: PackedOp64<CmpOp, (u32, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::stack_pop(&mut self.value_stack);
        let rhs = i32::local_get(&self.value_stack, &self.cf, operand.b());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_local64(&mut self, packed: PackedOp64<CmpOp, (u32, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i64::stack_pop(&mut self.value_stack);
        let rhs = i64::local_get(&self.value_stack, &self.cf, operand.b());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_binop_local_const_jump(&mut self, packed: PackedOp128<BinOp, LocalUpdateOperand>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let value = i32::local_update(&mut self.value_stack, &self.cf, operand.local(), |value| {
            packed.op.exec(value as u32, operand.value() as u32) as i32
        });
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_binop_local_const_jump_cmp_local(
        &mut self,
        packed: PackedOp128<(BinOp, CmpOp), LocalUpdateCmpOperand>,
    ) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::local_update(&mut self.value_stack, &self.cf, operand.local(), |value| {
            packed.op.0.exec(value as u32, operand.value() as u32) as i32
        });
        let rhs = i32::local_get(&self.value_stack, &self.cf, operand.right());
        packed.op.1.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_binop_stack_const_tee_local_jump(
        &mut self,
        packed: PackedOp128<BinOp, LocalUpdateOperand>,
    ) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let value =
            i32::stack_update(&mut self.value_stack, |lhs| packed.op.exec(lhs as u32, operand.value() as u32) as i32);
        i32::local_set(&mut self.value_stack, &self.cf, operand.local(), value);
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_binop_global_const_jump(&mut self, packed: PackedOp128<BinOp, GlobalUpdateOperand>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let global = self.module.resolve_global_addr(operand.global());
        let value = i32::global_update(&mut self.state.globals, global, |value| {
            packed.op.exec(value as u32, operand.value() as u32) as i32
        });
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_inc_local_jump(&mut self, index: Operand128Idx<LocalUpdateOperand>) -> Option<usize> {
        let operand = index.resolve(&self.func.data);
        let value = i32::local_update(&mut self.value_stack, &self.cf, operand.local(), |value| {
            value.wrapping_add(operand.value())
        });
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_inc_stack_tee_local_jump(&mut self, index: Operand128Idx<LocalUpdateOperand>) -> Option<usize> {
        let operand = index.resolve(&self.func.data);
        let value = i32::stack_update(&mut self.value_stack, |value| value.wrapping_add(operand.value()));
        i32::local_set(&mut self.value_stack, &self.cf, operand.local(), value);
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_inc_global_jump(&mut self, index: Operand128Idx<GlobalUpdateOperand>) -> Option<usize> {
        let operand = index.resolve(&self.func.data);
        let global = self.module.resolve_global_addr(operand.global());
        let value = i32::global_update(&mut self.state.globals, global, |value| value.wrapping_add(operand.value()));
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_inc_local_jump_cmp_local(&mut self, packed: PackedOp128<CmpOp, LocalUpdateCmpOperand>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::local_update(&mut self.value_stack, &self.cf, operand.local(), |value| {
            value.wrapping_add(operand.value())
        });
        let rhs = i32::local_get(&self.value_stack, &self.cf, operand.right());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_const32(&self, packed: PackedOp128<CmpOp, (u32, i32, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::local_get(&self.value_stack, &self.cf, operand.c());
        packed.op.cmp(lhs, operand.b()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_const64(&self, packed: PackedOp128<CmpOp, (u32, i32, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i64::local_get(&self.value_stack, &self.cf, operand.c());
        packed.op.cmp(lhs, i64::from(operand.b())).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_local32(&self, packed: PackedOp64<CmpOp, (u32, u16, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::local_get(&self.value_stack, &self.cf, operand.b());
        let rhs = i32::local_get(&self.value_stack, &self.cf, operand.c());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_local64(&self, packed: PackedOp64<CmpOp, (u32, u16, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i64::local_get(&self.value_stack, &self.cf, operand.b());
        let rhs = i64::local_get(&self.value_stack, &self.cf, operand.c());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    fn exec_i64_add128(&mut self) {
        let b_hi = i64::stack_pop(&mut self.value_stack);
        let b_lo = i64::stack_pop(&mut self.value_stack);
        let a_hi = i64::stack_pop(&mut self.value_stack);
        let a_lo = i64::stack_pop(&mut self.value_stack);
        let lo = a_lo.wrapping_add(b_lo);
        let carry = u64::from((lo as u64) < (a_lo as u64));
        let hi = a_hi.wrapping_add(b_hi).wrapping_add(carry as i64);
        i64::stack_push(&mut self.value_stack, lo);
        i64::stack_push(&mut self.value_stack, hi);
    }

    fn exec_i64_sub128(&mut self) {
        let b_hi = i64::stack_pop(&mut self.value_stack);
        let b_lo = i64::stack_pop(&mut self.value_stack);
        let a_hi = i64::stack_pop(&mut self.value_stack);
        let a_lo = i64::stack_pop(&mut self.value_stack);
        let lo = a_lo.wrapping_sub(b_lo);
        let borrow = u64::from((a_lo as u64) < (b_lo as u64));
        let hi = a_hi.wrapping_sub(b_hi).wrapping_sub(borrow as i64);
        i64::stack_push(&mut self.value_stack, lo);
        i64::stack_push(&mut self.value_stack, hi);
    }

    fn exec_i64_mul_wide_s(&mut self) {
        let rhs = i64::stack_pop(&mut self.value_stack);
        let lhs = i64::stack_pop(&mut self.value_stack);
        let product = (lhs as i128).wrapping_mul(rhs as i128);
        i64::stack_push(&mut self.value_stack, product as i64);
        i64::stack_push(&mut self.value_stack, (product >> 64) as i64);
    }

    fn exec_i64_mul_wide_u(&mut self) {
        let rhs = i64::stack_pop(&mut self.value_stack);
        let lhs = i64::stack_pop(&mut self.value_stack);
        let product = (lhs as u64 as u128).wrapping_mul(rhs as u64 as u128);
        i64::stack_push(&mut self.value_stack, product as u64 as i64);
        i64::stack_push(&mut self.value_stack, (product >> 64) as u64 as i64);
    }

    #[inline(always)]
    fn exec_simd_extract_lane<TO: InternalValue>(&mut self, lane: u8, operation: impl FnOnce(Value128, u8) -> TO) {
        let vector = Value128::stack_pop(&mut self.value_stack);
        TO::stack_push(&mut self.value_stack, operation(vector, lane));
    }

    #[inline(always)]
    fn exec_simd_replace_lane<VALUE: InternalValue>(
        &mut self,
        lane: u8,
        operation: impl FnOnce(VALUE, Value128, u8) -> Value128,
    ) {
        let vector = Value128::stack_pop(&mut self.value_stack);
        let value = VALUE::stack_pop(&mut self.value_stack);
        Value128::stack_push(&mut self.value_stack, operation(value, vector, lane));
    }

    fn exec_simd_shuffle(&mut self, lanes: Value128) {
        let rhs = Value128::stack_pop(&mut self.value_stack);
        let lhs = Value128::stack_pop(&mut self.value_stack);
        Value128::stack_push(&mut self.value_stack, Value128::i8x16_shuffle(lhs, rhs, lanes));
    }

    #[inline(always)]
    fn exec_local_set_pop<T: InternalValue>(&mut self, local: LocalAddr) {
        let value = T::stack_pop(&mut self.value_stack);
        T::local_set(&mut self.value_stack, &self.cf, local, value);
    }

    #[inline(always)]
    fn exec_local_tee<T: InternalValue>(&mut self, local: LocalAddr) {
        let value = T::stack_peek(&self.value_stack);
        T::local_set(&mut self.value_stack, &self.cf, local, value);
    }

    #[inline(always)]
    fn exec_global_get<T: InternalValue>(&mut self, global: GlobalAddr) {
        let addr = self.module.resolve_global_addr(global);
        let value = T::global_get(&self.state.globals, addr);
        T::stack_push(&mut self.value_stack, value);
    }

    #[inline(always)]
    fn exec_global_set<T: InternalValue>(&mut self, global: GlobalAddr) {
        let addr = self.module.resolve_global_addr(global);
        let value = T::stack_pop(&mut self.value_stack);
        T::global_set(&mut self.state.globals, addr, value);
    }

    #[inline(always)]
    fn exec_global_tee<T: InternalValue>(&mut self, global: GlobalAddr) {
        let addr = self.module.resolve_global_addr(global);
        let value = T::stack_peek(&self.value_stack);
        T::global_set(&mut self.state.globals, addr, value);
    }

    #[inline(always)]
    fn exec_binop_result<T: InternalValue, const PUSH: bool>(&mut self, dst: Option<LocalAddr>, val: T) {
        if let Some(dst) = dst {
            T::local_set(&mut self.value_stack, &self.cf, dst, val);
        }
        if PUSH {
            T::stack_push(&mut self.value_stack, val);
        }
    }

    #[inline(always)]
    fn exec_binop_local_local<T: InternalValue, const PUSH: bool>(
        &mut self,
        lhs: LocalAddr,
        rhs: LocalAddr,
        destination: Option<LocalAddr>,
        op: impl BinOpExt<T>,
    ) {
        let lhs = T::local_get(&self.value_stack, &self.cf, lhs);
        let rhs = T::local_get(&self.value_stack, &self.cf, rhs);
        self.exec_binop_result::<T, PUSH>(destination, op.exec(lhs, rhs));
    }

    #[inline(always)]
    fn exec_binop_local_local_indexed<T: InternalValue, const PUSH: bool>(
        &mut self,
        index: Operand64Idx<(u16, u16, u16)>,
        op: impl BinOpExt<T>,
    ) {
        let operands = index.resolve(&self.func.data);
        self.exec_binop_local_local::<T, PUSH>(operands.a(), operands.b(), Some(operands.c()), op);
    }

    #[inline(always)]
    fn exec_binop_local_const<T: InternalValue, const PUSH: bool>(
        &mut self,
        lhs: LocalAddr,
        rhs: T,
        destination: Option<LocalAddr>,
        op: impl BinOpExt<T>,
    ) {
        let lhs = T::local_get(&self.value_stack, &self.cf, lhs);
        self.exec_binop_result::<T, PUSH>(destination, op.exec(lhs, rhs));
    }

    #[inline(always)]
    fn exec_binop_global_const<T: InternalValue>(&mut self, lhs: GlobalAddr, rhs: T, op: impl BinOpExt<T>) {
        let lhs = T::global_get(&self.state.globals, self.module.resolve_global_addr(lhs));
        T::stack_push(&mut self.value_stack, op.exec(lhs, rhs));
    }

    #[inline(always)]
    fn exec_cmp_local_local<T>(&mut self, lhs: LocalAddr, rhs: LocalAddr, op: CmpOp)
    where
        T: InternalValue,
        CmpOp: CmpOpExt<T>,
    {
        let lhs = T::local_get(&self.value_stack, &self.cf, lhs);
        let rhs = T::local_get(&self.value_stack, &self.cf, rhs);
        i32::stack_push(&mut self.value_stack, i32::from(op.cmp(lhs, rhs)));
    }

    #[inline(always)]
    fn exec_binop_stack_global<T: InternalValue>(&mut self, global: GlobalAddr, op: impl BinOpExt<T>) {
        let addr = self.module.resolve_global_addr(global);
        let rhs = T::global_get(&self.state.globals, addr);
        T::stack_update(&mut self.value_stack, |lhs| op.exec(lhs, rhs));
    }

    #[inline(always)]
    fn exec_binop_stack_local<T: InternalValue, const PUSH: bool>(
        &mut self,
        local: LocalAddr,
        destination: Option<LocalAddr>,
        op: impl BinOpExt<T>,
    ) {
        let rhs = T::local_get(&self.value_stack, &self.cf, local);
        let lhs = T::stack_pop(&mut self.value_stack);
        self.exec_binop_result::<T, PUSH>(destination, op.exec(lhs, rhs));
    }

    #[inline(always)]
    fn exec_binop_const_tee<T: InternalValue>(&mut self, rhs: T, destination: LocalAddr, op: impl BinOpExt<T>) {
        let value = T::stack_update(&mut self.value_stack, |lhs| op.exec(lhs, rhs));
        T::local_set(&mut self.value_stack, &self.cf, destination, value);
    }

    #[inline(always)]
    fn exec_mul_acc_local<T: InternalValue>(
        &mut self,
        accumulator: LocalAddr,
        multiply: fn(T, T) -> T,
        add: fn(T, T) -> T,
    ) {
        let rhs = T::stack_pop(&mut self.value_stack);
        let lhs = T::stack_pop(&mut self.value_stack);
        let product = multiply(lhs, rhs);
        T::local_update(&mut self.value_stack, &self.cf, accumulator, |value| add(product, value));
    }

    fn exec_branch_table(&mut self, index: Operand128Idx<BranchTableOperand>) -> usize {
        let v = index.resolve(&self.func.data);
        let idx = <i32>::stack_pop(&mut self.value_stack);
        let target_ip = if idx >= 0 && (idx as u32) < v.size() {
            self.func.data.branch_table_targets.get((v.start() + idx as u32) as usize).copied().unwrap_or(v.target())
        } else {
            v.target()
        };

        target_ip as usize
    }

    fn create_exception(&mut self, tag_index: TagAddr) -> Result<ValueRef, Trap> {
        let tag_addr = self.module.resolve_tag_addr(tag_index);
        let type_addr = self.state.get_tag(tag_addr).type_addr;
        let payload_len = self.state.get_canonical_func_type(type_addr).params().len();
        self.state.gc.check_allocation(payload_len, true)?;
        let mut payload = Vec::new();
        cold_err!(payload.try_reserve_exact(payload_len)).map_err(|_| Trap::OutOfMemory)?;
        let value_stack = &mut self.value_stack;
        for index in (0..payload_len).rev() {
            let ty = self.state.get_canonical_func_type(type_addr).params()[index];
            payload.push(match ty {
                WasmType::I32 | WasmType::F32 => RuntimeValue::Value32(Value32::stack_pop(value_stack)),
                WasmType::I64 | WasmType::F64 => RuntimeValue::Value64(Value64::stack_pop(value_stack)),
                WasmType::V128 => RuntimeValue::Value128(Value128::stack_pop(value_stack)),
                WasmType::Ref(_) => RuntimeValue::ValueRef(ValueRef::stack_pop(value_stack)),
            });
        }
        payload.reverse();
        let roots = self.value_stack.stack_32.as_slice().iter().copied().map(ValueRef::from_raw);
        self.state.alloc_exception(tag_addr, payload, roots)
    }

    fn exec_throw(&mut self, tag_index: TagAddr, instr_ptr: usize) -> ExecResult<ExecFlow> {
        let exception = self.create_exception(tag_index)?;
        self.throw_exception(exception, instr_ptr)
    }

    fn exec_throw_ref(&mut self, instr_ptr: usize) -> ExecResult<ExecFlow> {
        let exception = ValueRef::stack_pop(&mut self.value_stack);
        if exception.is_null() {
            return Err(Trap::NullReference.into());
        }
        self.throw_exception(exception, instr_ptr)
    }

    fn throw_exception(&mut self, exception: ValueRef, protected_ip: usize) -> ExecResult<ExecFlow> {
        match self.dispatch_exception(exception, protected_ip)? {
            Some(flow) => Ok(flow),
            None => {
                self.outcome = ExecOutcome::Exception(exception);
                Ok(ExecFlow::Stop)
            }
        }
    }

    fn matching_catch(&self, protected_ip: usize, tag_addr: TagAddr) -> Option<ExceptionCatch> {
        let handlers = &self.func.data.exception_handlers;
        let end = handlers.partition_point(|handler| handler.start_ip as usize <= protected_ip);
        handlers[..end]
            .iter()
            .rev()
            .filter(|handler| protected_ip < handler.end_ip as usize)
            .flat_map(|handler| handler.catches.iter().copied())
            .find(|catch| match catch {
                ExceptionCatch::Tag { tag, .. } => self.module.resolve_tag_addr(*tag) == tag_addr,
                ExceptionCatch::All { .. } => true,
            })
    }

    #[inline(always)]
    fn switch_to_frame(&mut self, frame: CallFrame) {
        let previous = core::mem::replace(&mut self.cf, frame);
        if previous.func_addr == self.cf.func_addr {
            return;
        }

        let wasm_func = self.code.funcs.wasm(self.cf.func_addr);
        self.func = &wasm_func.func;
        self.module = &self.code.instances[wasm_func.owner as usize];
        self.mem0 = self.module.mem0_addr();
    }

    fn dispatch_exception(&mut self, exception: ValueRef, mut protected_ip: usize) -> Result<Option<ExecFlow>, Trap> {
        let object = self.state.gc.get(exception).ok_or(Trap::InvalidReference)?;
        let crate::store::GcObjectKind::Exception(tag_addr) = object.kind else {
            return Err(Trap::InvalidReference);
        };
        loop {
            if let Some(catch) = self.matching_catch(protected_ip, tag_addr) {
                let (landing_pad, base, with_ref, include_payload) = match catch {
                    ExceptionCatch::Tag { landing_pad, base, with_ref, .. } => (landing_pad, base, with_ref, true),
                    ExceptionCatch::All { landing_pad, base, with_ref } => (landing_pad, base, with_ref, false),
                };
                let stack_base = self.cf.stack_base();
                let target = interpreter::stack::StackBase {
                    s32: stack_base.s32 + base.c32 as u32,
                    s64: stack_base.s64 + base.c64 as u32,
                    s128: stack_base.s128 + base.c128 as u32,
                };
                self.value_stack.truncate_to_base(target);
                if include_payload {
                    let object = self.state.gc.get(exception).ok_or(Trap::InvalidReference)?;
                    for value in object.values.iter().copied() {
                        self.value_stack.push_reserved(value);
                    }
                }
                if with_ref {
                    ValueRef::stack_push(&mut self.value_stack, exception);
                }
                return Ok(Some(ExecFlow::Jump(landing_pad as usize)));
            }

            self.value_stack.truncate_to_base(self.cf.locals_base);
            let Some(caller) = self.call_stack.pop_frame(self.call_stack_base) else {
                return Ok(None);
            };
            self.switch_to_frame(caller);
            protected_ip = self.cf.instr_ptr.checked_sub(1).expect("invalid caller IP");
        }
    }

    #[inline(always)]
    fn exec_call_resolved<const TAIL: bool>(
        &mut self,
        func_addr: FuncAddr,
        return_instr_ptr: usize,
    ) -> ExecResult<ExecFlow> {
        if !self.code.funcs.is_host(func_addr) {
            return self.exec_call_wasm::<TAIL>(func_addr, return_instr_ptr);
        }
        let mut caller = self.cf;
        caller.instr_ptr = return_instr_ptr;
        self.outcome = ExecOutcome::Host(HostCall { func: func_addr, module: self.module.id(), caller, tail: TAIL });
        Ok(ExecFlow::Stop)
    }

    fn exec_call_direct(&mut self, v: u32, return_instr_ptr: usize) -> ExecResult<ExecFlow> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        self.exec_call_resolved::<false>(addr, return_instr_ptr)
    }

    fn exec_return_call_direct(&mut self, v: u32) -> ExecResult<ExecFlow> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        self.exec_call_resolved::<true>(addr, 0)
    }

    fn exec_call_self(&mut self, return_instr_ptr: usize) -> ExecResult<()> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        self.call_stack.reserve_frame()?;
        let locals_base = self.value_stack.enter_locals(&self.func.params, &self.func.locals, &self.func.max_stack)?;
        let new = CallFrame::new(self.cf.func_addr, locals_base, self.func.locals);
        self.call_stack.push(self.cf, return_instr_ptr);
        self.cf = new;
        Ok(())
    }

    fn exec_return_call_self(&mut self) -> ExecResult<()> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);

        let locals_base = self.value_stack.replace_locals(
            self.cf.locals_base,
            &self.func.params,
            &self.func.locals,
            &self.func.max_stack,
        )?;
        self.cf = CallFrame::new(self.cf.func_addr, locals_base, self.func.locals);
        Ok(())
    }

    fn exec_call_indirect<const TAIL: bool>(
        &mut self,
        index: Operand64Idx<(u32, u32)>,
        return_instr_ptr: usize,
    ) -> ExecResult<ExecFlow> {
        let operand = index.resolve(&self.func.data);
        let type_addr = operand.a();
        let table_addr = operand.b();
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);

        // verify that the table is of the right type, this should be validated by the parser already
        let table_addr = self.module.resolve_table_addr(table_addr);
        let table_idx = self.pop_table_operand(self.state.get_table(table_addr).kind.arch())?;
        let table = self.state.get_table(table_addr);

        let Ok(table) = table.get(table_idx) else {
            return cold!(Err(Trap::UndefinedElement { index: table_idx }.into()));
        };

        let Some(func_ref) = table.addr() else {
            return cold!(Err(Trap::UninitializedElement { index: table_idx }.into()));
        };

        self.exec_typed_call::<TAIL>(func_ref, self.module.resolve_type_addr(type_addr), return_instr_ptr)
    }

    fn exec_typed_call<const TAIL: bool>(
        &mut self,
        func_addr: FuncAddr,
        expected_type_addr: TypeAddr,
        return_instr_ptr: usize,
    ) -> ExecResult<ExecFlow> {
        let type_addr = self.code.funcs.type_addr(func_addr);
        if !self.state.type_addr_is_subtype(type_addr, expected_type_addr) {
            return cold!(Err(Trap::IndirectCallTypeMismatch {
                actual: Box::new(self.state.get_canonical_func_type(type_addr).clone()),
                expected: Box::new(self.state.get_canonical_func_type(expected_type_addr).clone()),
            }
            .into()));
        }
        self.exec_call_resolved::<TAIL>(func_addr, return_instr_ptr)
    }

    #[inline(always)]
    fn exec_call_wasm<const TAIL: bool>(
        &mut self,
        func_addr: FuncAddr,
        return_instr_ptr: usize,
    ) -> ExecResult<ExecFlow> {
        let func = &self.code.funcs.wasm(func_addr).func;
        let (params, locals, max_stack) = (func.params, func.locals, func.max_stack);
        let locals_base = if TAIL {
            self.value_stack.replace_locals(self.cf.locals_base, &params, &locals, &max_stack)?
        } else {
            self.call_stack.reserve_frame()?;
            let locals_base = self.value_stack.enter_locals(&params, &locals, &max_stack)?;
            self.call_stack.push(self.cf, return_instr_ptr);
            locals_base
        };
        self.switch_to_frame(CallFrame::new(func_addr, locals_base, locals));
        Ok(ExecFlow::Jump(0))
    }

    fn exec_call_ref<const TAIL: bool>(&mut self, type_addr: u32, return_instr_ptr: usize) -> ExecResult<ExecFlow> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let func_ref = ValueRef::stack_pop(&mut self.value_stack);
        let Some(func_addr) = func_ref.addr() else {
            return cold!(Err(Trap::NullFunctionReference.into()));
        };

        self.exec_typed_call::<TAIL>(func_addr, self.module.resolve_type_addr(type_addr), return_instr_ptr)
    }

    fn exec_return(&mut self) -> ExecFlow {
        self.value_stack.truncate_keep_counts(self.cf.locals_base, self.func.results);
        self.finish_return()
    }

    #[inline(always)]
    fn finish_return(&mut self) -> ExecFlow {
        let Some(caller) = self.call_stack.pop_frame(self.call_stack_base) else {
            return ExecFlow::Complete;
        };
        let instr_ptr = caller.instr_ptr;
        self.switch_to_frame(caller);
        ExecFlow::Jump(instr_ptr)
    }

    fn exec_return_void(&mut self) -> ExecFlow {
        self.value_stack.truncate_to_base(self.cf.locals_base);
        self.finish_return()
    }

    fn exec_return_32(&mut self) -> ExecFlow {
        self.value_stack.stack_32.truncate_to_one_tail(self.cf.locals_base.s32 as usize);
        self.value_stack.stack_64.truncate_to(self.cf.locals_base.s64 as usize);
        self.value_stack.stack_128.truncate_to(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    fn exec_return_64(&mut self) -> ExecFlow {
        self.value_stack.stack_32.truncate_to(self.cf.locals_base.s32 as usize);
        self.value_stack.stack_64.truncate_to_one_tail(self.cf.locals_base.s64 as usize);
        self.value_stack.stack_128.truncate_to(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    fn exec_return_128(&mut self) -> ExecFlow {
        self.value_stack.stack_32.truncate_to(self.cf.locals_base.s32 as usize);
        self.value_stack.stack_64.truncate_to(self.cf.locals_base.s64 as usize);
        self.value_stack.stack_128.truncate_to_one_tail(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    #[inline(always)]
    fn exec_store_local_local<T: InternalValue + MemValue<N>, const N: usize>(
        &mut self,
        index: Operand64Idx<CompactMemoryOperand>,
        addr_local: u8,
        value_local: u8,
    ) -> Result<(), Trap> {
        let memarg = index.resolve(&self.func.data);
        let value = T::local_get(&self.value_stack, &self.cf, u16::from(value_local));
        let mem_addr = self.mem_addr(MemAddr::from(memarg.memory()));
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            let base = if kind.arch() == MemoryArch::I64 {
                let base = u64::local_get(&self.value_stack, &self.cf, u16::from(addr_local));
                cold_err!(usize::try_from(base).map_err(|_| Trap::MemoryOutOfBounds {
                    offset: usize::MAX,
                    len: N,
                    max: mem.len(),
                }))?
            } else {
                u32::local_get(&self.value_stack, &self.cf, u16::from(addr_local)) as usize
            };
            let addr = cold_err!(mem.effective_addr::<N>(base, u64::from(memarg.offset())))?;
            value.store_at(&mut *mem, addr)
        })
    }

    #[inline(always)]
    fn exec_inc_memory_local<T: MemValue<N>, const N: usize>(
        &mut self,
        index: Operand64Idx<CompactMemoryOperand>,
        addr_local: u8,
        increment: impl FnOnce(T) -> T,
    ) -> Result<(), Trap> {
        let memarg = index.resolve(&self.func.data);
        let mem_addr = self.mem_addr(MemAddr::from(memarg.memory()));
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            let base = if kind.arch() == MemoryArch::I64 {
                let base = i64::local_get(&self.value_stack, &self.cf, u16::from(addr_local)) as u64;
                cold_err!(usize::try_from(base).map_err(|_| Trap::MemoryOutOfBounds {
                    offset: usize::MAX,
                    len: N,
                    max: mem.len(),
                }))?
            } else {
                u32::local_get(&self.value_stack, &self.cf, u16::from(addr_local)) as usize
            };
            let addr = cold_err!(mem.effective_addr::<N>(base, u64::from(memarg.offset())))?;

            let value = cold_err!(T::load_at(&*mem, addr))?;
            increment(value).store_at(&mut *mem, addr)
        })
    }

    #[inline(always)]
    fn exec_fma_store<
        T: InternalValue + MemValue<N> + core::ops::Add<Output = T> + core::ops::Mul<Output = T>,
        const N: usize,
    >(
        &mut self,
        m: CompactMemoryArg,
    ) -> Result<(), Trap> {
        let rhs = T::stack_pop(&mut self.value_stack);
        let lhs = T::stack_pop(&mut self.value_stack);
        let acc = T::stack_pop(&mut self.value_stack);
        let fma = acc + lhs * rhs;
        let mem_addr = self.mem_addr(m.mem_addr());
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            let base = self.value_stack.pop_memory_operand(kind.arch())?;
            let addr = cold_err!(mem.effective_addr::<N>(base, m.offset()))?;
            cold_err!(fma.store_at(&mut *mem, addr))
        })
    }

    #[inline(always)]
    fn exec_load_local<
        LOAD: MemValue<N>,
        const N: usize,
        TARGET: InternalValue,
        const SET_LOCAL: bool,
        const TEE: bool,
    >(
        &mut self,
        index: Operand64Idx<CompactMemoryOperand>,
        addr_local: u8,
        dst_local: u8,
        cast: impl Fn(LOAD) -> TARGET,
    ) -> Result<(), Trap> {
        let memarg = index.resolve(&self.func.data);

        let mem_addr = self.mem_addr(MemAddr::from(memarg.memory()));
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            let base = if kind.arch() == MemoryArch::I64 {
                let base = i64::local_get(&self.value_stack, &self.cf, u16::from(addr_local)) as u64;
                cold_err!(usize::try_from(base)).map_err(|_| Trap::MemoryOutOfBounds {
                    offset: usize::MAX,
                    len: N,
                    max: mem.len(),
                })?
            } else {
                u32::local_get(&self.value_stack, &self.cf, u16::from(addr_local)) as usize
            };

            let addr = cold_err!(mem.effective_addr::<N>(base, u64::from(memarg.offset())))?;
            let value = cast(cold_err!(LOAD::load_at(&*mem, addr))?);

            if SET_LOCAL {
                TARGET::local_set(&mut self.value_stack, &self.cf, u16::from(dst_local), value);
            }
            if !SET_LOCAL || TEE {
                TARGET::stack_push(&mut self.value_stack, value);
            }
            Ok(())
        })
    }

    fn exec_ref_is_null(&mut self) {
        let is_null = i32::from(ValueRef::stack_pop(&mut self.value_stack).is_null());
        i32::stack_push(&mut self.value_stack, is_null);
    }

    fn exec_ref_as_non_null(&mut self) -> Result<(), Trap> {
        if ValueRef::stack_peek(&self.value_stack).is_null() {
            return cold!(Err(Trap::NullReference));
        }
        Ok(())
    }

    fn canonical_ref_type(&self, ty: RefType) -> RefType {
        let Some(type_index) = ty.type_index() else { return ty };
        RefType::new_concrete(ty.is_nullable(), self.module.resolve_type_addr(type_index))
    }

    fn exec_ref_matches(&self, ty: RefType) -> bool {
        let value = ValueRef::stack_peek(&self.value_stack);
        self.state.value_ref_matches(&self.code.funcs, value, self.canonical_ref_type(ty))
    }

    fn exec_ref_test(&mut self, ty: RefType) {
        let value = ValueRef::stack_pop(&mut self.value_stack);
        let matches = self.state.value_ref_matches(&self.code.funcs, value, self.canonical_ref_type(ty));
        i32::stack_push(&mut self.value_stack, i32::from(matches));
    }

    fn exec_ref_cast(&self, ty: RefType) -> Result<(), Trap> {
        if !self.exec_ref_matches(ty) {
            return cold!(Err(Trap::CastFailure));
        }
        Ok(())
    }

    fn exec_i31_get(&mut self, signed: bool) -> Result<(), Trap> {
        let value = ValueRef::stack_pop(&mut self.value_stack);
        if value.is_null() {
            return cold!(Err(Trap::NullI31Reference));
        }
        let value = if signed {
            value.i31_s().expect("validated i31.get operand")
        } else {
            value.i31_u().expect("validated i31.get operand") as i32
        };
        i32::stack_push(&mut self.value_stack, value);
        Ok(())
    }

    fn push_gc_object(&mut self, type_addr: TypeAddr, values: Vec<RuntimeValue>) -> Result<(), Trap> {
        let roots = self.value_stack.stack_32.as_slice().iter().copied().map(ValueRef::from_raw);
        let reference = self.state.alloc_gc_object(type_addr, values, roots)?;
        ValueRef::stack_push(&mut self.value_stack, reference);
        Ok(())
    }

    fn exec_struct_new(&mut self, type_index: TypeAddr, default: bool) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let field_count = self.state.get_type(type_addr).as_struct().expect("validated struct.new type").fields.len();
        self.state.gc.check_allocation(field_count, self.state.gc_type_has_references(type_addr))?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(field_count)).map_err(|_| Trap::OutOfMemory)?;
        if default {
            values.extend(
                self.state
                    .get_type(type_addr)
                    .as_struct()
                    .expect("validated struct.new type")
                    .fields
                    .iter()
                    .map(|field| default_value(field.storage)),
            );
        } else {
            for index in (0..field_count).rev() {
                let storage = self.state.get_type(type_addr).as_struct().unwrap().fields[index].storage;
                values.push(pop_value(&mut self.value_stack, storage));
            }
            values.reverse();
        }
        self.push_gc_object(type_addr, values)
    }

    fn exec_struct_get(&mut self, index: Operand64Idx<(u32, u32)>, signed: Option<bool>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let field_index = operand.b();
        let reference = ValueRef::stack_pop(&mut self.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.state.get_type(type_addr).as_struct().expect("validated struct.get type").fields
            [field_index as usize]
            .storage;
        let object = self.state.gc_object(reference, type_addr)?;
        let object = self.state.gc.get_handle(object).ok_or(Trap::Other("invalid GC reference"))?;
        let value = *object.values.get(field_index as usize).expect("validated struct field index");
        push_value(&mut self.value_stack, value, storage, signed);
        Ok(())
    }

    fn exec_struct_set(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let field_index = operand.b();
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.state.get_type(type_addr).as_struct().expect("validated struct.set type").fields
            [field_index as usize]
            .storage;
        let value = pop_value(&mut self.value_stack, storage);
        let reference = ValueRef::stack_pop(&mut self.value_stack);
        let object = self.state.gc_object(reference, type_addr)?;
        self.state.gc.set(object, field_index as usize, value).expect("live struct field");
        Ok(())
    }

    fn exec_array_new(&mut self, type_index: TypeAddr, default: bool) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.state.get_type(type_addr).as_array().expect("validated array.new type").field.storage;
        let len = u32::stack_pop(&mut self.value_stack) as usize;
        self.state.gc.check_allocation(len, self.state.gc_type_has_references(type_addr))?;
        let value = if default { default_value(storage) } else { pop_value(&mut self.value_stack, storage) };
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        values.resize(len, value);
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_new_fixed(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let len = operand.b();
        let type_addr = self.module.resolve_type_addr(type_index);
        let arr_type = self.state.get_type(type_addr).as_array().expect("validated array.new_fixed type");
        let len = len as usize;
        self.state.gc.check_allocation(len, self.state.gc_type_has_references(type_addr))?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        for _ in 0..len {
            values.push(pop_value(&mut self.value_stack, arr_type.field.storage));
        }
        values.reverse();
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_get(&mut self, type_index: TypeAddr, signed: Option<bool>) -> Result<(), Trap> {
        let index = u32::stack_pop(&mut self.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.state.get_type(type_addr).as_array().expect("validated array.get type").field.storage;
        let object = self.state.gc_object(reference, type_addr)?;
        let object = self.state.gc.get_handle(object).ok_or(Trap::Other("invalid GC reference"))?;
        let value = *object.values.get(index).ok_or(Trap::ArrayOutOfBounds)?;
        push_value(&mut self.value_stack, value, storage, signed);
        Ok(())
    }

    fn exec_array_set(&mut self, type_index: TypeAddr) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.state.get_type(type_addr).as_array().expect("validated array.set type").field.storage;
        let value = pop_value(&mut self.value_stack, storage);
        let index = u32::stack_pop(&mut self.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.value_stack);
        let object = self.state.gc_object(reference, type_addr)?;
        self.state.gc.set(object, index, value).ok_or(Trap::ArrayOutOfBounds)
    }

    fn exec_array_len(&mut self) -> Result<(), Trap> {
        let reference = ValueRef::stack_pop(&mut self.value_stack);
        if reference.is_null() {
            return Err(Trap::NullArrayReference);
        }
        let object = self.state.gc.get(reference).ok_or(Trap::Other("invalid GC reference"))?;
        let crate::store::GcObjectKind::Composite(type_addr) = object.kind else {
            return Err(Trap::Other("GC reference is not an array"));
        };
        if self.state.get_type(type_addr).as_array().is_none() {
            return Err(Trap::Other("GC reference is not an array"));
        }
        i32::stack_push(&mut self.value_stack, object.values.len() as i32);
        Ok(())
    }

    fn exec_array_fill(&mut self, type_index: TypeAddr) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.state.get_type(type_addr).as_array().expect("validated array.fill type").field.storage;
        let len = u32::stack_pop(&mut self.value_stack) as usize;
        let value = pop_value(&mut self.value_stack, storage);
        let index = u32::stack_pop(&mut self.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.value_stack);
        let object = self.state.gc_object(reference, type_addr)?;
        let object_ref = self.state.gc.get_handle(object).ok_or(Trap::Other("invalid GC reference"))?;
        let end = index.checked_add(len).filter(|end| *end <= object_ref.values.len()).ok_or(Trap::ArrayOutOfBounds)?;
        self.state.gc.fill(object, index..end, value).expect("live array range");
        Ok(())
    }

    fn exec_array_copy(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let dst_type = operand.a();
        let src_type = operand.b();
        let len = u32::stack_pop(&mut self.value_stack) as usize;
        let src_index = u32::stack_pop(&mut self.value_stack) as usize;
        let src = ValueRef::stack_pop(&mut self.value_stack);
        let dst_index = u32::stack_pop(&mut self.value_stack) as usize;
        let dst = ValueRef::stack_pop(&mut self.value_stack);
        let dst_type = self.module.resolve_type_addr(dst_type);
        let src_type = self.module.resolve_type_addr(src_type);
        let dst_handle = self.state.gc_object(dst, dst_type)?;
        let dst_len = self.state.gc.get_handle(dst_handle).expect("validated array").values.len();
        let src_handle = self.state.gc_object(src, src_type)?;
        let src_object = self.state.gc.get_handle(src_handle).expect("validated array");
        let src_end =
            src_index.checked_add(len).filter(|end| *end <= src_object.values.len()).ok_or(Trap::ArrayOutOfBounds)?;
        dst_index.checked_add(len).filter(|end| *end <= dst_len).ok_or(Trap::ArrayOutOfBounds)?;
        if src_handle == dst_handle {
            self.state.gc.copy_within(dst_handle, src_index..src_end, dst_index).expect("live array range");
            return Ok(());
        }
        self.state.gc.copy_between(src_handle, src_index..src_end, dst_handle, dst_index).expect("live array ranges");
        Ok(())
    }

    fn exec_array_new_data(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let data_index = operand.b();
        let len = u32::stack_pop(&mut self.value_stack) as usize;
        let src = u32::stack_pop(&mut self.value_stack) as usize;
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.state.get_type(type_addr).as_array().expect("validated array type").field.storage;
        let data_addr = self.module.resolve_data_addr(data_index);
        let data = self.state.data[data_addr as usize].data.as_deref().unwrap_or(&[]);
        data_range(storage, data, src, len)?;
        self.state.gc.check_allocation(len, self.state.gc_type_has_references(type_addr))?;
        let values = decode_data(storage, data, src, len)?;
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_new_elem(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let elem_index = operand.b();
        let len = u32::stack_pop(&mut self.value_stack) as usize;
        let src = u32::stack_pop(&mut self.value_stack) as usize;
        let type_addr = self.module.resolve_type_addr(type_index);
        let elem_addr = self.module.resolve_elem_addr(elem_index);
        let items = self.state.elements[elem_addr as usize].items_range(src, len)?;
        self.state.gc.check_allocation(len, self.state.gc_type_has_references(type_addr))?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        values.extend(items.iter().copied().map(RuntimeValue::ValueRef));
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_init_data(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let data_index = operand.b();
        let len = u32::stack_pop(&mut self.value_stack) as usize;
        let src = u32::stack_pop(&mut self.value_stack) as usize;
        let dst = u32::stack_pop(&mut self.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.state.get_type(type_addr).as_array().expect("validated array type").field.storage;
        let object = self.state.gc_object(reference, type_addr)?;
        let object_len = self.state.gc.get_handle(object).expect("validated array").values.len();
        dst.checked_add(len).filter(|end| *end <= object_len).ok_or(Trap::ArrayOutOfBounds)?;
        let data = self.state.data[self.module.resolve_data_addr(data_index) as usize].data.as_deref().unwrap_or(&[]);
        let values = decode_data(storage, data, src, len)?;
        self.state.gc.set_slice(object, dst, &values).expect("live array range");
        Ok(())
    }

    fn exec_array_init_elem(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let elem_index = operand.b();
        let len = u32::stack_pop(&mut self.value_stack) as usize;
        let src = u32::stack_pop(&mut self.value_stack) as usize;
        let dst = u32::stack_pop(&mut self.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let object = self.state.gc_object(reference, type_addr)?;
        let object_len = self.state.gc.get_handle(object).expect("validated array").values.len();
        dst.checked_add(len).filter(|end| *end <= object_len).ok_or(Trap::ArrayOutOfBounds)?;
        let elem_addr = self.module.resolve_elem_addr(elem_index);
        let items = self.state.elements[elem_addr as usize].items_range(src, len)?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        values.extend(items.iter().copied().map(RuntimeValue::ValueRef));
        self.state.gc.set_slice(object, dst, &values).expect("live array range");
        Ok(())
    }

    fn exec_memory_size(&mut self, addr: u32) {
        match self.state.memory_size(self.mem_addr(addr)) {
            (MemoryArch::I64, pages) => i64::stack_push(&mut self.value_stack, pages as i64),
            (MemoryArch::I32, pages) => i32::stack_push(&mut self.value_stack, pages as i32),
        }
    }

    fn exec_memory_grow(&mut self, addr: u32) -> Result<(), Trap> {
        let mem_addr = self.mem_addr(addr);
        let limiter = self.config.resource_limiter.as_deref();
        let is_64bit = self.state.memory_type(mem_addr).arch() == MemoryArch::I64;
        let pages_delta = match is_64bit {
            true => i64::stack_pop(&mut self.value_stack),
            false => i64::from(i32::stack_pop(&mut self.value_stack)),
        };

        let size = self.state.grow_mem(mem_addr, pages_delta, limiter)?.unwrap_or(-1);
        match is_64bit {
            true => i64::stack_push(&mut self.value_stack, size),
            false => i32::stack_push(&mut self.value_stack, size as i32),
        };

        Ok(())
    }

    fn exec_memory_copy(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let dst_mem = operand.a();
        let src_mem = operand.b();
        let dst_mem_addr = self.mem_addr(dst_mem);
        let src_mem_addr = self.mem_addr(src_mem);
        let dst_arch = self.state.memory_type(dst_mem_addr).arch();
        let src_arch = self.state.memory_type(src_mem_addr).arch();
        let len_arch =
            if dst_arch == MemoryArch::I32 || src_arch == MemoryArch::I32 { MemoryArch::I32 } else { MemoryArch::I64 };
        let size = self.value_stack.pop_memory_operand(len_arch)?;
        let src = self.value_stack.pop_memory_operand(src_arch)?;
        let dst = self.value_stack.pop_memory_operand(dst_arch)?;

        self.state.copy_memories(dst_mem_addr, dst, src_mem_addr, src, size)
    }

    fn exec_memory_fill(&mut self, addr: u32) -> Result<(), Trap> {
        let mem_addr = self.mem_addr(addr);
        let arch = self.state.memory_type(mem_addr).arch();
        let size = self.value_stack.pop_memory_operand(arch)?;
        let val = i32::stack_pop(&mut self.value_stack);
        let dst = self.value_stack.pop_memory_operand(arch)?;
        self.exec_memory_fill_impl(mem_addr, dst, val as u8, size)
    }

    fn exec_memory_fill_const(&mut self, index: Operand128Idx<MemoryFillOperand>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let mem_addr = self.mem_addr(operand.memory());
        let arch = self.state.memory_type(mem_addr).arch();
        let dst = self.value_stack.pop_memory_operand(arch)?;
        self.exec_memory_fill_impl(mem_addr, dst, operand.byte(), operand.value() as u32 as usize)
    }

    fn exec_memory_fill_impl(&mut self, mem_addr: MemAddr, dst: usize, val: u8, size: usize) -> Result<(), Trap> {
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            let max = mem.len();
            if mem.fill(dst, size, val).is_none() {
                return cold!(Err(Trap::MemoryOutOfBounds { offset: dst, len: size, max }));
            }
            Ok(())
        })
    }

    fn exec_memory_init(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let data_index = operand.a();
        let mem_index = operand.b();
        let size = u32::stack_pop(&mut self.value_stack) as usize;
        let offset = u32::stack_pop(&mut self.value_stack) as usize;
        let mem_addr = self.mem_addr(mem_index);
        let arch = self.state.memory_type(mem_addr).arch();
        let dst = self.value_stack.pop_memory_operand(arch)?;

        let data = self.state.data[self.module.resolve_data_addr(data_index) as usize].data.clone();
        // Dropped segments behave like empty segments, including valid zero-length copies.
        let data = data.as_deref().unwrap_or_default();
        let end = offset.checked_add(size).filter(|&end| end <= data.len()).ok_or(Trap::MemoryOutOfBounds {
            offset,
            len: size,
            max: data.len(),
        })?;
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            cold_err!(mem.write_all(dst, &data[offset..end]).ok_or_else(|| Trap::MemoryOutOfBounds {
                offset: dst,
                len: size,
                max: mem.len(),
            }))
        })
    }

    fn exec_table_copy(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let dst_table = operand.a();
        let src_table = operand.b();
        let dst_table_addr = self.module.resolve_table_addr(dst_table);
        let src_table_addr = self.module.resolve_table_addr(src_table);
        let dst_arch = self.state.get_table(dst_table_addr).kind.arch();
        let src_arch = self.state.get_table(src_table_addr).kind.arch();
        let len_arch =
            if dst_arch == MemoryArch::I32 || src_arch == MemoryArch::I32 { MemoryArch::I32 } else { MemoryArch::I64 };
        let size = self.pop_table_operand(len_arch)?;
        let src = self.pop_table_operand(src_arch)?;
        let dst = self.pop_table_operand(dst_arch)?;

        if dst_table_addr == src_table_addr {
            // copy within the same table
            self.state.get_table_mut(dst_table_addr).copy_within(dst, src, size)
        } else {
            // copy between two tables
            let (dst_table_ref, src_table_ref) = self.state.get_tables_mut(dst_table_addr, src_table_addr);
            dst_table_ref.copy_from_slice(dst, src_table_ref.load(src, size)?)
        }
    }

    #[inline(always)]
    fn exec_mem_load_lane<LOAD: MemValue<LOAD_SIZE>, const LOAD_SIZE: usize>(
        &mut self,
        arg: MemoryLaneArg,
    ) -> Result<(), Trap> {
        let m = arg.memory_arg_idx.resolve(&self.func.data);
        let mem_addr = self.mem_addr(m.memory());
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            let base = self.value_stack.pop_memory_operand(kind.arch())?;
            let addr = cold_err!(mem.effective_addr::<LOAD_SIZE>(base, m.offset()))?;
            let val = cold_err!(LOAD::load_at(&*mem, addr))?;
            let offset = arg.lane as usize * LOAD_SIZE;
            Value128::stack_update(&mut self.value_stack, |value| {
                let mut bytes = value.to_mem_bytes();
                bytes[offset..offset + LOAD_SIZE].copy_from_slice(&val.to_mem_bytes());
                Value128(bytes)
            });
            Ok(())
        })
    }

    #[inline(always)]
    fn exec_mem_load<LOAD: MemValue<LOAD_SIZE>, const LOAD_SIZE: usize, TARGET: InternalValue>(
        &mut self,
        m: Operand128<MemoryOperand>,
        cast: impl Fn(LOAD) -> TARGET,
    ) -> Result<(), Trap> {
        let mem_addr = self.mem_addr(m.memory());
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            let base = self.value_stack.pop_memory_operand(kind.arch())?;
            let addr = cold_err!(mem.effective_addr::<LOAD_SIZE>(base, m.offset()))?;
            let value = cold_err!(LOAD::load_at(&*mem, addr))?;
            TARGET::stack_push(&mut self.value_stack, cast(value));
            Ok(())
        })
    }

    fn exec_atomic(&mut self, arg: AtomicArg) -> Result<(), Trap> {
        match arg.width() {
            AtomicWidth::Bytes1 => self.exec_atomic_width::<1>(arg),
            AtomicWidth::Bytes2 => self.exec_atomic_width::<2>(arg),
            AtomicWidth::Bytes4 => self.exec_atomic_width::<4>(arg),
            AtomicWidth::Bytes8 => self.exec_atomic_width::<8>(arg),
        }
    }

    fn exec_atomic_wait(&mut self, arg: Operand128Idx<MemoryOperand>, op: AtomicWaitOp) -> Result<(), Trap> {
        let timeout = if op == AtomicWaitOp::Notify { 0 } else { i64::stack_pop(&mut self.value_stack) };
        let value = match op {
            AtomicWaitOp::Wait64 => u64::stack_pop(&mut self.value_stack),
            _ => u32::stack_pop(&mut self.value_stack) as u64,
        };
        let memory = arg.resolve(&self.func.data);
        let mem_addr = self.mem_addr(memory.memory());
        let width = if op == AtomicWaitOp::Wait64 { 8 } else { 4 };
        let addr = crate::store::with_memory!(@lock_inline self.state, mem_addr, |mem, kind| {
            let base = self.value_stack.pop_memory_operand(kind.arch())?;
            let addr = cold_err!(mem.effective_addr::<1>(base, memory.offset()))?;
            if addr % width != 0 {
                return cold!(Err(Trap::UnalignedAtomic));
            }

            if op != AtomicWaitOp::Notify && !kind.shared() {
                return cold!(Err(Trap::Other("atomic wait requires shared memory")));
            }

            // Check the entire access even for notify and even when no waiter exists.
            if mem.checked_range(addr, width).is_none() {
                return cold!(Err(Trap::MemoryOutOfBounds { offset: addr, len: width, max: mem.len() }));
            }

            Ok(addr)
        })?;

        #[cfg(not(feature = "std"))]
        let _ = (timeout, value, addr);

        #[cfg(feature = "std")]
        if mem_addr & crate::store::SHARED_MEM_BIT != 0 {
            let shared = &self.state.shared_memories[(mem_addr & !crate::store::SHARED_MEM_BIT) as usize];
            let result = match op {
                AtomicWaitOp::Notify => shared.notify(addr, value as u32),
                AtomicWaitOp::Wait32 => shared.wait::<4>(addr, value, timeout)?,
                AtomicWaitOp::Wait64 => shared.wait::<8>(addr, value, timeout)?,
            };
            u32::stack_push(&mut self.value_stack, result);
            return Ok(());
        }

        u32::stack_push(&mut self.value_stack, 0);
        Ok(())
    }

    fn exec_atomic_width<const N: usize>(&mut self, arg: AtomicArg) -> Result<(), Trap> {
        let op = arg.op();
        let value = match op {
            AtomicOp::Load => 0,
            _ if arg.is_64() => u64::stack_pop(&mut self.value_stack),
            _ => u32::stack_pop(&mut self.value_stack) as u64,
        };

        let expected = match op {
            AtomicOp::Cmpxchg if arg.is_64() => u64::stack_pop(&mut self.value_stack),
            AtomicOp::Cmpxchg => u32::stack_pop(&mut self.value_stack) as u64,
            _ => 0,
        };

        let memory = arg.memory.resolve(&self.func.data);
        let mem_addr = self.mem_addr(memory.memory());
        crate::store::with_memory!(@lock_inline self.state, mem_addr, |mem, kind| {
            let base = self.value_stack.pop_memory_operand(kind.arch())?;
            let addr = cold_err!(mem.effective_addr::<N>(base, memory.offset()))?;
            if addr % N != 0 {
                return cold!(Err(Trap::UnalignedAtomic));
            }
            if op == AtomicOp::Store {
                let mut bytes = [0u8; N];
                bytes.copy_from_slice(&value.to_le_bytes()[..N]);
                return cold_err!(mem.write_fixed::<N>(addr, &bytes));
            }
            let bytes = cold_err!(mem.read_fixed::<N>(addr))?;
            let mut padded = [0u8; 8];
            padded[..N].copy_from_slice(&bytes);
            let old = u64::from_le_bytes(padded);
            let mask = if N == 8 { u64::MAX } else { (1u64 << (N * 8)) - 1 };
            let next = match op {
                AtomicOp::Load => None,
                AtomicOp::Store => unreachable!(),
                AtomicOp::Xchg => Some(value),
                AtomicOp::Add => Some(old.wrapping_add(value)),
                AtomicOp::Sub => Some(old.wrapping_sub(value)),
                AtomicOp::And => Some(old & value),
                AtomicOp::Or => Some(old | value),
                AtomicOp::Xor => Some(old ^ value),
                AtomicOp::Cmpxchg => (old == (expected & mask)).then_some(value),
            };
            if let Some(next) = next {
                let mut bytes = [0u8; N];
                bytes.copy_from_slice(&next.to_le_bytes()[..N]);
                cold_err!(mem.write_fixed::<N>(addr, &bytes))?;
            }

            match arg.is_64() {
                true => u64::stack_push(&mut self.value_stack, old),
                false => u32::stack_push(&mut self.value_stack, old as u32),
            }
            Ok(())
        })
    }

    #[inline(always)]
    fn exec_mem_store_lane<U: MemValue<N> + Copy, const N: usize>(&mut self, arg: MemoryLaneArg) -> Result<(), Trap> {
        let bytes = Value128::stack_pop(&mut self.value_stack).to_mem_bytes();
        let lane_offset = arg.lane as usize * N;
        let mut val_bytes = [0u8; N];
        val_bytes.copy_from_slice(&bytes[lane_offset..lane_offset + N]);
        let val = U::from_mem_bytes(val_bytes);
        let m = arg.memory_arg_idx.resolve(&self.func.data);
        let mem_addr = self.mem_addr(m.memory());
        crate::store::with_memory!(self.state, mem_addr, |mem, kind| {
            let base = self.value_stack.pop_memory_operand(kind.arch())?;
            let addr = cold_err!(mem.effective_addr::<N>(base, m.offset()))?;
            cold_err!(val.store_at(&mut *mem, addr))
        })
    }

    #[inline(always)]
    fn exec_mem_store<T: InternalValue, U: MemValue<N>, const N: usize>(
        &mut self,
        memory: Operand128<MemoryOperand>,
        cast: impl Fn(T) -> U,
    ) -> Result<(), Trap> {
        let val = cast(<T>::stack_pop(&mut self.value_stack));
        self.exec_mem_store_value(self.mem_addr(memory.memory()), memory.offset(), val)
    }

    #[inline(always)]
    fn exec_select_store<T: InternalValue + MemValue<N>, const N: usize>(
        &mut self,
        memory: Operand128<MemoryOperand>,
    ) -> Result<(), Trap> {
        let condition = Value32::stack_pop(&mut self.value_stack);
        let false_value = T::stack_pop(&mut self.value_stack);
        let true_value = T::stack_pop(&mut self.value_stack);
        let value = if condition == 0 { false_value } else { true_value };
        self.exec_mem_store_value(self.mem_addr(memory.memory()), memory.offset(), value)
    }

    #[inline(always)]
    fn exec_mem_store_value<U: MemValue<N>, const N: usize>(
        &mut self,
        memory_addr: MemAddr,
        offset: u64,
        val: U,
    ) -> Result<(), Trap> {
        crate::store::with_memory!(self.state, memory_addr, |mem, kind| {
            let base = self.value_stack.pop_memory_operand(kind.arch())?;
            let addr = cold_err!(mem.effective_addr::<N>(base, offset))?;
            cold_err!(val.store_at(&mut *mem, addr))
        })
    }

    fn exec_table_get(&mut self, table_index: u32) -> Result<(), Trap> {
        let table_addr = self.module.resolve_table_addr(table_index);
        let idx = self.pop_table_operand(self.state.get_table(table_addr).kind.arch())?;
        let value = *self.state.get_table(table_addr).get(idx)?;
        ValueRef::stack_push(&mut self.value_stack, value);
        Ok(())
    }

    fn exec_table_set(&mut self, table_index: u32) -> Result<(), Trap> {
        let val = ValueRef::stack_pop(&mut self.value_stack);
        let table_addr = self.module.resolve_table_addr(table_index);
        let idx = self.pop_table_operand(self.state.get_table(table_addr).kind.arch())?;
        let table = self.state.get_table_mut(table_addr);
        table.set(idx, val)
    }

    fn exec_table_size(&mut self, table_index: u32) {
        let table = self.state.get_table(self.module.resolve_table_addr(table_index));
        match table.kind.arch() {
            MemoryArch::I32 => i32::stack_push(&mut self.value_stack, table.size() as i32),
            MemoryArch::I64 => i64::stack_push(&mut self.value_stack, table.size() as i64),
        }
    }

    fn exec_table_init(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let size = self.pop_table_operand(MemoryArch::I32)?; // n
        let offset = self.pop_table_operand(MemoryArch::I32)?; // s
        let table_addr = self.module.resolve_table_addr(operand.b());
        let dst = self.pop_table_operand(self.state.get_table(table_addr).kind.arch())?; // d
        let elem_addr = self.module.resolve_elem_addr(operand.a()) as usize;
        let elem = self.state.elements.get(elem_addr).ok_or_else(|| Trap::Other("element not found"))?;
        let items = elem.items_range(offset, size)?;

        let table = self.state.tables.get_mut(table_addr as usize);
        table.ok_or_else(|| Trap::Other("table not found"))?.init(dst, items)
    }

    fn exec_table_grow(&mut self, table_index: u32) -> Result<(), Trap> {
        let table_addr = self.module.resolve_table_addr(table_index);
        let arch = self.state.get_table(table_addr).kind.arch();
        let n = self.pop_table_operand(arch)?;
        let val = ValueRef::stack_pop(&mut self.value_stack);
        let limiter = self.config.resource_limiter.as_deref();
        let table = self.state.get_table_mut(table_addr);
        let sz = table.size();
        let grew = table.grow(n, val, limiter)?;
        match (arch, grew) {
            (MemoryArch::I32, true) => i32::stack_push(&mut self.value_stack, sz as i32),
            (MemoryArch::I32, false) => i32::stack_push(&mut self.value_stack, -1),
            (MemoryArch::I64, true) => i64::stack_push(&mut self.value_stack, sz as i64),
            (MemoryArch::I64, false) => i64::stack_push(&mut self.value_stack, -1),
        }
        Ok(())
    }

    fn exec_table_fill(&mut self, table_index: u32) -> Result<(), Trap> {
        let table_addr = self.module.resolve_table_addr(table_index);
        let arch = self.state.get_table(table_addr).kind.arch();
        let n = self.pop_table_operand(arch)?;
        let val = ValueRef::stack_pop(&mut self.value_stack);
        let i = self.pop_table_operand(arch)?;
        self.state.get_table_mut(table_addr).fill(i, n, val)
    }

    fn pop_table_operand(&mut self, arch: MemoryArch) -> Result<usize, Trap> {
        let value = match arch {
            MemoryArch::I32 => i32::stack_pop(&mut self.value_stack) as u32 as u64,
            MemoryArch::I64 => i64::stack_pop(&mut self.value_stack) as u64,
        };
        cold_err!(usize::try_from(value).map_err(|_| Trap::TableOutOfBounds {
            offset: usize::MAX,
            len: 1,
            max: usize::MAX,
        }))
    }
}

impl Drop for Executor<'_> {
    fn drop(&mut self) {
        core::mem::swap(&mut self.value_stack, self.stack_slot);
    }
}
