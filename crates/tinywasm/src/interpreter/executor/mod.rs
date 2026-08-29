use core::convert::identity;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use super::no_std_floats::NoStdFloatExt;

use alloc::boxed::Box;
use alloc::vec::Vec;

use interpreter::stack::{CallFrame, ValueStack};
use tinywasm_types::*;

use super::ExecState;
use super::num_helpers::*;
use super::values::*;
use crate::engine::FuelPolicy;
use crate::func::HostFunction;
use crate::interpreter::Value128;
use crate::*;

#[macro_use]
mod instructions;

#[cfg(not(feature = "nightly-tail-calls"))]
mod dispatch;
#[cfg(feature = "nightly-tail-calls")]
mod dispatch_become;

const CHECKPOINT_INTERVAL: u32 = 128;
const FUEL_COST_CALL_TOTAL: u32 = 5;

struct ExecError(Box<Error>);

type ExecResult<T> = core::result::Result<T, ExecError>;

impl From<Trap> for ExecError {
    #[cold]
    fn from(trap: Trap) -> Self {
        Self(Box::new(Error::Trap(trap)))
    }
}

impl From<Error> for ExecError {
    #[cold]
    fn from(error: Error) -> Self {
        Self(Box::new(error))
    }
}

impl From<ExecError> for Error {
    #[cold]
    fn from(error: ExecError) -> Self {
        *error.0
    }
}

#[derive(Clone, Copy)]
struct ExecFlow(usize);

impl ExecFlow {
    const COMPLETE: Self = Self(usize::MAX);

    #[inline(always)]
    fn next(instr_ptr: usize) -> Self {
        debug_assert_ne!(instr_ptr, Self::COMPLETE.0);
        Self(instr_ptr)
    }

    #[inline(always)]
    fn next_instr_ptr(self) -> Option<usize> {
        (self.0 != Self::COMPLETE.0).then_some(self.0)
    }
}

pub(crate) struct Executor<'store> {
    cf: CallFrame,
    func: Shared<WasmFunction>,
    module: ModuleInstance,
    store: &'store mut Store,
    call_stack_base: u32,
    mem0: MemAddr,
    fuel_metered: bool,
    #[cfg(feature = "nightly-tail-calls")]
    completed: bool,
}

impl<'store> Executor<'store> {
    pub(crate) fn new(store: &'store mut Store, cf: CallFrame, call_stack_base: u32) -> Self {
        let wasm_func = store.state.funcs.wasm(cf.func_addr);
        let module = store.get_module_instance(wasm_func.owner).expect("invalid module instance").clone();
        let mem0 = module.mem0_addr();
        Self {
            module,
            cf,
            func: wasm_func.func.clone(),
            store,
            call_stack_base,
            mem0,
            fuel_metered: false,
            #[cfg(feature = "nightly-tail-calls")]
            completed: false,
        }
    }

    /// Resolves a module-local memory index to its store address, caching the common memory-0 case.
    #[inline(always)]
    fn mem_addr(&self, idx: MemAddr) -> MemAddr {
        if idx == 0 { self.mem0 } else { self.module.resolve_mem_addr(idx) }
    }

    /// Switches the executor to another module, keeping the cached memory-0 address in sync.
    #[inline]
    fn set_module(&mut self, owner: ModuleInstanceId) {
        self.module = self.store.get_module_instance(owner).expect("invalid module instance").clone();
        self.mem0 = self.module.mem0_addr();
    }

    #[inline(always)]
    fn charge_call_fuel(&mut self, total_fuel_cost: u32) {
        if self.fuel_metered {
            let extra = match self.store.engine.config().fuel_policy {
                FuelPolicy::PerInstruction => 0,
                FuelPolicy::Weighted => total_fuel_cost.saturating_sub(1),
            };

            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(extra);
        }
    }

    #[inline(always)]
    fn exec_jump_if_ref<const ON_NULL: bool>(&mut self) -> bool {
        let is_null = ValueRef::stack_peek(&self.store.value_stack).is_null();
        if is_null {
            ValueRef::stack_pop(&mut self.store.value_stack);
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
        (T::local_get(&self.store.value_stack, &self.cf, local) == T::default()) == ON_ZERO
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_const32(&mut self, packed: PackedOp64<CmpOp, (u32, i32)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        packed.op.cmp(i32::stack_pop(&mut self.store.value_stack), operand.b()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_const64(&mut self, packed: PackedOp128<CmpOp, (u32, i64)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        packed.op.cmp(i64::stack_pop(&mut self.store.value_stack), operand.b()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_local32(&mut self, packed: PackedOp64<CmpOp, (u32, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::stack_pop(&mut self.store.value_stack);
        let rhs = i32::local_get(&self.store.value_stack, &self.cf, operand.b());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_local64(&mut self, packed: PackedOp64<CmpOp, (u32, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i64::stack_pop(&mut self.store.value_stack);
        let rhs = i64::local_get(&self.store.value_stack, &self.cf, operand.b());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_binop_local_const_jump(&mut self, packed: PackedOp128<BinOp, LocalUpdateOperand>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let value = i32::local_update(&mut self.store.value_stack, &self.cf, operand.local(), |value| {
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
        let lhs = i32::local_update(&mut self.store.value_stack, &self.cf, operand.local(), |value| {
            packed.op.0.exec(value as u32, operand.value() as u32) as i32
        });
        let rhs = i32::local_get(&self.store.value_stack, &self.cf, operand.right());
        packed.op.1.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_binop_stack_const_tee_local_jump(
        &mut self,
        packed: PackedOp128<BinOp, LocalUpdateOperand>,
    ) -> Result<Option<usize>, Trap> {
        let operand = packed.index.resolve(&self.func.data);
        let value = i32::stack_update(&mut self.store.value_stack, |lhs| {
            packed.op.exec(lhs as u32, operand.value() as u32) as i32
        });
        i32::local_set(&mut self.store.value_stack, &self.cf, operand.local(), value);
        Ok(((value == 0) == operand.on_zero()).then_some(operand.target() as usize))
    }

    #[inline(always)]
    fn exec_binop_global_const_jump(&mut self, packed: PackedOp128<BinOp, GlobalUpdateOperand>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let global = self.module.resolve_global_addr(operand.global());
        let value = i32::global_update(&mut self.store.state.globals, global, |value| {
            packed.op.exec(value as u32, operand.value() as u32) as i32
        });
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_inc_local_jump(&mut self, index: Operand128Idx<LocalUpdateOperand>) -> Option<usize> {
        let operand = index.resolve(&self.func.data);
        let value = i32::local_update(&mut self.store.value_stack, &self.cf, operand.local(), |value| {
            value.wrapping_add(operand.value())
        });
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_inc_stack_tee_local_jump(
        &mut self,
        index: Operand128Idx<LocalUpdateOperand>,
    ) -> Result<Option<usize>, Trap> {
        let operand = index.resolve(&self.func.data);
        let value = i32::stack_update(&mut self.store.value_stack, |value| value.wrapping_add(operand.value()));
        i32::local_set(&mut self.store.value_stack, &self.cf, operand.local(), value);
        Ok(((value == 0) == operand.on_zero()).then_some(operand.target() as usize))
    }

    #[inline(always)]
    fn exec_inc_global_jump(&mut self, index: Operand128Idx<GlobalUpdateOperand>) -> Option<usize> {
        let operand = index.resolve(&self.func.data);
        let global = self.module.resolve_global_addr(operand.global());
        let value =
            i32::global_update(&mut self.store.state.globals, global, |value| value.wrapping_add(operand.value()));
        ((value == 0) == operand.on_zero()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_inc_local_jump_cmp_local(&mut self, packed: PackedOp128<CmpOp, LocalUpdateCmpOperand>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::local_update(&mut self.store.value_stack, &self.cf, operand.local(), |value| {
            value.wrapping_add(operand.value())
        });
        let rhs = i32::local_get(&self.store.value_stack, &self.cf, operand.right());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_const32(&self, packed: PackedOp128<CmpOp, (u32, i32, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::local_get(&self.store.value_stack, &self.cf, operand.c());
        packed.op.cmp(lhs, operand.b()).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_const64(&self, packed: PackedOp128<CmpOp, (u32, i32, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i64::local_get(&self.store.value_stack, &self.cf, operand.c());
        packed.op.cmp(lhs, i64::from(operand.b())).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_local32(&self, packed: PackedOp64<CmpOp, (u32, u16, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i32::local_get(&self.store.value_stack, &self.cf, operand.b());
        let rhs = i32::local_get(&self.store.value_stack, &self.cf, operand.c());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_local64(&self, packed: PackedOp64<CmpOp, (u32, u16, u16)>) -> Option<usize> {
        let operand = packed.index.resolve(&self.func.data);
        let lhs = i64::local_get(&self.store.value_stack, &self.cf, operand.b());
        let rhs = i64::local_get(&self.store.value_stack, &self.cf, operand.c());
        packed.op.cmp(lhs, rhs).then_some(operand.target() as usize)
    }

    fn exec_i64_add128(&mut self) -> Result<(), Trap> {
        let b_hi = i64::stack_pop(&mut self.store.value_stack);
        let b_lo = i64::stack_pop(&mut self.store.value_stack);
        let a_hi = i64::stack_pop(&mut self.store.value_stack);
        let a_lo = i64::stack_pop(&mut self.store.value_stack);
        let lo = a_lo.wrapping_add(b_lo);
        let carry = u64::from((lo as u64) < (a_lo as u64));
        let hi = a_hi.wrapping_add(b_hi).wrapping_add(carry as i64);
        i64::stack_push(&mut self.store.value_stack, lo)?;
        i64::stack_push(&mut self.store.value_stack, hi)
    }

    fn exec_i64_sub128(&mut self) -> Result<(), Trap> {
        let b_hi = i64::stack_pop(&mut self.store.value_stack);
        let b_lo = i64::stack_pop(&mut self.store.value_stack);
        let a_hi = i64::stack_pop(&mut self.store.value_stack);
        let a_lo = i64::stack_pop(&mut self.store.value_stack);
        let lo = a_lo.wrapping_sub(b_lo);
        let borrow = u64::from((a_lo as u64) < (b_lo as u64));
        let hi = a_hi.wrapping_sub(b_hi).wrapping_sub(borrow as i64);
        i64::stack_push(&mut self.store.value_stack, lo)?;
        i64::stack_push(&mut self.store.value_stack, hi)
    }

    fn exec_i64_mul_wide_s(&mut self) -> Result<(), Trap> {
        let rhs = i64::stack_pop(&mut self.store.value_stack);
        let lhs = i64::stack_pop(&mut self.store.value_stack);
        let product = (lhs as i128).wrapping_mul(rhs as i128);
        i64::stack_push(&mut self.store.value_stack, product as i64)?;
        i64::stack_push(&mut self.store.value_stack, (product >> 64) as i64)
    }

    fn exec_i64_mul_wide_u(&mut self) -> Result<(), Trap> {
        let rhs = i64::stack_pop(&mut self.store.value_stack);
        let lhs = i64::stack_pop(&mut self.store.value_stack);
        let product = (lhs as u64 as u128).wrapping_mul(rhs as u64 as u128);
        i64::stack_push(&mut self.store.value_stack, product as u64 as i64)?;
        i64::stack_push(&mut self.store.value_stack, (product >> 64) as u64 as i64)
    }

    #[inline(always)]
    fn exec_simd_extract_lane<TO: InternalValue>(
        &mut self,
        lane: u8,
        operation: impl FnOnce(Value128, u8) -> TO,
    ) -> Result<(), Trap> {
        let vector = Value128::stack_pop(&mut self.store.value_stack);
        TO::stack_push(&mut self.store.value_stack, operation(vector, lane))
    }

    #[inline(always)]
    fn exec_simd_replace_lane<VALUE: InternalValue>(
        &mut self,
        lane: u8,
        operation: impl FnOnce(VALUE, Value128, u8) -> Value128,
    ) -> Result<(), Trap> {
        let vector = Value128::stack_pop(&mut self.store.value_stack);
        let value = VALUE::stack_pop(&mut self.store.value_stack);
        Value128::stack_push(&mut self.store.value_stack, operation(value, vector, lane))
    }

    fn exec_simd_shuffle(&mut self, lanes: Value128) -> Result<(), Trap> {
        let rhs = Value128::stack_pop(&mut self.store.value_stack);
        let lhs = Value128::stack_pop(&mut self.store.value_stack);
        Value128::stack_push(&mut self.store.value_stack, Value128::i8x16_shuffle(lhs, rhs, lanes))
    }

    #[inline(always)]
    fn exec_local_set_pop<T: InternalValue>(&mut self, local: LocalAddr) {
        let value = T::stack_pop(&mut self.store.value_stack);
        T::local_set(&mut self.store.value_stack, &self.cf, local, value);
    }

    #[inline(always)]
    fn exec_local_tee<T: InternalValue>(&mut self, local: LocalAddr) {
        let value = T::stack_peek(&self.store.value_stack);
        T::local_set(&mut self.store.value_stack, &self.cf, local, value);
    }

    #[inline(always)]
    fn exec_global_get<T: InternalValue>(&mut self, global: GlobalAddr) -> Result<(), Trap> {
        let addr = self.module.resolve_global_addr(global);
        let value = T::global_get(&self.store.state.globals, addr);
        T::stack_push(&mut self.store.value_stack, value)
    }

    #[inline(always)]
    fn exec_global_set<T: InternalValue>(&mut self, global: GlobalAddr) {
        let addr = self.module.resolve_global_addr(global);
        let value = T::stack_pop(&mut self.store.value_stack);
        T::global_set(&mut self.store.state.globals, addr, value);
    }

    #[inline(always)]
    fn exec_global_tee<T: InternalValue>(&mut self, global: GlobalAddr) {
        let addr = self.module.resolve_global_addr(global);
        let value = T::stack_peek(&self.store.value_stack);
        T::global_set(&mut self.store.state.globals, addr, value);
    }

    #[inline(always)]
    fn exec_binop_result<T: InternalValue, const PUSH: bool>(
        &mut self,
        dst: Option<LocalAddr>,
        val: T,
    ) -> Result<(), Trap> {
        if let Some(dst) = dst {
            T::local_set(&mut self.store.value_stack, &self.cf, dst, val);
        }
        if PUSH { T::stack_push(&mut self.store.value_stack, val) } else { Ok(()) }
    }

    #[inline(always)]
    fn exec_binop_local_local<T: InternalValue, const PUSH: bool>(
        &mut self,
        lhs: LocalAddr,
        rhs: LocalAddr,
        destination: Option<LocalAddr>,
        op: impl BinOpExt<T>,
    ) -> Result<(), Trap> {
        let lhs = T::local_get(&self.store.value_stack, &self.cf, lhs);
        let rhs = T::local_get(&self.store.value_stack, &self.cf, rhs);
        self.exec_binop_result::<T, PUSH>(destination, op.exec(lhs, rhs))
    }

    #[inline(always)]
    fn exec_binop_local_local_indexed<T: InternalValue, const PUSH: bool>(
        &mut self,
        index: Operand64Idx<(u16, u16, u16)>,
        op: impl BinOpExt<T>,
    ) -> Result<(), Trap> {
        let operands = index.resolve(&self.func.data);
        self.exec_binop_local_local::<T, PUSH>(operands.a(), operands.b(), Some(operands.c()), op)
    }

    #[inline(always)]
    fn exec_binop_local_const<T: InternalValue, const PUSH: bool>(
        &mut self,
        lhs: LocalAddr,
        rhs: T,
        destination: Option<LocalAddr>,
        op: impl BinOpExt<T>,
    ) -> Result<(), Trap> {
        let lhs = T::local_get(&self.store.value_stack, &self.cf, lhs);
        self.exec_binop_result::<T, PUSH>(destination, op.exec(lhs, rhs))
    }

    #[inline(always)]
    fn exec_binop_global_const<T: InternalValue>(
        &mut self,
        lhs: GlobalAddr,
        rhs: T,
        op: impl BinOpExt<T>,
    ) -> Result<(), Trap> {
        let lhs = T::global_get(&self.store.state.globals, self.module.resolve_global_addr(lhs));
        T::stack_push(&mut self.store.value_stack, op.exec(lhs, rhs))
    }

    #[inline(always)]
    fn exec_cmp_local_local<T>(&mut self, lhs: LocalAddr, rhs: LocalAddr, op: CmpOp) -> Result<(), Trap>
    where
        T: InternalValue,
        CmpOp: CmpOpExt<T>,
    {
        let lhs = T::local_get(&self.store.value_stack, &self.cf, lhs);
        let rhs = T::local_get(&self.store.value_stack, &self.cf, rhs);
        i32::stack_push(&mut self.store.value_stack, i32::from(op.cmp(lhs, rhs)))
    }

    #[inline(always)]
    fn exec_binop_stack_global<T: InternalValue>(
        &mut self,
        global: GlobalAddr,
        op: impl BinOpExt<T>,
    ) -> Result<(), Trap> {
        let addr = self.module.resolve_global_addr(global);
        let rhs = T::global_get(&self.store.state.globals, addr);
        T::stack_update(&mut self.store.value_stack, |lhs| op.exec(lhs, rhs));
        Ok(())
    }

    #[inline(always)]
    fn exec_binop_stack_local<T: InternalValue, const PUSH: bool>(
        &mut self,
        local: LocalAddr,
        destination: Option<LocalAddr>,
        op: impl BinOpExt<T>,
    ) -> Result<(), Trap> {
        let rhs = T::local_get(&self.store.value_stack, &self.cf, local);
        let lhs = T::stack_pop(&mut self.store.value_stack);
        self.exec_binop_result::<T, PUSH>(destination, op.exec(lhs, rhs))
    }

    #[inline(always)]
    fn exec_binop_const_tee<T: InternalValue>(
        &mut self,
        rhs: T,
        destination: LocalAddr,
        op: impl BinOpExt<T>,
    ) -> Result<(), Trap> {
        let value = T::stack_update(&mut self.store.value_stack, |lhs| op.exec(lhs, rhs));
        T::local_set(&mut self.store.value_stack, &self.cf, destination, value);
        Ok(())
    }

    #[inline(always)]
    fn exec_mul_acc_local<T: InternalValue>(
        &mut self,
        accumulator: LocalAddr,
        multiply: fn(T, T) -> T,
        add: fn(T, T) -> T,
    ) {
        let rhs = T::stack_pop(&mut self.store.value_stack);
        let lhs = T::stack_pop(&mut self.store.value_stack);
        let product = multiply(lhs, rhs);
        T::local_update(&mut self.store.value_stack, &self.cf, accumulator, |value| add(product, value));
    }

    fn exec_branch_table(&mut self, index: Operand128Idx<BranchTableOperand>) -> usize {
        let v = index.resolve(&self.func.data);
        let idx = <i32>::stack_pop(&mut self.store.value_stack);
        let target_ip = if idx >= 0 && (idx as u32) < v.size() {
            self.func.data.branch_table_targets.get((v.start() + idx as u32) as usize).copied().unwrap_or(v.target())
        } else {
            v.target()
        };

        target_ip as usize
    }

    fn create_exception(&mut self, tag_index: TagAddr) -> Result<ValueRef, Trap> {
        let tag_addr = self.module.resolve_tag_addr(tag_index);
        let type_addr = self.store.state.get_tag(tag_addr).type_addr;
        let payload_len = self.store.state.get_canonical_func_type(type_addr).params().len();
        self.store.state.gc.check_allocation(payload_len, true)?;
        let mut payload = Vec::new();
        cold_err!(payload.try_reserve_exact(payload_len)).map_err(|_| Trap::OutOfMemory)?;
        let value_stack = &mut self.store.value_stack;
        for index in (0..payload_len).rev() {
            let ty = self.store.state.get_canonical_func_type(type_addr).params()[index];
            payload.push(match ty {
                WasmType::I32 | WasmType::F32 => RuntimeValue::Value32(Value32::stack_pop(value_stack)),
                WasmType::I64 | WasmType::F64 => RuntimeValue::Value64(Value64::stack_pop(value_stack)),
                WasmType::V128 => RuntimeValue::Value128(Value128::stack_pop(value_stack)),
                WasmType::Ref(_) => RuntimeValue::ValueRef(ValueRef::stack_pop(value_stack)),
            });
        }
        payload.reverse();
        let roots = (&self.store.value_stack.stack_32).into_iter().copied().map(ValueRef::from_raw);
        self.store.state.alloc_exception(tag_addr, payload, roots)
    }

    fn exec_throw(&mut self, tag_index: TagAddr, instr_ptr: usize) -> ExecResult<ExecFlow> {
        let exception = self.create_exception(tag_index)?;
        self.throw_exception(exception, instr_ptr)
    }

    fn exec_throw_ref(&mut self, instr_ptr: usize) -> ExecResult<ExecFlow> {
        let exception = ValueRef::stack_pop(&mut self.store.value_stack);
        if exception.is_null() {
            return Err(Trap::NullReference.into());
        }
        self.throw_exception(exception, instr_ptr)
    }

    fn throw_exception(&mut self, exception: ValueRef, protected_ip: usize) -> ExecResult<ExecFlow> {
        match self.dispatch_exception(exception, protected_ip)? {
            Some(landing_pad) => Ok(ExecFlow::next(landing_pad)),
            None => match self.store.root_exception(exception) {
                Ok(exception) => Err(Error::Exception(exception).into()),
                Err(error) => Err(error.into()),
            },
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

        let wasm_func = self.store.state.funcs.wasm(self.cf.func_addr);
        if !Shared::ptr_eq(&self.func, &wasm_func.func) {
            self.func = wasm_func.func.clone();
        }
        if wasm_func.owner != self.module.id() {
            self.set_module(wasm_func.owner);
        }
    }

    fn dispatch_exception(&mut self, exception: ValueRef, mut protected_ip: usize) -> Result<Option<usize>, Trap> {
        let object = self.store.state.gc.get(exception).ok_or(Trap::InvalidReference)?;
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
                self.store.value_stack.truncate_to_base(target);
                if include_payload {
                    let Store { state, value_stack, .. } = self.store;
                    let object = state.gc.get(exception).ok_or(Trap::InvalidReference)?;
                    for value in object.values.iter().copied() {
                        value_stack.push_dyn(value)?;
                    }
                }
                if with_ref {
                    ValueRef::stack_push(&mut self.store.value_stack, exception)?;
                }
                return Ok(Some(landing_pad as usize));
            }

            self.store.value_stack.truncate_to_base(self.cf.locals_base);
            let Some(caller) = self.store.call_stack.pop_frame(self.call_stack_base) else {
                return Ok(None);
            };
            self.switch_to_frame(caller);
            protected_ip = self.cf.instr_ptr.checked_sub(1).expect("invalid caller IP");
        }
    }

    fn exec_call_host<const TAIL: bool>(
        &mut self,
        host_func: HostFunction,
        type_addr: TypeAddr,
        return_instr_ptr: usize,
    ) -> ExecResult<ExecFlow> {
        if let Some(host_func) = host_func.typed_callback() {
            cold_err!(host_func.call_stack(self.store, self.module.id(), type_addr))
                .map_err(|error| Trap::HostFunction(Box::new(error)))?;
            if TAIL {
                return Ok(self.exec_return());
            }
            return Ok(ExecFlow::next(return_instr_ptr));
        }

        let (param_count, result_count, base) = {
            let ty = self.store.state.get_canonical_func_type(type_addr);
            (ty.params().len(), ty.results().len(), self.store.value_stack.base_before(ty.params().iter().collect()))
        };
        let module_id = self.module.id();
        self.store
            .with_scratch_values(param_count + result_count, |store, values| {
                let host_values = store.stack_value_iter(type_addr, crate::store::FuncValueTypes::Params, base)?;
                for (slot, value) in values[..param_count].iter_mut().zip(host_values) {
                    *slot = value;
                }
                store.value_stack.truncate_to_base(base);
                let (params, results) = values.split_at_mut(param_count);
                host_func
                    .call_values(store, module_id, type_addr, params, results)
                    .map_err(|error| Error::Trap(Trap::HostFunction(Box::new(error))))?;
                store.push_wasm_values(results)
            })
            .map_err(|error| match error {
                Error::Trap(trap) => trap,
                other => Trap::HostFunction(Box::new(other)),
            })?;
        if TAIL { Ok(self.exec_return()) } else { Ok(ExecFlow::next(return_instr_ptr)) }
    }

    fn exec_call_direct(&mut self, v: u32, return_instr_ptr: usize) -> ExecResult<ExecFlow> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        if self.store.state.funcs.is_host(addr) {
            let host_func = self.store.state.funcs.host(addr);
            let type_addr = host_func.type_addr;
            let host_func = host_func.func.clone();
            self.exec_call_host::<false>(host_func, type_addr, return_instr_ptr)
        } else {
            self.exec_call_wasm::<false>(addr, return_instr_ptr)
        }
    }

    fn exec_return_call_direct(&mut self, v: u32) -> ExecResult<ExecFlow> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        if self.store.state.funcs.is_host(addr) {
            let host_func = self.store.state.funcs.host(addr);
            let type_addr = host_func.type_addr;
            let host_func = host_func.func.clone();
            self.exec_call_host::<true>(host_func, type_addr, 0)
        } else {
            self.exec_call_wasm::<true>(addr, 0)
        }
    }

    fn exec_call_self(&mut self, return_instr_ptr: usize) -> ExecResult<()> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let Ok(locals_base) = self.store.value_stack.enter_locals(&self.func.params, &self.func.locals) else {
            return cold!(Err(Trap::CallStackOverflow.into()));
        };
        let new = CallFrame::new(self.cf.func_addr, locals_base, self.func.locals);
        self.store.call_stack.push(core::mem::replace(&mut self.cf, new), return_instr_ptr)?;
        Ok(())
    }

    fn exec_return_call_self(&mut self) -> ExecResult<()> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);

        self.store.value_stack.truncate_keep_counts(self.cf.locals_base, self.func.params);
        let Ok(locals_base) = self.store.value_stack.enter_locals(&self.func.params, &self.func.locals) else {
            return cold!(Err(Trap::CallStackOverflow.into()));
        };
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
        let table_idx = self.pop_table_operand(self.store.state.get_table(table_addr).kind.arch())?;
        let table = self.store.state.get_table(table_addr);

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
        let type_addr = self.store.state.funcs.type_addr(func_addr);
        if !self.store.state.type_addr_is_subtype(type_addr, expected_type_addr) {
            return cold!(Err(Trap::IndirectCallTypeMismatch {
                actual: Box::new(self.store.state.get_canonical_func_type(type_addr).clone()),
                expected: Box::new(self.store.state.get_canonical_func_type(expected_type_addr).clone()),
            }
            .into()));
        }
        if self.store.state.funcs.is_host(func_addr) {
            let host_func = self.store.state.funcs.host(func_addr);
            let host_func = host_func.func.clone();
            return self.exec_call_host::<TAIL>(host_func, type_addr, return_instr_ptr);
        }

        self.exec_call_wasm::<TAIL>(func_addr, return_instr_ptr)
    }

    #[inline(always)]
    fn exec_call_wasm<const TAIL: bool>(
        &mut self,
        func_addr: FuncAddr,
        return_instr_ptr: usize,
    ) -> ExecResult<ExecFlow> {
        let wasm_func = self.store.state.funcs.wasm(func_addr);
        let (params, locals, owner, next_func) = {
            let next_func = (!Shared::ptr_eq(&self.func, &wasm_func.func)).then(|| wasm_func.func.clone());
            (wasm_func.func.params, wasm_func.func.locals, wasm_func.owner, next_func)
        };
        if TAIL {
            self.store.value_stack.truncate_keep_counts(self.cf.locals_base, params);
        }
        let locals_base = self.store.value_stack.enter_locals(&params, &locals)?;
        if TAIL {
            self.cf = CallFrame::new(func_addr, locals_base, locals);
        } else {
            self.store.call_stack.push(self.cf, return_instr_ptr)?;
            self.cf = CallFrame::new(func_addr, locals_base, locals);
        }
        if let Some(next_func) = next_func {
            self.func = next_func;
        }
        if owner != self.module.id() {
            self.set_module(owner);
        }
        Ok(ExecFlow::next(0))
    }

    fn exec_call_ref<const TAIL: bool>(&mut self, type_addr: u32, return_instr_ptr: usize) -> ExecResult<ExecFlow> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let func_ref = ValueRef::stack_pop(&mut self.store.value_stack);
        let Some(func_addr) = func_ref.addr() else {
            return cold!(Err(Trap::NullFunctionReference.into()));
        };

        self.exec_typed_call::<TAIL>(func_addr, self.module.resolve_type_addr(type_addr), return_instr_ptr)
    }

    fn exec_return(&mut self) -> ExecFlow {
        self.store.value_stack.truncate_keep_counts(self.cf.locals_base, self.func.results);
        self.finish_return()
    }

    #[inline(always)]
    fn finish_return(&mut self) -> ExecFlow {
        let Some(caller) = self.store.call_stack.pop_frame(self.call_stack_base) else {
            return ExecFlow::COMPLETE;
        };
        let instr_ptr = caller.instr_ptr;
        if caller.func_addr == self.cf.func_addr {
            self.cf = caller;
        } else {
            self.switch_to_frame(caller);
        }
        ExecFlow::next(instr_ptr)
    }

    fn exec_return_void(&mut self) -> ExecFlow {
        self.store.value_stack.truncate_to_base(self.cf.locals_base);
        self.finish_return()
    }

    fn exec_return_32(&mut self) -> ExecFlow {
        self.store.value_stack.stack_32.truncate_to_one_tail(self.cf.locals_base.s32 as usize);
        self.store.value_stack.stack_64.truncate_to(self.cf.locals_base.s64 as usize);
        self.store.value_stack.stack_128.truncate_to(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    fn exec_return_64(&mut self) -> ExecFlow {
        self.store.value_stack.stack_32.truncate_to(self.cf.locals_base.s32 as usize);
        self.store.value_stack.stack_64.truncate_to_one_tail(self.cf.locals_base.s64 as usize);
        self.store.value_stack.stack_128.truncate_to(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    fn exec_return_128(&mut self) -> ExecFlow {
        self.store.value_stack.stack_32.truncate_to(self.cf.locals_base.s32 as usize);
        self.store.value_stack.stack_64.truncate_to(self.cf.locals_base.s64 as usize);
        self.store.value_stack.stack_128.truncate_to_one_tail(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    fn exec_store_local_local<T: InternalValue + MemValue<N>, const N: usize>(
        &mut self,
        index: Operand64Idx<CompactMemoryOperand>,
        addr_local: u8,
        value_local: u8,
    ) -> Result<(), Trap> {
        let memarg = index.resolve(&self.func.data);
        let value = T::local_get(&self.store.value_stack, &self.cf, u16::from(value_local));
        let mem_addr = self.mem_addr(MemAddr::from(memarg.memory()));
        let mem = self.store.state.get_mem_mut(mem_addr);
        let addr = if mem.is_64bit() {
            let base = u64::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local));
            let base = cold_err!(usize::try_from(base).map_err(|_| Trap::MemoryOutOfBounds {
                offset: usize::MAX,
                len: N,
                max: mem.inner.len(),
            }))?;
            cold_err!(mem.effective_addr::<N>(base, u64::from(memarg.offset())))?
        } else {
            let base = u32::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local));
            cold_err!(mem.effective_addr::<N>(base as usize, u64::from(memarg.offset())))?
        };
        value.store_at(&mut mem.inner, addr)
    }

    fn exec_inc_memory_local<T: MemValue<N>, const N: usize>(
        &mut self,
        index: Operand64Idx<CompactMemoryOperand>,
        addr_local: u8,
        increment: impl FnOnce(T) -> T,
    ) -> Result<(), Trap> {
        let memarg = index.resolve(&self.func.data);
        let mem_addr = self.mem_addr(MemAddr::from(memarg.memory()));
        let mem = self.store.state.get_mem_mut(mem_addr);
        let addr = if mem.is_64bit() {
            let base = i64::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local)) as u64;
            let base = cold_err!(usize::try_from(base).map_err(|_| Trap::MemoryOutOfBounds {
                offset: usize::MAX,
                len: N,
                max: mem.inner.len(),
            }))?;
            cold_err!(mem.effective_addr::<N>(base, u64::from(memarg.offset())))?
        } else {
            let base = u32::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local));
            cold_err!(mem.effective_addr::<N>(base as usize, u64::from(memarg.offset())))?
        };

        let value = cold_err!(T::load_at(&mem.inner, addr))?;
        increment(value).store_at(&mut mem.inner, addr)
    }

    fn exec_fma_store<
        T: InternalValue + MemValue<N> + core::ops::Add<Output = T> + core::ops::Mul<Output = T>,
        const N: usize,
    >(
        &mut self,
        m: CompactMemoryArg,
    ) -> Result<(), Trap> {
        let rhs = T::stack_pop(&mut self.store.value_stack);
        let lhs = T::stack_pop(&mut self.store.value_stack);
        let acc = T::stack_pop(&mut self.store.value_stack);
        let fma = acc + lhs * rhs;
        let mem_addr = self.mem_addr(m.mem_addr());
        let mem = self.store.state.get_mem_mut(mem_addr);
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = cold_err!(mem.effective_addr::<N>(base, m.offset()))?;
        cold_err!(fma.store_at(&mut mem.inner, addr))
    }

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

        let mem = self.store.state.get_mem(self.mem_addr(MemAddr::from(memarg.memory())));
        let base = if mem.is_64bit() {
            let base = i64::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local)) as u64;
            cold_err!(usize::try_from(base)).map_err(|_| Trap::MemoryOutOfBounds {
                offset: usize::MAX,
                len: N,
                max: mem.inner.len(),
            })?
        } else {
            u32::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local)) as usize
        };

        let addr = cold_err!(mem.effective_addr::<N>(base, u64::from(memarg.offset())))?;
        let value = cast(cold_err!(LOAD::load_at(&mem.inner, addr))?);

        if SET_LOCAL {
            TARGET::local_set(&mut self.store.value_stack, &self.cf, u16::from(dst_local), value);
        }
        if !SET_LOCAL || TEE {
            TARGET::stack_push(&mut self.store.value_stack, value)?;
        }
        Ok(())
    }

    fn exec_ref_is_null(&mut self) -> Result<(), Trap> {
        let is_null = i32::from(ValueRef::stack_pop(&mut self.store.value_stack).is_null());
        i32::stack_push(&mut self.store.value_stack, is_null)
    }

    fn exec_ref_as_non_null(&mut self) -> Result<(), Trap> {
        if ValueRef::stack_peek(&self.store.value_stack).is_null() {
            return cold!(Err(Trap::NullReference));
        }
        Ok(())
    }

    fn canonical_ref_type(&self, ty: RefType) -> RefType {
        let Some(type_index) = ty.type_index() else { return ty };
        RefType::new_concrete(ty.is_nullable(), self.module.resolve_type_addr(type_index))
    }

    fn exec_ref_matches(&self, ty: RefType) -> bool {
        let value = ValueRef::stack_peek(&self.store.value_stack);
        self.store.state.value_ref_matches(value, self.canonical_ref_type(ty))
    }

    fn exec_ref_test(&mut self, ty: RefType) -> Result<(), Trap> {
        let value = ValueRef::stack_pop(&mut self.store.value_stack);
        let matches = self.store.state.value_ref_matches(value, self.canonical_ref_type(ty));
        i32::stack_push(&mut self.store.value_stack, i32::from(matches))
    }

    fn exec_ref_cast(&self, ty: RefType) -> Result<(), Trap> {
        if !self.exec_ref_matches(ty) {
            return cold!(Err(Trap::CastFailure));
        }
        Ok(())
    }

    fn exec_i31_get(&mut self, signed: bool) -> Result<(), Trap> {
        let value = ValueRef::stack_pop(&mut self.store.value_stack);
        if value.is_null() {
            return cold!(Err(Trap::NullI31Reference));
        }
        let value = if signed {
            value.i31_s().expect("validated i31.get operand")
        } else {
            value.i31_u().expect("validated i31.get operand") as i32
        };
        i32::stack_push(&mut self.store.value_stack, value)
    }

    fn push_gc_object(&mut self, type_addr: TypeAddr, values: Vec<RuntimeValue>) -> Result<(), Trap> {
        let roots = (&self.store.value_stack.stack_32).into_iter().copied().map(ValueRef::from_raw);
        let reference = self.store.state.alloc_gc_object(type_addr, values, roots)?;
        ValueRef::stack_push(&mut self.store.value_stack, reference)
    }

    fn exec_struct_new(&mut self, type_index: TypeAddr, default: bool) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let field_count =
            self.store.state.get_type(type_addr).as_struct().expect("validated struct.new type").fields.len();
        self.store.state.check_gc_allocation(type_addr, field_count)?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(field_count)).map_err(|_| Trap::OutOfMemory)?;
        if default {
            values.extend(
                self.store
                    .state
                    .get_type(type_addr)
                    .as_struct()
                    .expect("validated struct.new type")
                    .fields
                    .iter()
                    .map(|field| default_value(field.storage)),
            );
        } else {
            for index in (0..field_count).rev() {
                let storage = self.store.state.get_type(type_addr).as_struct().unwrap().fields[index].storage;
                values.push(pop_value(&mut self.store.value_stack, storage));
            }
            values.reverse();
        }
        self.push_gc_object(type_addr, values)
    }

    fn exec_struct_get(&mut self, index: Operand64Idx<(u32, u32)>, signed: Option<bool>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let field_index = operand.b();
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_struct().expect("validated struct.get type").fields
            [field_index as usize]
            .storage;
        let object = self.store.state.gc_object(reference, type_addr)?;
        let object = self.store.state.gc.get_handle(object).ok_or(Trap::Other("invalid GC reference"))?;
        let value = *object.values.get(field_index as usize).expect("validated struct field index");
        push_value(&mut self.store.value_stack, value, storage, signed)
    }

    fn exec_struct_set(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let field_index = operand.b();
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_struct().expect("validated struct.set type").fields
            [field_index as usize]
            .storage;
        let value = pop_value(&mut self.store.value_stack, storage);
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let object = self.store.state.gc_object(reference, type_addr)?;
        self.store.state.gc.set(object, field_index as usize, value).expect("live struct field");
        Ok(())
    }

    fn exec_array_new(&mut self, type_index: TypeAddr, default: bool) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array.new type").field.storage;
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        self.store.state.check_gc_allocation(type_addr, len)?;
        let value = if default { default_value(storage) } else { pop_value(&mut self.store.value_stack, storage) };
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
        let storage =
            self.store.state.get_type(type_addr).as_array().expect("validated array.new_fixed type").field.storage;
        let len = len as usize;
        self.store.state.check_gc_allocation(type_addr, len)?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        for _ in 0..len {
            values.push(pop_value(&mut self.store.value_stack, storage));
        }
        values.reverse();
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_get(&mut self, type_index: TypeAddr, signed: Option<bool>) -> Result<(), Trap> {
        let index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array.get type").field.storage;
        let object = self.store.state.gc_object(reference, type_addr)?;
        let object = self.store.state.gc.get_handle(object).ok_or(Trap::Other("invalid GC reference"))?;
        let value = *object.values.get(index).ok_or(Trap::ArrayOutOfBounds)?;
        push_value(&mut self.store.value_stack, value, storage, signed)
    }

    fn exec_array_set(&mut self, type_index: TypeAddr) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array.set type").field.storage;
        let value = pop_value(&mut self.store.value_stack, storage);
        let index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let object = self.store.state.gc_object(reference, type_addr)?;
        self.store.state.gc.set(object, index, value).ok_or(Trap::ArrayOutOfBounds)
    }

    fn exec_array_len(&mut self) -> Result<(), Trap> {
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        if reference.is_null() {
            return Err(Trap::NullArrayReference);
        }
        let object = self.store.state.gc.get(reference).ok_or(Trap::Other("invalid GC reference"))?;
        let crate::store::GcObjectKind::Composite(type_addr) = object.kind else {
            return Err(Trap::Other("GC reference is not an array"));
        };
        if self.store.state.get_type(type_addr).as_array().is_none() {
            return Err(Trap::Other("GC reference is not an array"));
        }
        i32::stack_push(&mut self.store.value_stack, object.values.len() as i32)
    }

    fn exec_array_fill(&mut self, type_index: TypeAddr) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array.fill type").field.storage;
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let value = pop_value(&mut self.store.value_stack, storage);
        let index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let object = self.store.state.gc_object(reference, type_addr)?;
        let object_ref = self.store.state.gc.get_handle(object).ok_or(Trap::Other("invalid GC reference"))?;
        let end = index.checked_add(len).filter(|end| *end <= object_ref.values.len()).ok_or(Trap::ArrayOutOfBounds)?;
        self.store.state.gc.fill(object, index..end, value).expect("live array range");
        Ok(())
    }

    fn exec_array_copy(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let dst_type = operand.a();
        let src_type = operand.b();
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src_index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = ValueRef::stack_pop(&mut self.store.value_stack);
        let dst_index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let dst = ValueRef::stack_pop(&mut self.store.value_stack);
        let dst_type = self.module.resolve_type_addr(dst_type);
        let src_type = self.module.resolve_type_addr(src_type);
        let dst_handle = self.store.state.gc_object(dst, dst_type)?;
        let dst_len = self.store.state.gc.get_handle(dst_handle).expect("validated array").values.len();
        let src_handle = self.store.state.gc_object(src, src_type)?;
        let src_object = self.store.state.gc.get_handle(src_handle).expect("validated array");
        let src_end =
            src_index.checked_add(len).filter(|end| *end <= src_object.values.len()).ok_or(Trap::ArrayOutOfBounds)?;
        dst_index.checked_add(len).filter(|end| *end <= dst_len).ok_or(Trap::ArrayOutOfBounds)?;
        if src_handle == dst_handle {
            self.store.state.gc.copy_within(dst_handle, src_index..src_end, dst_index).expect("live array range");
            return Ok(());
        }
        self.store
            .state
            .gc
            .copy_between(src_handle, src_index..src_end, dst_handle, dst_index)
            .expect("live array ranges");
        Ok(())
    }

    fn exec_array_new_data(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let data_index = operand.b();
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = u32::stack_pop(&mut self.store.value_stack) as usize;
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array type").field.storage;
        let data_addr = self.module.resolve_data_addr(data_index);
        let data = self.store.state.data[data_addr as usize].data.as_deref().unwrap_or(&[]);
        data_range(storage, data, src, len)?;
        self.store.state.check_gc_allocation(type_addr, len)?;
        let values = decode_data(storage, data, src, len)?;
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_new_elem(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let elem_index = operand.b();
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = u32::stack_pop(&mut self.store.value_stack) as usize;
        let type_addr = self.module.resolve_type_addr(type_index);
        let elem_addr = self.module.resolve_elem_addr(elem_index);
        let items = self.store.state.elements[elem_addr as usize].items_range(src, len)?;
        self.store.state.check_gc_allocation(type_addr, len)?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        values.extend(items.iter().copied().map(RuntimeValue::ValueRef));
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_init_data(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let data_index = operand.b();
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = u32::stack_pop(&mut self.store.value_stack) as usize;
        let dst = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array type").field.storage;
        let object = self.store.state.gc_object(reference, type_addr)?;
        let object_len = self.store.state.gc.get_handle(object).expect("validated array").values.len();
        dst.checked_add(len).filter(|end| *end <= object_len).ok_or(Trap::ArrayOutOfBounds)?;
        let data =
            self.store.state.data[self.module.resolve_data_addr(data_index) as usize].data.as_deref().unwrap_or(&[]);
        let values = decode_data(storage, data, src, len)?;
        self.store.state.gc.set_slice(object, dst, &values).expect("live array range");
        Ok(())
    }

    fn exec_array_init_elem(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let type_index = operand.a();
        let elem_index = operand.b();
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = u32::stack_pop(&mut self.store.value_stack) as usize;
        let dst = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let object = self.store.state.gc_object(reference, type_addr)?;
        let object_len = self.store.state.gc.get_handle(object).expect("validated array").values.len();
        dst.checked_add(len).filter(|end| *end <= object_len).ok_or(Trap::ArrayOutOfBounds)?;
        let items =
            self.store.state.elements[self.module.resolve_elem_addr(elem_index) as usize].items_range(src, len)?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        values.extend(items.iter().copied().map(RuntimeValue::ValueRef));
        self.store.state.gc.set_slice(object, dst, &values).expect("live array range");
        Ok(())
    }

    fn exec_memory_size(&mut self, addr: u32) -> Result<(), Trap> {
        let mem = self.store.state.get_mem(self.mem_addr(addr));
        match mem.is_64bit() {
            true => i64::stack_push(&mut self.store.value_stack, mem.page_count as i64),
            false => i32::stack_push(&mut self.store.value_stack, mem.page_count as i32),
        }
    }

    fn exec_memory_grow(&mut self, addr: u32) -> Result<(), Trap> {
        let mem_addr = self.mem_addr(addr);
        let limiter = self.store.engine.config().resource_limiter.as_deref();
        let mem = self.store.state.get_mem_mut(mem_addr);
        let is_64bit = mem.is_64bit();
        let pages_delta = match is_64bit {
            true => i64::stack_pop(&mut self.store.value_stack),
            false => i64::from(i32::stack_pop(&mut self.store.value_stack)),
        };

        let size = mem.grow(pages_delta, limiter)?.unwrap_or(-1);
        match is_64bit {
            true => i64::stack_push(&mut self.store.value_stack, size)?,
            false => i32::stack_push(&mut self.store.value_stack, size as i32)?,
        };

        Ok(())
    }

    fn exec_memory_copy(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let dst_mem = operand.a();
        let src_mem = operand.b();
        let dst_mem_addr = self.mem_addr(dst_mem);
        let src_mem_addr = self.mem_addr(src_mem);
        let dst_arch = self.store.state.get_mem(dst_mem_addr).kind.arch();
        let src_arch = self.store.state.get_mem(src_mem_addr).kind.arch();
        let len_arch =
            if dst_arch == MemoryArch::I32 || src_arch == MemoryArch::I32 { MemoryArch::I32 } else { MemoryArch::I64 };
        let size = self.store.value_stack.pop_memory_operand(len_arch)?;
        let src = self.store.value_stack.pop_memory_operand(src_arch)?;
        let dst = self.store.value_stack.pop_memory_operand(dst_arch)?;

        if dst_mem_addr == src_mem_addr {
            // copy within the same memory
            let mem = self.store.state.get_mem_mut(dst_mem_addr);
            mem.copy_within(dst, src, size)?;
        } else {
            // copy between two memories
            let (dst_memory, src_memory) = self.store.state.get_mems_mut(dst_mem_addr, src_mem_addr);
            dst_memory.copy_from_memory(dst, src_memory, src, size)?;
        }
        Ok(())
    }

    fn exec_memory_fill(&mut self, addr: u32) -> Result<(), Trap> {
        let mem_addr = self.mem_addr(addr);
        let arch = self.store.state.get_mem(mem_addr).kind.arch();
        let size = self.store.value_stack.pop_memory_operand(arch)?;
        let val = i32::stack_pop(&mut self.store.value_stack);
        let dst = self.store.value_stack.pop_memory_operand(arch)?;
        self.exec_memory_fill_impl(mem_addr, dst, val as u8, size)
    }

    fn exec_memory_fill_const(&mut self, index: Operand128Idx<MemoryFillOperand>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let mem_addr = self.mem_addr(operand.memory());
        let arch = self.store.state.get_mem(mem_addr).kind.arch();
        let dst = self.store.value_stack.pop_memory_operand(arch)?;
        self.exec_memory_fill_impl(mem_addr, dst, operand.byte(), operand.value() as u32 as usize)
    }

    fn exec_memory_fill_impl(&mut self, mem_addr: MemAddr, dst: usize, val: u8, size: usize) -> Result<(), Trap> {
        let mem = self.store.state.get_mem_mut(mem_addr);
        let max = mem.inner.len();
        if mem.inner.fill(dst, size, val).is_none() {
            return cold!(Err(Trap::MemoryOutOfBounds { offset: dst, len: size, max }));
        }
        Ok(())
    }

    fn exec_memory_init(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let data_index = operand.a();
        let mem_index = operand.b();
        let size = u32::stack_pop(&mut self.store.value_stack) as usize;
        let offset = u32::stack_pop(&mut self.store.value_stack) as usize;
        let mem_addr = self.mem_addr(mem_index);
        let arch = self.store.state.get_mem(mem_addr).kind.arch();
        let dst = self.store.value_stack.pop_memory_operand(arch)?;

        let data = &self.store.state.data[self.module.resolve_data_addr(data_index) as usize];
        let mem = &mut self.store.state.memories[mem_addr as usize];
        let data_len = data.data.as_ref().map_or(0, |d| d.len());
        let mem_len = mem.inner.len();
        if offset.checked_add(size).is_none_or(|end| end > data_len) {
            return cold!(Err(Trap::MemoryOutOfBounds { offset, len: size, max: data_len }));
        }
        if dst.checked_add(size).is_none_or(|end| end > mem_len) {
            return cold!(Err(Trap::MemoryOutOfBounds { offset: dst, len: size, max: mem_len }));
        }

        if size == 0 {
            return Ok(());
        }

        let Some(data) = &data.data else {
            return cold!(Err(Trap::MemoryOutOfBounds { offset: 0, len: 0, max: 0 }));
        };

        if mem.inner.write_all(dst, &data[offset..offset + size]).is_none() {
            return cold!(Err(Trap::MemoryOutOfBounds { offset: dst, len: size, max: mem_len }));
        }
        Ok(())
    }

    fn exec_table_copy(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let dst_table = operand.a();
        let src_table = operand.b();
        let dst_table_addr = self.module.resolve_table_addr(dst_table);
        let src_table_addr = self.module.resolve_table_addr(src_table);
        let dst_arch = self.store.state.get_table(dst_table_addr).kind.arch();
        let src_arch = self.store.state.get_table(src_table_addr).kind.arch();
        let len_arch =
            if dst_arch == MemoryArch::I32 || src_arch == MemoryArch::I32 { MemoryArch::I32 } else { MemoryArch::I64 };
        let size = self.pop_table_operand(len_arch)?;
        let src = self.pop_table_operand(src_arch)?;
        let dst = self.pop_table_operand(dst_arch)?;

        if dst_table_addr == src_table_addr {
            // copy within the same table
            self.store.state.get_table_mut(dst_table_addr).copy_within(dst, src, size)
        } else {
            // copy between two tables
            let (dst_table_ref, src_table_ref) = self.store.state.get_tables_mut(dst_table_addr, src_table_addr);
            dst_table_ref.copy_from_slice(dst, src_table_ref.load(src, size)?)
        }
    }

    fn exec_mem_load_lane<LOAD: MemValue<LOAD_SIZE>, const LOAD_SIZE: usize>(
        &mut self,
        arg: MemoryLaneArg,
    ) -> Result<(), Trap> {
        let m = arg.memory_arg_idx.resolve(&self.func.data);
        let mem = self.store.state.get_mem(self.mem_addr(m.memory()));
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = cold_err!(mem.effective_addr::<LOAD_SIZE>(base, m.offset()))?;
        let val = cold_err!(LOAD::load_at(&mem.inner, addr))?;
        let offset = arg.lane as usize * LOAD_SIZE;
        Value128::stack_update(&mut self.store.value_stack, |value| {
            let mut bytes = value.to_mem_bytes();
            bytes[offset..offset + LOAD_SIZE].copy_from_slice(&val.to_mem_bytes());
            Value128(bytes)
        });
        Ok(())
    }

    #[inline(always)]
    fn exec_mem_load<LOAD: MemValue<LOAD_SIZE>, const LOAD_SIZE: usize, TARGET: InternalValue>(
        &mut self,
        m: Operand128<MemoryOperand>,
        cast: impl Fn(LOAD) -> TARGET,
    ) -> Result<(), Trap> {
        let mem = self.store.state.get_mem(self.mem_addr(m.memory()));
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = cold_err!(mem.effective_addr::<LOAD_SIZE>(base, m.offset()))?;
        let value = cold_err!(LOAD::load_at(&mem.inner, addr))?;
        TARGET::stack_push(&mut self.store.value_stack, cast(value))
    }

    #[inline(always)]
    fn exec_mem_store_lane<U: MemValue<N> + Copy, const N: usize>(&mut self, arg: MemoryLaneArg) -> Result<(), Trap> {
        let bytes = Value128::stack_pop(&mut self.store.value_stack).to_mem_bytes();
        let lane_offset = arg.lane as usize * N;
        let mut val_bytes = [0u8; N];
        val_bytes.copy_from_slice(&bytes[lane_offset..lane_offset + N]);
        let val = U::from_mem_bytes(val_bytes);
        let m = arg.memory_arg_idx.resolve(&self.func.data);
        let mem_addr = self.mem_addr(m.memory());
        let mem = self.store.state.get_mem_mut(mem_addr);
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = cold_err!(mem.effective_addr::<N>(base, m.offset()))?;
        cold_err!(val.store_at(&mut mem.inner, addr))?;
        Ok(())
    }

    #[inline(always)]
    fn exec_mem_store<T: InternalValue, U: MemValue<N>, const N: usize>(
        &mut self,
        memory: Operand128<MemoryOperand>,
        cast: impl Fn(T) -> U,
    ) -> Result<(), Trap> {
        let val = cast(<T>::stack_pop(&mut self.store.value_stack));
        self.exec_mem_store_value(self.mem_addr(memory.memory()), memory.offset(), val)
    }

    #[inline(always)]
    fn exec_select_store<T: InternalValue + MemValue<N>, const N: usize>(
        &mut self,
        memory: Operand128<MemoryOperand>,
    ) -> Result<(), Trap> {
        let condition = Value32::stack_pop(&mut self.store.value_stack);
        let false_value = T::stack_pop(&mut self.store.value_stack);
        let true_value = T::stack_pop(&mut self.store.value_stack);
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
        let mem = self.store.state.get_mem_mut(memory_addr);
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = cold_err!(mem.effective_addr::<N>(base, offset))?;
        cold_err!(val.store_at(&mut mem.inner, addr))
    }

    fn exec_table_get(&mut self, table_index: u32) -> Result<(), Trap> {
        let table_addr = self.module.resolve_table_addr(table_index);
        let idx = self.pop_table_operand(self.store.state.get_table(table_addr).kind.arch())?;
        let value = *self.store.state.get_table(table_addr).get(idx)?;
        ValueRef::stack_push(&mut self.store.value_stack, value)
    }

    fn exec_table_set(&mut self, table_index: u32) -> Result<(), Trap> {
        let val = ValueRef::stack_pop(&mut self.store.value_stack);
        let table_addr = self.module.resolve_table_addr(table_index);
        let idx = self.pop_table_operand(self.store.state.get_table(table_addr).kind.arch())?;
        let table = self.store.state.get_table_mut(table_addr);
        table.set(idx, val)
    }

    fn exec_table_size(&mut self, table_index: u32) -> Result<(), Trap> {
        let table = self.store.state.get_table(self.module.resolve_table_addr(table_index));
        match table.kind.arch() {
            MemoryArch::I32 => i32::stack_push(&mut self.store.value_stack, table.size() as i32),
            MemoryArch::I64 => i64::stack_push(&mut self.store.value_stack, table.size() as i64),
        }
    }

    fn exec_table_init(&mut self, index: Operand64Idx<(u32, u32)>) -> Result<(), Trap> {
        let operand = index.resolve(&self.func.data);
        let elem_index = operand.a();
        let table_index = operand.b();
        let size = self.pop_table_operand(MemoryArch::I32)?; // n
        let offset = self.pop_table_operand(MemoryArch::I32)?; // s
        let table_addr = self.module.resolve_table_addr(table_index);
        let dst = self.pop_table_operand(self.store.state.get_table(table_addr).kind.arch())?; // d
        let elem_addr = self.module.resolve_elem_addr(elem_index) as usize;
        let elem = self.store.state.elements.get(elem_addr).ok_or_else(|| Trap::Other("element not found"))?;
        let items = elem.items_range(offset, size)?;

        let table =
            self.store.state.tables.get_mut(table_addr as usize).ok_or_else(|| Trap::Other("table not found"))?;
        table.init(dst, items)
    }

    fn exec_table_grow(&mut self, table_index: u32) -> Result<(), Trap> {
        let table_addr = self.module.resolve_table_addr(table_index);
        let arch = self.store.state.get_table(table_addr).kind.arch();
        let n = self.pop_table_operand(arch)?;
        let val = ValueRef::stack_pop(&mut self.store.value_stack);
        let limiter = self.store.engine.config().resource_limiter.as_deref();
        let table = self.store.state.get_table_mut(table_addr);
        let sz = table.size();
        let grew = table.grow(n, val, limiter)?;
        match (arch, grew) {
            (MemoryArch::I32, true) => i32::stack_push(&mut self.store.value_stack, sz as i32),
            (MemoryArch::I32, false) => i32::stack_push(&mut self.store.value_stack, -1),
            (MemoryArch::I64, true) => i64::stack_push(&mut self.store.value_stack, sz as i64),
            (MemoryArch::I64, false) => i64::stack_push(&mut self.store.value_stack, -1),
        }
    }

    fn exec_table_fill(&mut self, table_index: u32) -> Result<(), Trap> {
        let table_addr = self.module.resolve_table_addr(table_index);
        let arch = self.store.state.get_table(table_addr).kind.arch();
        let n = self.pop_table_operand(arch)?;
        let val = ValueRef::stack_pop(&mut self.store.value_stack);
        let i = self.pop_table_operand(arch)?;
        self.store.state.get_table_mut(table_addr).fill(i, n, val)
    }

    fn pop_table_operand(&mut self, arch: MemoryArch) -> Result<usize, Trap> {
        let value = match arch {
            MemoryArch::I32 => i32::stack_pop(&mut self.store.value_stack) as u32 as u64,
            MemoryArch::I64 => i64::stack_pop(&mut self.store.value_stack) as u64,
        };
        cold_err!(usize::try_from(value).map_err(|_| Trap::TableOutOfBounds {
            offset: usize::MAX,
            len: 1,
            max: usize::MAX,
        }))
    }
}
