use core::hint::cold_path;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use super::no_std_floats::NoStdFloatExt;

use alloc::boxed::Box;
use alloc::vec::Vec;

use alloc::sync::Arc;
use interpreter::stack::CallFrame;
use tinywasm_types::*;

use super::ExecState;
use super::num_helpers::*;
use super::values::*;
use crate::engine::FuelPolicy;
use crate::func::HostFunction;
use crate::interpreter::Value128;
use crate::*;

const FUEL_COST_CALL_TOTAL: u32 = 5;

pub(crate) struct Executor<'store, const BUDGETED: bool> {
    cf: CallFrame,
    func: Arc<WasmFunction>,
    module: ModuleInstance,
    store: &'store mut Store,
    call_stack_base: u32,
}

impl<'store, const BUDGETED: bool> Executor<'store, BUDGETED> {
    pub(crate) fn new(store: &'store mut Store, cf: CallFrame, call_stack_base: u32) -> Self {
        let wasm_func = store.state.get_wasm_func(cf.func_addr);
        let module = store
            .get_module_instance(wasm_func.owner)
            .unwrap_or_else(|| unreachable!("invalid module instance"))
            .clone();
        Self { module, cf, func: wasm_func.func.clone(), store, call_stack_base }
    }

    #[inline(always)]
    fn charge_call_fuel(&mut self, total_fuel_cost: u32) {
        if BUDGETED {
            let extra = match self.store.engine.config().fuel_policy {
                FuelPolicy::PerInstruction => 0,
                FuelPolicy::Weighted => total_fuel_cost.saturating_sub(1),
            };

            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(extra);
        }
    }

    #[inline(always)]
    fn exec(&mut self, instr_ptr: usize) -> Result<Option<()>> {
        macro_rules! exec_op {
            (binary_fallible $ty:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                let $rhs = <$ty>::stack_pop(&mut self.store.value_stack);
                let $lhs = <$ty>::stack_pop(&mut self.store.value_stack);
                <$ty>::stack_push(&mut self.store.value_stack, $expr?)?;
            }};
            (unary $from:ty => $to:ty, |$v:ident| $expr:expr) => {{
                let $v = <$from>::stack_pop(&mut self.store.value_stack);
                <$to>::stack_push(&mut self.store.value_stack, $expr)?;
            }};
            (binary $from:ty => $to:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                let $rhs = <$from>::stack_pop(&mut self.store.value_stack);
                let $lhs = <$from>::stack_pop(&mut self.store.value_stack);
                <$to>::stack_push(&mut self.store.value_stack, $expr)?;
            }};
            (binary_two_results $from:ty => $to:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                let $rhs = <$from>::stack_pop(&mut self.store.value_stack);
                let $lhs = <$from>::stack_pop(&mut self.store.value_stack);
                let out = $expr;
                <$to>::stack_push(&mut self.store.value_stack, out.0)?;
                <$to>::stack_push(&mut self.store.value_stack, out.1)?;
            }};
            (binary_mixed $lhs_ty:ty, $rhs_ty:ty => $res:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                let $rhs = <$rhs_ty>::stack_pop(&mut self.store.value_stack);
                let $lhs = <$lhs_ty>::stack_pop(&mut self.store.value_stack);
                <$res>::stack_push(&mut self.store.value_stack, $expr)?;
            }};
            (ternary $from:ty => $to:ty, |$a:ident, $b:ident, $c:ident| $expr:expr) => {{
                let $c = <$from>::stack_pop(&mut self.store.value_stack);
                let $b = <$from>::stack_pop(&mut self.store.value_stack);
                let $a = <$from>::stack_pop(&mut self.store.value_stack);
                <$to>::stack_push(&mut self.store.value_stack, $expr)?;
            }};
            (quaternary_two_results $from:ty => $to:ty, |$a:ident, $b:ident, $c:ident, $d:ident| $expr:expr) => {{
                let $d = <$from>::stack_pop(&mut self.store.value_stack);
                let $c = <$from>::stack_pop(&mut self.store.value_stack);
                let $b = <$from>::stack_pop(&mut self.store.value_stack);
                let $a = <$from>::stack_pop(&mut self.store.value_stack);
                let out = $expr;
                <$to>::stack_push(&mut self.store.value_stack, out.0)?;
                <$to>::stack_push(&mut self.store.value_stack, out.1)?;
            }};
            (local_set_pop $ty:ty, $local_index:expr) => {{
                let val = <$ty>::stack_pop(&mut self.store.value_stack);
                <$ty>::local_set(&mut self.store.value_stack, &self.cf, *$local_index, val);
            }};
            (local_tee $ty:ty, $local_index:expr) => {{
                let val = <$ty>::stack_peek(&self.store.value_stack);
                <$ty>::local_set(&mut self.store.value_stack, &self.cf, *$local_index, val);
            }};
            (global_get $ty:ty, $global_index:expr) => {{
                let addr = self.module.resolve_global_addr(*$global_index);
                let value = <$ty>::global_get(&self.store.state.globals, addr);
                <$ty>::stack_push(&mut self.store.value_stack, value)?;
            }};
            (global_set $ty:ty, $global_index:expr) => {{
                let addr = self.module.resolve_global_addr(*$global_index);
                let value = <$ty>::stack_pop(&mut self.store.value_stack);
                <$ty>::global_set(&mut self.store.state.globals, addr, value);
            }};
            (global_tee $ty:ty, $global_index:expr) => {{
                let addr = self.module.resolve_global_addr(*$global_index);
                let value = <$ty>::stack_peek(&self.store.value_stack);
                <$ty>::global_set(&mut self.store.state.globals, addr, value);
            }};
            (binop_local_local $vt:ty, $exec:ident, $op:ident, $a:ident, $b:ident) => {{
                let lhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$a);
                let rhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$b);
                <$vt>::stack_push(&mut self.store.value_stack, $exec(*$op, lhs, rhs))?;
            }};
            (binop_local_local_set $vt:ty, $exec:ident, $op:ident, $a:ident, $b:ident, $dst:ident) => {{
                let lhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$a);
                let rhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$b);
                let value = $exec(*$op, lhs, rhs);
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
            }};
            (binop_local_local_tee $vt:ty, $exec:ident, $op:ident, $a:ident, $b:ident, $dst:ident) => {{
                let lhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$a);
                let rhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$b);
                let value = $exec(*$op, lhs, rhs);
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
                <$vt>::stack_push(&mut self.store.value_stack, value)?;
            }};
            (cmp_local_local $vt:ty, $cmp:ident, $op:ident, $a:ident, $b:ident) => {{
                let lhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$a);
                let rhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$b);
                self.store.value_stack.push(i32::from($cmp(lhs, rhs, *$op)))?;
            }};
            (binop_local_const $vt:ty, $exec:ident, $op:ident, $local:ident, $rhs:expr) => {{
                let lhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$local);
                <$vt>::stack_push(&mut self.store.value_stack, $exec(*$op, lhs, $rhs))?;
            }};
            (binop_local_const_set $vt:ty, $exec:ident, $op:ident, $local:ident, $rhs:expr, $dst:ident) => {{
                let lhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$local);
                let value = $exec(*$op, lhs, $rhs);
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
            }};
            (binop_local_const_tee $vt:ty, $exec:ident, $op:ident, $local:ident, $rhs:expr, $dst:ident) => {{
                let lhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$local);
                let value = $exec(*$op, lhs, $rhs);
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
                <$vt>::stack_push(&mut self.store.value_stack, value)?;
            }};
            (binop_global_const $vt:ty, $exec:ident, $op:ident, $global:ident, $rhs:expr) => {{
                let lhs = <$vt>::global_get(&self.store.state.globals, self.module.resolve_global_addr(*$global));
                self.store.value_stack.push($exec(*$op, lhs, $rhs))?;
            }};
            (binop_stack_global $vt:ty, $exec:ident, $op:ident, $global:ident) => {{
                let global_val =
                    <$vt>::global_get(&self.store.state.globals, self.module.resolve_global_addr(*$global));
                let stack_val = <$vt>::stack_pop(&mut self.store.value_stack);
                self.store.value_stack.push($exec(*$op, stack_val, global_val))?;
            }};
            (binop_stack_local $vt:ty, $exec:ident, $op:ident, $local:ident) => {{
                let rhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$local);
                let lhs = <$vt>::stack_pop(&mut self.store.value_stack);
                <$vt>::stack_push(&mut self.store.value_stack, $exec(*$op, lhs, rhs))?;
            }};
            (binop_stack_local_set $vt:ty, $exec:ident, $op:ident, $local:ident, $dst:ident) => {{
                let rhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$local);
                let lhs = <$vt>::stack_pop(&mut self.store.value_stack);
                let value = $exec(*$op, lhs, rhs);
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
            }};
            (binop_stack_local_tee $vt:ty, $exec:ident, $op:ident, $local:ident, $dst:ident) => {{
                let rhs = <$vt>::local_get(&self.store.value_stack, &self.cf, *$local);
                let lhs = <$vt>::stack_pop(&mut self.store.value_stack);
                let value = $exec(*$op, lhs, rhs);
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
                <$vt>::stack_push(&mut self.store.value_stack, value)?;
            }};
            (binop_acc_local $ty:ty, $acc:ident, $mul:expr, $add:expr) => {{
                let rhs = <$ty>::stack_pop(&mut self.store.value_stack);
                let lhs = <$ty>::stack_pop(&mut self.store.value_stack);
                let value = ($mul)(lhs, rhs);
                <$ty>::local_update(&mut self.store.value_stack, &self.cf, *$acc, |acc| ($add)(value, acc));
            }};
        }

        use tinywasm_types::Instruction::*;
        #[rustfmt::skip]
        match &self.func.instructions[instr_ptr] {
            Unreachable => { cold_path(); return Err(Trap::Unreachable.into()) },
            Drop32 => { _ = Value32::stack_pop(&mut self.store.value_stack)},
            Drop64 => { _ = Value64::stack_pop(&mut self.store.value_stack)},
            Drop128 => { _ = Value128::stack_pop(&mut self.store.value_stack)},
            Select32 => Value32::stack_select(&mut self.store.value_stack),
            Select64 => Value64::stack_select(&mut self.store.value_stack),
            Select128 => Value128::stack_select(&mut self.store.value_stack),
            SelectMulti(counts) => self.store.value_stack.select_multi(*counts),
            Call(v) => { self.exec_call_direct(*v)?; return Ok(None); }
            CallSelf => { self.exec_call_self()?; return Ok(None); }
            CallIndirect(ty, table) => { self.exec_call_indirect::<false>(*ty, *table)?; return Ok(None); }
            CallRef(ty) => { self.exec_call_ref::<false>(*ty)?; return Ok(None); }
            ReturnCall(v) => { if self.exec_return_call_direct(*v)? { return Ok(Some(())); } return Ok(None); }
            ReturnCallSelf => { self.exec_return_call_self()?; return Ok(None); }
            ReturnCallIndirect(ty, table) => { if self.exec_call_indirect::<true>(*ty, *table)? { return Ok(Some(())); } return Ok(None); }
            ReturnCallRef(ty) => { if self.exec_call_ref::<true>(*ty)? { return Ok(Some(())); } return Ok(None); }
            Throw(tag) => { self.exec_throw(*tag)?; return Ok(None); }
            ThrowRef => { self.exec_throw_ref()?; return Ok(None); }
            Jump(ip) => { self.cf.instr_ptr = *ip as usize; return Ok(None); }
            JumpIfZero32(ip) => if Self::exec_jump_if(&mut self.cf, *ip, |_| i32::stack_pop(&mut self.store.value_stack) == 0) { return Ok(None) },
            JumpIfNonZero32(ip) => if Self::exec_jump_if(&mut self.cf, *ip, |_| i32::stack_pop(&mut self.store.value_stack) != 0) { return Ok(None) },
            JumpIfZero64(ip) => if Self::exec_jump_if(&mut self.cf, *ip, |_| i64::stack_pop(&mut self.store.value_stack) == 0) { return Ok(None) },
            JumpIfNonZero64(ip) => if Self::exec_jump_if(&mut self.cf, *ip, |_| i64::stack_pop(&mut self.store.value_stack) != 0) { return Ok(None) },
            JumpIfRefNull(ip) => if Self::exec_jump_if(&mut self.cf, *ip, |_| {
                let is_null = ValueRef::stack_peek(&self.store.value_stack).is_null();
                if is_null { ValueRef::stack_pop(&mut self.store.value_stack); }
                is_null
            }) { return Ok(None) },
            JumpIfRefNonNull(ip) => if Self::exec_jump_if(&mut self.cf, *ip, |_| {
                let is_non_null = !ValueRef::stack_peek(&self.store.value_stack).is_null();
                if !is_non_null { ValueRef::stack_pop(&mut self.store.value_stack); }
                is_non_null
            }) { return Ok(None) },
            BrOnCast(ip, ty, on_fail) => if self.exec_ref_matches(*ty) == *on_fail { self.cf.instr_ptr = *ip as usize; return Ok(None); },
            JumpIfLocalZero32 { target_ip, local } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| Value32::local_get(&self.store.value_stack, cf, *local) == 0) { return Ok(None) },
            JumpIfLocalNonZero32 { target_ip, local } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| Value32::local_get(&self.store.value_stack, cf, *local) != 0) { return Ok(None) },
            JumpIfLocalZero64 { target_ip, local } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| Value64::local_get(&self.store.value_stack, cf, *local) == 0) { return Ok(None) },
            JumpIfLocalNonZero64 { target_ip, local } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| Value64::local_get(&self.store.value_stack, cf, *local) != 0) { return Ok(None) },
            JumpCmpStackConst32 { target_ip, imm, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |_| cmp_i32(i32::stack_pop(&mut self.store.value_stack), *imm, *op)) { return Ok(None) },
            JumpCmpStackConst64 { target_ip, imm, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |_| cmp_i64(i64::stack_pop(&mut self.store.value_stack), *imm, *op)) { return Ok(None) },
            JumpCmpStackLocal32 { target_ip, local, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| {
                let lhs = i32::stack_pop(&mut self.store.value_stack);
                cmp_i32(lhs, i32::local_get(&self.store.value_stack, cf, *local), *op)
            }) { return Ok(None) },
            JumpCmpStackLocal64 { target_ip, local, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| {
                let lhs = i64::stack_pop(&mut self.store.value_stack);
                cmp_i64(lhs, i64::local_get(&self.store.value_stack, cf, *local), *op)
            }) { return Ok(None) },
            BinOpLocalConstJump32 { target_ip, local, imm, op, on_zero } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| {
                let value = exec_binop_32(*op, i32::local_get(&self.store.value_stack, cf, *local) as u32, *imm as u32) as i32;
                i32::local_set(&mut self.store.value_stack, cf, *local, value);
                (value == 0) == *on_zero
            }) { return Ok(None) },
            BinOpLocalConstJumpCmpLocal32 { target_ip, local, imm, binop, right, cmp } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| {
                let lhs = exec_binop_32(*binop, i32::local_get(&self.store.value_stack, cf, *local) as u32, *imm as u32) as i32;
                i32::local_set(&mut self.store.value_stack, cf, *local, lhs);
                cmp_i32(lhs, i32::local_get(&self.store.value_stack, cf, *right), *cmp)
            }) { return Ok(None) },
            BinOpStackConstTeeLocalJump32 { target_ip, local, imm, op, on_zero } => {
                let value = exec_binop_32(*op, i32::stack_pop(&mut self.store.value_stack) as u32, *imm as u32) as i32;
                i32::local_set(&mut self.store.value_stack, &self.cf, *local, value);
                i32::stack_push(&mut self.store.value_stack, value)?;
                if Self::exec_jump_if(&mut self.cf, *target_ip, |_| (value == 0) == *on_zero) { return Ok(None) }
            },
            BinOpGlobalConstJump32 { target_ip, global, imm, op, on_zero } => if Self::exec_jump_if(&mut self.cf, *target_ip, |_| {
                let global = self.module.resolve_global_addr(*global);
                let value = exec_binop_32(*op, i32::global_get(&self.store.state.globals, global) as u32, *imm as u32) as i32;
                i32::global_set(&mut self.store.state.globals, global, value);
                (value == 0) == *on_zero
            }) { return Ok(None) },
            IncLocalJump32 { target_ip, local, delta, on_zero } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| {
                let value = i32::local_get(&self.store.value_stack, cf, *local).wrapping_add(*delta);
                i32::local_set(&mut self.store.value_stack, cf, *local, value);
                (value == 0) == *on_zero
            }) { return Ok(None) },
            IncStackTeeLocalJump32 { target_ip, local, delta, on_zero } => {
                let value = i32::stack_pop(&mut self.store.value_stack).wrapping_add(*delta);
                i32::local_set(&mut self.store.value_stack, &self.cf, *local, value);
                i32::stack_push(&mut self.store.value_stack, value)?;
                if Self::exec_jump_if(&mut self.cf, *target_ip, |_| (value == 0) == *on_zero) { return Ok(None) }
            },
            IncGlobalJump32 { target_ip, global, delta, on_zero } => if Self::exec_jump_if(&mut self.cf, *target_ip, |_| {
                let global = self.module.resolve_global_addr(*global);
                let value = i32::global_get(&self.store.state.globals, global).wrapping_add(*delta);
                i32::global_set(&mut self.store.state.globals, global, value);
                (value == 0) == *on_zero
            }) { return Ok(None) },
            IncLocalJumpCmpLocal32 { target_ip, local, delta, right, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| {
                let lhs = i32::local_get(&self.store.value_stack, cf, *local).wrapping_add(*delta);
                i32::local_set(&mut self.store.value_stack, cf, *local, lhs);
                cmp_i32(lhs, i32::local_get(&self.store.value_stack, cf, *right), *op)
            }) { return Ok(None) },
            JumpCmpLocalConst32 { target_ip, local, imm, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| cmp_i32(i32::local_get(&self.store.value_stack, cf, *local), *imm, *op)) { return Ok(None) },
            JumpCmpLocalConst64 { target_ip, local, imm, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| cmp_i64(i64::local_get(&self.store.value_stack, cf, *local), i64::from(*imm), *op)) { return Ok(None) },
            JumpCmpLocalLocal32 { target_ip, left, right, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| cmp_i32(i32::local_get(&self.store.value_stack, cf, *left), i32::local_get(&self.store.value_stack, cf, *right), *op)) { return Ok(None) },
            JumpCmpLocalLocal64 { target_ip, left, right, op } => if Self::exec_jump_if(&mut self.cf, *target_ip, |cf| cmp_i64(i64::local_get(&self.store.value_stack, cf, *left), i64::local_get(&self.store.value_stack, cf, *right), *op)) { return Ok(None) },
            DropKeep(drop_keep) => self.exec_drop_keep(*drop_keep),
            BranchTable(default_ip, start, len) => { self.exec_branch_table(*default_ip, *start, *len); return Ok(None); }
            Return => { if self.exec_return() { return Ok(Some(())); } return Ok(None); }
            ReturnVoid => { if self.exec_return_void() { return Ok(Some(())); } return Ok(None); }
            Return32 => { if self.exec_return_32() { return Ok(Some(())); } return Ok(None); }
            Return64 => { if self.exec_return_64() { return Ok(Some(())); } return Ok(None); }
            Return128 => { if self.exec_return_128() { return Ok(Some(())); } return Ok(None); }
            LocalGet32(local_index) => self.store.value_stack.push(Value32::local_get(&self.store.value_stack, &self.cf, *local_index))?,
            LocalGet64(local_index) => self.store.value_stack.push(Value64::local_get(&self.store.value_stack, &self.cf, *local_index))?,
            LocalGet128(local_index) => self.store.value_stack.push(Value128::local_get(&self.store.value_stack, &self.cf, *local_index))?,
            LocalSet32(local_index) => exec_op!(local_set_pop Value32, local_index),
            LocalSet64(local_index) => exec_op!(local_set_pop Value64, local_index),
            LocalSet128(local_index) => exec_op!(local_set_pop Value128, local_index),
            LocalCopy32(from, to) => Value32::local_copy(&mut self.store.value_stack, &self.cf, *from, *to),
            LocalCopy64(from, to) => Value64::local_copy(&mut self.store.value_stack, &self.cf, *from, *to),
            LocalCopy128(from, to) => Value128::local_copy(&mut self.store.value_stack, &self.cf, *from, *to),
            AddConst32(c) => exec_op!(unary i32 => i32, |v| v.wrapping_add(*c)),
            AddConst64(c) => exec_op!(unary i64 => i64, |v| v.wrapping_add(*c)),
            IncLocal32(local_index, delta) => i32::local_update(&mut self.store.value_stack, &self.cf, *local_index, |v| v.wrapping_add(*delta)),
            IncLocal64(local_index, delta) => i64::local_update(&mut self.store.value_stack, &self.cf, *local_index, |v| v.wrapping_add(*delta )),
            I32Add3 => exec_op!(ternary i32 => i32, |a, b, c| a.wrapping_add(b).wrapping_add(c)),
            I64Add3 => exec_op!(ternary i64 => i64, |a, b, c| a.wrapping_add(b).wrapping_add(c)),
            MulAccLocal32(acc) => exec_op!(binop_acc_local i32, acc, |a: i32, b| a.wrapping_mul(b), |a: i32, b| a.wrapping_add(b)),
            MulAccLocal64(acc) => exec_op!(binop_acc_local i64, acc, |a: i64, b| a.wrapping_mul(b), |a: i64, b| a.wrapping_add(b)),
            FMulAccLocal32(acc) => exec_op!(binop_acc_local f32, acc, |a: f32, b| a * b, |a: f32, b| a + b),
            FMulAccLocal64(acc) => exec_op!(binop_acc_local f64, acc, |a: f64, b| a * b, |a: f64, b| a + b),
            BinOpLocalLocal32(op, a, b) => exec_op!(binop_local_local Value32, exec_binop_32, op, a, b),
            BinOpLocalLocal64(op, a, b) => exec_op!(binop_local_local Value64, exec_binop_64, op, a, b),
            BinOpLocalLocal128(op, a, b) => exec_op!(binop_local_local Value128, exec_binop_128, op, a, b),
            CmpLocalLocal32(op, a, b) => exec_op!(cmp_local_local i32, cmp_i32, op, a, b),
            CmpLocalLocal64(op, a, b) => exec_op!(cmp_local_local i64, cmp_i64, op, a, b),
            BinOpLocalLocalSet32(op, a, b, dst) => exec_op!(binop_local_local_set Value32, exec_binop_32, op, a, b, dst),
            BinOpLocalLocalSet64(op, a, b, dst) => exec_op!(binop_local_local_set Value64, exec_binop_64, op, a, b, dst),
            BinOpLocalLocalSet128(op, a, b, dst) => exec_op!(binop_local_local_set Value128, exec_binop_128, op, a, b, dst),
            BinOpLocalLocalTee32(op, a, b, dst) => exec_op!(binop_local_local_tee Value32, exec_binop_32, op, a, b, dst),
            BinOpLocalLocalTee64(op, a, b, dst) => exec_op!(binop_local_local_tee Value64, exec_binop_64, op, a, b, dst),
            BinOpLocalLocalTee128(op, a, b, dst) => exec_op!(binop_local_local_tee Value128, exec_binop_128, op, a, b, dst),
            BinOpLocalConst32(op, local_index, c) => exec_op!(binop_local_const Value32, exec_binop_32, op, local_index, *c as u32),
            BinOpLocalConst64(op, local_index, c) => exec_op!(binop_local_const Value64, exec_binop_64, op, local_index, *c as u64),
            BinOpLocalConst128(op, local_index, c) => exec_op!(binop_local_const Value128, exec_binop_128, op, local_index, Value128(self.func.data.v128_const(*c))),
            BinOpGlobalConst32(op, global_index, c) => exec_op!(binop_global_const Value32, exec_binop_32, op, global_index, *c as u32),
            BinOpGlobalConst64(op, global_index, c) => exec_op!(binop_global_const Value64, exec_binop_64, op, global_index, *c as u64),
            BinOpGlobalConst128(op, global_index, c) => exec_op!(binop_global_const Value128, exec_binop_128, op, global_index, Value128(self.func.data.v128_const(*c))),
            BinOpLocalConstSet32(op, local_index, c, dst) => exec_op!(binop_local_const_set Value32, exec_binop_32, op, local_index, *c as u32, dst),
            BinOpLocalConstSet64(op, local_index, c, dst) => exec_op!(binop_local_const_set Value64, exec_binop_64, op, local_index, *c as u64, dst),
            BinOpLocalConstSet128(op, local_index, c, dst) => exec_op!(binop_local_const_set Value128, exec_binop_128, op, local_index, Value128(self.func.data.v128_const(*c)), dst),
            BinOpLocalConstTee32(op, local_index, c, dst) => exec_op!(binop_local_const_tee Value32, exec_binop_32, op, local_index, *c as u32, dst),
            BinOpLocalConstTee64(op, local_index, c, dst) => exec_op!(binop_local_const_tee Value64, exec_binop_64, op, local_index, *c as u64, dst),
            BinOpLocalConstTee128(op, local_index, c, dst) => exec_op!(binop_local_const_tee Value128, exec_binop_128, op, local_index, Value128(self.func.data.v128_const(*c)), dst),
            BinOpStackGlobal32(op, global_index) => exec_op!(binop_stack_global Value32, exec_binop_32, op, global_index),
            BinOpStackGlobal64(op, global_index) => exec_op!(binop_stack_global Value64, exec_binop_64, op, global_index),
            BinOpStackLocal32(op, local) => exec_op!(binop_stack_local Value32, exec_binop_32, op, local),
            BinOpStackLocalSet32(op, local, dst) => exec_op!(binop_stack_local_set Value32, exec_binop_32, op, local, dst),
            BinOpStackLocalTee32(op, local, dst) => exec_op!(binop_stack_local_tee Value32, exec_binop_32, op, local, dst),
            SetLocalConst32(local_index, c) => i32::local_set(&mut self.store.value_stack, &self.cf, *local_index, *c),
            SetLocalConst64(local_index, c) => i64::local_set(&mut self.store.value_stack, &self.cf, *local_index, *c),
            SetLocalConst128(local_index, c) => Value128::local_set(&mut self.store.value_stack, &self.cf, *local_index, Value128(self.func.data.v128_const(*c))),
            IncMemoryLocal32(m, addr_local) => self.exec_inc_memory_local::<i32, 4>(*m, *addr_local, |v| v.wrapping_add(1))?,
            IncMemoryLocal64(m, addr_local) => self.exec_inc_memory_local::<i64, 8>(*m, *addr_local, |v| v.wrapping_add(1))?,
            StoreLocalLocal32(m, addr_local, value_local) => self.exec_store_local_local::<u32, 4>(*m, *addr_local, *value_local)?,
            StoreLocalLocal64(m, addr_local, value_local) => self.exec_store_local_local::<i64, 8>(*m, *addr_local, *value_local)?,
            StoreLocalLocal128(m, addr_local, value_local) => self.exec_store_local_local::<Value128, 16>(*m, *addr_local, *value_local)?,
            LoadLocal32(m, addr_local) => self.exec_load_local::<i32, 4, _, false, false>(*m, *addr_local, 0, |v| v)?,
            LoadLocal64(m, addr_local) => self.exec_load_local::<i64, 8, _, false, false>(*m, *addr_local, 0, |v| v)?,
            LoadLocal8S32(m, addr_local) => self.exec_load_local::<i8, 1, _, false, false>(*m, *addr_local, 0, i32::from)?,
            LoadLocal8U32(m, addr_local) => self.exec_load_local::<u8, 1, _, false, false>(*m, *addr_local, 0, i32::from)?,
            LoadLocal16S32(m, addr_local) => self.exec_load_local::<i16, 2, _, false, false>(*m, *addr_local, 0, i32::from)?,
            LoadLocal16U32(m, addr_local) => self.exec_load_local::<u16, 2, _, false, false>(*m, *addr_local, 0, i32::from)?,
            LoadLocalTee32(m, addr_local, dst_local) => self.exec_load_local::<i32, 4, _, true, true>(*m, *addr_local, *dst_local, |v| v)?,
            LoadLocalSet32(m, addr_local, dst_local) => self.exec_load_local::<i32, 4, _, true, false>(*m, *addr_local, *dst_local, |v| v)?,
            LoadLocalTee8S32(m, addr_local, dst_local) => self.exec_load_local::<i8, 1, _, true, true>(*m, *addr_local, *dst_local, i32::from)?,
            LoadLocalTee8U32(m, addr_local, dst_local) => self.exec_load_local::<u8, 1, _, true, true>(*m, *addr_local, *dst_local, i32::from)?,
            LoadLocalTee16S32(m, addr_local, dst_local) => self.exec_load_local::<i16, 2, _, true, true>(*m, *addr_local, *dst_local, i32::from)?,
            LoadLocalTee16U32(m, addr_local, dst_local) => self.exec_load_local::<u16, 2, _, true, true>(*m, *addr_local, *dst_local, i32::from)?,
            LoadLocalSet8S32(m, addr_local, dst_local) => self.exec_load_local::<i8, 1, _, true, false>(*m, *addr_local, *dst_local, i32::from)?,
            LoadLocalSet8U32(m, addr_local, dst_local) => self.exec_load_local::<u8, 1, _, true, false>(*m, *addr_local, *dst_local, i32::from)?,
            LoadLocalSet16S32(m, addr_local, dst_local) => self.exec_load_local::<i16, 2, _, true, false>(*m, *addr_local, *dst_local, i32::from)?,
            LoadLocalSet16U32(m, addr_local, dst_local) => self.exec_load_local::<u16, 2, _, true, false>(*m, *addr_local, *dst_local, i32::from)?,
            LoadLocalTee128(m, addr_local, dst_local) => self.exec_load_local::<Value128, 16, _, true, true>(*m, *addr_local, *dst_local, |v| v)?,
            LoadLocalSet128(m, addr_local, dst_local) => self.exec_load_local::<Value128, 16, _, true, false>(*m, *addr_local, *dst_local, |v| v)?,
            AndConstTee32(c, local_index) => { exec_op!(unary i32 => i32, |v| v & *c); exec_op!(local_tee i32, local_index); }
            SubConstTee32(c, local_index) => { exec_op!(unary i32 => i32, |v| v.wrapping_sub(*c)); exec_op!(local_tee i32, local_index); }
            AndConstTee64(c, local_index) => { exec_op!(unary i64 => i64, |v| v & *c); exec_op!(local_tee i64, local_index); }
            SubConstTee64(c, local_index) => { exec_op!(unary i64 => i64, |v| v.wrapping_sub(*c)); exec_op!(local_tee i64, local_index); }
            LocalTee32(local_index) => exec_op!(local_tee Value32, local_index),
            LocalTee64(local_index) => exec_op!(local_tee Value64, local_index),
            LocalTee128(local_index) => exec_op!(local_tee Value128, local_index),
            GlobalGet32(global_index) => exec_op!(global_get Value32, global_index),
            GlobalGet64(global_index) => exec_op!(global_get Value64, global_index),
            GlobalGet128(global_index) => exec_op!(global_get Value128, global_index),
            GlobalSet32(global_index) => exec_op!(global_set Value32, global_index),
            GlobalSet64(global_index) => exec_op!(global_set Value64, global_index),
            GlobalSet128(global_index) => exec_op!(global_set Value128, global_index),
            GlobalTee32(global_index) => exec_op!(global_tee Value32, global_index),
            GlobalTee64(global_index) => exec_op!(global_tee Value64, global_index),
            GlobalTee128(global_index) => exec_op!(global_tee Value128, global_index),
            Const32(val) => i32::stack_push(&mut self.store.value_stack, *val)?,
            Const64(val) => i64::stack_push(&mut self.store.value_stack, *val)?,
            I64Eqz => exec_op!(unary i64 => i32, |v| i32::from(v == 0)),
            I32Eqz => exec_op!(unary i32 => i32, |v| i32::from(v == 0)),
            I32Eq => exec_op!(binary i32 => i32, |a, b| i32::from(a == b)),
            I64Eq => exec_op!(binary i64 => i32, |a, b| i32::from(a == b)),
            F32Eq => exec_op!(binary f32 => i32, |a, b| i32::from(a == b)),
            F64Eq => exec_op!(binary f64 => i32, |a, b| i32::from(a == b)),
            I32Ne => exec_op!(binary i32 => i32, |a, b| i32::from(a != b)),
            I64Ne => exec_op!(binary i64 => i32, |a, b| i32::from(a != b)),
            F32Ne => exec_op!(binary f32 => i32, |a, b| i32::from(a != b)),
            F64Ne => exec_op!(binary f64 => i32, |a, b| i32::from(a != b)),
            I32LtS => exec_op!(binary i32 => i32, |a, b| i32::from(a < b)),
            I64LtS => exec_op!(binary i64 => i32, |a, b| i32::from(a < b)),
            I32LtU => exec_op!(binary u32 => i32, |a, b| i32::from(a < b)),
            I64LtU => exec_op!(binary u64 => i32, |a, b| i32::from(a < b)),
            F32Lt => exec_op!(binary f32 => i32, |a, b| i32::from(a < b)),
            F64Lt => exec_op!(binary f64 => i32, |a, b| i32::from(a < b)),
            I32LeS => exec_op!(binary i32 => i32, |a, b| i32::from(a <= b)),
            I64LeS => exec_op!(binary i64 => i32, |a, b| i32::from(a <= b)),
            I32LeU => exec_op!(binary u32 => i32, |a, b| i32::from(a <= b)),
            I64LeU => exec_op!(binary u64 => i32, |a, b| i32::from(a <= b)),
            F32Le => exec_op!(binary f32 => i32, |a, b| i32::from(a <= b)),
            F64Le => exec_op!(binary f64 => i32, |a, b| i32::from(a <= b)),
            I32GeS => exec_op!(binary i32 => i32, |a, b| i32::from(a >= b)),
            I64GeS => exec_op!(binary i64 => i32, |a, b| i32::from(a >= b)),
            I32GeU => exec_op!(binary u32 => i32, |a, b| i32::from(a >= b)),
            I64GeU => exec_op!(binary u64 => i32, |a, b| i32::from(a >= b)),
            F32Ge => exec_op!(binary f32 => i32, |a, b| i32::from(a >= b)),
            F64Ge => exec_op!(binary f64 => i32, |a, b| i32::from(a >= b)),
            I32GtS => exec_op!(binary i32 => i32, |a, b| i32::from(a > b)),
            I64GtS => exec_op!(binary i64 => i32, |a, b| i32::from(a > b)),
            I32GtU => exec_op!(binary u32 => i32, |a, b| i32::from(a > b)),
            I64GtU => exec_op!(binary u64 => i32, |a, b| i32::from(a > b)),
            F32Gt => exec_op!(binary f32 => i32, |a, b| i32::from(a > b)),
            F64Gt => exec_op!(binary f64 => i32, |a, b| i32::from(a > b)),
            I32Add => exec_op!(binary i32 => i32, |a, b| a.wrapping_add(b)),
            I64Add => exec_op!(binary i64 => i64, |a, b| a.wrapping_add(b)),
            F32Add => exec_op!(binary f32 => f32, |a, b| a + b),
            F64Add => exec_op!(binary f64 => f64, |a, b| a + b),
            I32Sub => exec_op!(binary i32 => i32, |a, b| a.wrapping_sub(b)),
            I64Sub => exec_op!(binary i64 => i64, |a, b| a.wrapping_sub(b)),
            F32Sub => exec_op!(binary f32 => f32, |a, b| a - b),
            F64Sub => exec_op!(binary f64 => f64, |a, b| a - b),
            F32Div => exec_op!(binary f32 => f32, |a, b| a / b),
            F64Div => exec_op!(binary f64 => f64, |a, b| a / b),
            I32Mul => exec_op!(binary i32 => i32, |a, b| a.wrapping_mul(b)),
            I64Mul => exec_op!(binary i64 => i64, |a, b| a.wrapping_mul(b)),
            F32Mul => exec_op!(binary f32 => f32, |a, b| a * b),
            F64Mul => exec_op!(binary f64 => f64, |a, b| a * b),
            I32DivS => exec_op!(binary_fallible i32, |a, b| a.tw_checked_div(b)),
            I64DivS => exec_op!(binary_fallible i64, |a, b| a.tw_checked_div(b)),
            I32DivU => exec_op!(binary_fallible u32, |a, b| a.checked_div(b).ok_or(Trap::DivisionByZero)),
            I64DivU => exec_op!(binary_fallible u64, |a, b| a.checked_div(b).ok_or(Trap::DivisionByZero)),
            I32RemS => exec_op!(binary_fallible i32, |a, b| a.tw_checked_wrapping_rem(b)),
            I64RemS => exec_op!(binary_fallible i64, |a, b| a.tw_checked_wrapping_rem(b)),
            I32RemU => exec_op!(binary_fallible u32, |a, b| a.tw_checked_wrapping_rem(b)),
            I64RemU => exec_op!(binary_fallible u64, |a, b| a.tw_checked_wrapping_rem(b)),
            I32And => exec_op!(binary i32 => i32, |a, b| a & b),
            I64And => exec_op!(binary i64 => i64, |a, b| a & b),
            I32Or => exec_op!(binary i32 => i32, |a, b| a | b),
            I64Or => exec_op!(binary i64 => i64, |a, b| a | b),
            I32Xor => exec_op!(binary i32 => i32, |a, b| a ^ b),
            I64Xor => exec_op!(binary i64 => i64, |a, b| a ^ b),
            I32Shl => exec_op!(binary i32 => i32, |a, b| a.wrapping_shl(b as u32)),
            I64Shl => exec_op!(binary i64 => i64, |a, b| a.wrapping_shl(b as u32)),
            I32ShrS => exec_op!(binary i32 => i32, |a, b| a.wrapping_shr(b as u32)),
            I64ShrS => exec_op!(binary i64 => i64, |a, b| a.wrapping_shr(b as u32)),
            I32ShrU => exec_op!(binary u32 => u32, |a, b| a.wrapping_shr(b)),
            I64ShrU => exec_op!(binary u64 => u64, |a, b| a.wrapping_shr(b as u32)),
            I32Rotl => exec_op!(binary i32 => i32, |a, b| a.rotate_left(b as u32)),
            I64Rotl => exec_op!(binary i64 => i64, |a, b| a.rotate_left(b as u32)),
            I32Rotr => exec_op!(binary i32 => i32, |a, b| a.rotate_right(b as u32)),
            I64Rotr => exec_op!(binary i64 => i64, |a, b| a.rotate_right(b as u32)),
            I64Add128 => exec_op!(quaternary_two_results i64 => i64, |a_lo, a_hi, b_lo, b_hi| {
                let lo = a_lo.wrapping_add(b_lo);
                let carry = u64::from((lo as u64) < (a_lo as u64));
                let hi = a_hi.wrapping_add(b_hi).wrapping_add(carry as i64);
                (lo, hi)
            }),
            I64Sub128 => exec_op!(quaternary_two_results i64 => i64, |a_lo, a_hi, b_lo, b_hi| {
                let lo = a_lo.wrapping_sub(b_lo);
                let borrow = u64::from((a_lo as u64) < (b_lo as u64));
                let hi = a_hi.wrapping_sub(b_hi).wrapping_sub(borrow as i64);
                (lo, hi)
            }),
            I64MulWideS => exec_op!(binary_two_results i64 => i64, |a, b| {
                let product = (a as i128).wrapping_mul(b as i128);
                (product as i64, (product >> 64) as i64)
            }),
            I64MulWideU => exec_op!(binary_two_results i64 => i64, |a, b| {
                let product = (a as u64 as u128).wrapping_mul(b as u64 as u128);
                (product as u64 as i64, (product >> 64) as u64 as i64)
            }),
            I32Clz => exec_op!(unary i32 => i32, |v| v.leading_zeros() as i32),
            I64Clz => exec_op!(unary i64 => i64, |v| i64::from(v.leading_zeros())),
            I32Ctz => exec_op!(unary i32 => i32, |v| v.trailing_zeros() as i32),
            I64Ctz => exec_op!(unary i64 => i64, |v| i64::from(v.trailing_zeros())),
            I32Popcnt => exec_op!(unary i32 => i32, |v| v.count_ones() as i32),
            I64Popcnt => exec_op!(unary i64 => i64, |v| i64::from(v.count_ones())),

            // Reference types
            RefFunc(func_idx) => ValueRef::stack_push(&mut self.store.value_stack, ValueRef::from_category_addr(self.module.resolve_func_addr(*func_idx)))?,
            RefNull(_) => ValueRef::stack_push(&mut self.store.value_stack, ValueRef::NULL)?,
            RefIsNull => self.exec_ref_is_null()?,
            RefAsNonNull => self.exec_ref_as_non_null()?,
            RefI31 => exec_op!(unary i32 => ValueRef, |v| ValueRef::from_i31(v)),
            I31GetS => self.exec_i31_get(true)?,
            I31GetU => self.exec_i31_get(false)?,
            RefEq => exec_op!(binary ValueRef => i32, |a, b| i32::from(a == b)),
            RefTest(ty) => self.exec_ref_test(*ty)?,
            RefCast(ty) => self.exec_ref_cast(*ty)?,
            // GC objects
            StructNew(ty) => self.exec_struct_new(*ty, false)?,
            StructNewDefault(ty) => self.exec_struct_new(*ty, true)?,
            StructGet(ty, field) => self.exec_struct_get(*ty, *field, None)?,
            StructGetS(ty, field) => self.exec_struct_get(*ty, *field, Some(true))?,
            StructGetU(ty, field) => self.exec_struct_get(*ty, *field, Some(false))?,
            StructSet(ty, field) => self.exec_struct_set(*ty, *field)?,
            ArrayNew(ty) => self.exec_array_new(*ty, false)?,
            ArrayNewDefault(ty) => self.exec_array_new(*ty, true)?,
            ArrayNewFixed(ty, len) => self.exec_array_new_fixed(*ty, *len)?,
            ArrayNewData(ty, data) => self.exec_array_new_data(*ty, *data)?,
            ArrayNewElem(ty, elem) => self.exec_array_new_elem(*ty, *elem)?,
            ArrayGet(ty) => self.exec_array_get(*ty, None)?,
            ArrayGetS(ty) => self.exec_array_get(*ty, Some(true))?,
            ArrayGetU(ty) => self.exec_array_get(*ty, Some(false))?,
            ArraySet(ty) => self.exec_array_set(*ty)?,
            ArrayLen => self.exec_array_len()?,
            ArrayFill(ty) => self.exec_array_fill(*ty)?,
            ArrayCopy(dst, src) => self.exec_array_copy(*dst, *src)?,
            ArrayInitData(ty, data) => self.exec_array_init_data(*ty, *data)?,
            ArrayInitElem(ty, elem) => self.exec_array_init_elem(*ty, *elem)?,
            MemorySize(addr) => self.exec_memory_size(*addr)?,
            MemoryGrow(addr) => self.exec_memory_grow(*addr)?,

            // Bulk memory operations
            MemoryCopy { dst_mem, src_mem } => self.exec_memory_copy(*dst_mem, *src_mem)?,
            MemoryFill(addr) => self.exec_memory_fill(*addr)?,
            MemoryFillImm(addr, val, size) => self.exec_memory_fill_imm(*addr, *val, *size)?,
            MemoryInit(data_idx, mem_idx) => self.exec_memory_init(*data_idx, *mem_idx)?,
            DataDrop(data_index) => self.store.state.get_data_mut(self.module.resolve_data_addr(*data_index)).drop(),
            ElemDrop(elem_index) => self.store.state.get_elem_mut(self.module.resolve_elem_addr(*elem_index)).drop(),

            // Table instructions
            TableGet(table_idx) => self.exec_table_get(*table_idx)?,
            TableSet(table_idx) => self.exec_table_set(*table_idx)?,
            TableSize(table_idx) => self.exec_table_size(*table_idx)?,
            TableInit(elem_idx, table_idx) => self.exec_table_init(*elem_idx, *table_idx)?,
            TableGrow(table_idx) => self.exec_table_grow(*table_idx)?,
            TableFill(table_idx) => self.exec_table_fill(*table_idx)?,
            TableCopy { dst_table, src_table } => self.exec_table_copy(*dst_table, *src_table)?,

            // Core memory load/store operations
            I32Store(m) => self.exec_mem_store::<i32, i32, 4>(m.mem_addr(), m.offset(), |v| v)?,
            I64Store(m) => self.exec_mem_store::<i64, i64, 8>(m.mem_addr(), m.offset(), |v| v)?,
            F32Store(m) => self.exec_mem_store::<f32, f32, 4>(m.mem_addr(), m.offset(), |v| v)?,
            F64Store(m) => self.exec_mem_store::<f64, f64, 8>(m.mem_addr(), m.offset(), |v| v)?,
            FMaStoreF32(m) => self.exec_fma_store::<f32, 4>(*m)?,
            FMaStoreF64(m) => self.exec_fma_store::<f64, 8>(*m)?,
            I32Store8(m) => self.exec_mem_store::<i32, i8, 1>(m.mem_addr(), m.offset(), |v| v as i8)?,
            I32Store16(m) => self.exec_mem_store::<i32, i16, 2>(m.mem_addr(), m.offset(), |v| v as i16)?,
            I64Store8(m) => self.exec_mem_store::<i64, i8, 1>(m.mem_addr(), m.offset(), |v| v as i8)?,
            I64Store16(m) => self.exec_mem_store::<i64, i16, 2>(m.mem_addr(), m.offset(), |v| v as i16)?,
            I64Store32(m) => self.exec_mem_store::<i64, i32, 4>(m.mem_addr(), m.offset(), |v| v as i32)?,
            I32Load(m) => self.exec_mem_load::<i32, 4, _>(m.mem_addr(), m.offset(), |v| v)?,
            I64Load(m) => self.exec_mem_load::<i64, 8, _>(m.mem_addr(), m.offset(), |v| v)?,
            F32Load(m) => self.exec_mem_load::<f32, 4, _>(m.mem_addr(), m.offset(), |v| v)?,
            F64Load(m) => self.exec_mem_load::<f64, 8, _>(m.mem_addr(), m.offset(), |v| v)?,
            I32Load8S(m) => self.exec_mem_load::<i8, 1, _>(m.mem_addr(), m.offset(), i32::from)?,
            I32Load8U(m) => self.exec_mem_load::<u8, 1, _>(m.mem_addr(), m.offset(), i32::from)?,
            I32Load16S(m) => self.exec_mem_load::<i16, 2, _>(m.mem_addr(), m.offset(), i32::from)?,
            I32Load16U(m) => self.exec_mem_load::<u16, 2, _>(m.mem_addr(), m.offset(), i32::from)?,
            I64Load8S(m) => self.exec_mem_load::<i8, 1, _>(m.mem_addr(), m.offset(), i64::from)?,
            I64Load8U(m) => self.exec_mem_load::<u8, 1, _>(m.mem_addr(), m.offset(), i64::from)?,
            I64Load16S(m) => self.exec_mem_load::<i16, 2, _>(m.mem_addr(), m.offset(), i64::from)?,
            I64Load16U(m) => self.exec_mem_load::<u16, 2, _>(m.mem_addr(), m.offset(), i64::from)?,
            I64Load32S(m) => self.exec_mem_load::<i32, 4, _>(m.mem_addr(), m.offset(), i64::from)?,
            I64Load32U(m) => self.exec_mem_load::<u32, 4, _>(m.mem_addr(), m.offset(), i64::from)?,

            // Numeric conversion operations
            F32ConvertI32S => exec_op!(unary i32 => f32, |v| v as f32),
            F32ConvertI64S => exec_op!(unary i64 => f32, |v| v as f32),
            F64ConvertI32S => exec_op!(unary i32 => f64, |v| f64::from(v)),
            F64ConvertI64S => exec_op!(unary i64 => f64, |v| v as f64),
            F32ConvertI32U => exec_op!(unary u32 => f32, |v| v as f32),
            F32ConvertI64U => exec_op!(unary u64 => f32, |v| v as f32),
            F64ConvertI32U => exec_op!(unary u32 => f64, |v| f64::from(v)),
            F64ConvertI64U => exec_op!(unary u64 => f64, |v| v as f64),

            // Sign-extension operations
            I32Extend8S => exec_op!(unary i32 => i32, |v| i32::from(v as i8)),
            I32Extend16S => exec_op!(unary i32 => i32, |v| i32::from(v as i16)),
            I64Extend8S => exec_op!(unary i64 => i64, |v| i64::from(v as i8)),
            I64Extend16S => exec_op!(unary i64 => i64, |v| i64::from(v as i16)),
            I64Extend32S => exec_op!(unary i64 => i64, |v| i64::from(v as i32)),
            I64ExtendI32U => exec_op!(unary u32 => i64, |v| i64::from(v)),
            I64ExtendI32S => exec_op!(unary i32 => i64, |v| i64::from(v)),
            I32WrapI64 => exec_op!(unary i64 => i32, |v| v as i32),
            F32DemoteF64 => exec_op!(unary f64 => f32, |v| v as f32),
            F64PromoteF32 => exec_op!(unary f32 => f64, |v| f64::from(v)),
            F32Abs => exec_op!(unary f32 => f32, |v| v.abs()),
            F64Abs => exec_op!(unary f64 => f64, |v| v.abs()),
            F32Neg => exec_op!(unary f32 => f32, |v| -v),
            F64Neg => exec_op!(unary f64 => f64, |v| -v),
            F32Ceil => exec_op!(unary f32 => f32, |v| v.ceil()),
            F64Ceil => exec_op!(unary f64 => f64, |v| v.ceil()),
            F32Floor => exec_op!(unary f32 => f32, |v| v.floor()),
            F64Floor => exec_op!(unary f64 => f64, |v| v.floor()),
            F32Trunc => exec_op!(unary f32 => f32, |v| v.trunc()),
            F64Trunc => exec_op!(unary f64 => f64, |v| v.trunc()),
            F32Nearest => exec_op!(unary f32 => f32, |v| v.tw_nearest()),
            F64Nearest => exec_op!(unary f64 => f64, |v| v.tw_nearest()),
            F32Sqrt => exec_op!(unary f32 => f32, |v| v.sqrt()),
            F64Sqrt => exec_op!(unary f64 => f64, |v| v.sqrt()),
            F32Min => exec_op!(binary f32 => f32, |a, b| a.tw_minimum(b)),
            F64Min => exec_op!(binary f64 => f64, |a, b| a.tw_minimum(b)),
            F32Max => exec_op!(binary f32 => f32, |a, b| a.tw_maximum(b)),
            F64Max => exec_op!(binary f64 => f64, |a, b| a.tw_maximum(b)),
            F32Copysign => exec_op!(binary f32 => f32, |a, b| a.copysign(b)),
            F64Copysign => exec_op!(binary f64 => f64, |a, b| a.copysign(b)),
            I32TruncF32S => checked_conv_float!(f32, i32, self),
            I32TruncF64S => checked_conv_float!(f64, i32, self),
            I32TruncF32U => checked_conv_float!(f32, u32, i32, self),
            I32TruncF64U => checked_conv_float!(f64, u32, i32, self),
            I64TruncF32S => checked_conv_float!(f32, i64, self),
            I64TruncF64S => checked_conv_float!(f64, i64, self),
            I64TruncF32U => checked_conv_float!(f32, u64, i64, self),
            I64TruncF64U => checked_conv_float!(f64, u64, i64, self),

            // Non-trapping float-to-int conversions
            I32TruncSatF32S => exec_op!(unary f32 => i32, |v| v.trunc() as i32),
            I32TruncSatF32U => exec_op!(unary f32 => u32, |v| v.trunc() as u32),
            I32TruncSatF64S => exec_op!(unary f64 => i32, |v| v.trunc() as i32),
            I32TruncSatF64U => exec_op!(unary f64 => u32, |v| v.trunc() as u32),
            I64TruncSatF32S => exec_op!(unary f32 => i64, |v| v.trunc() as i64),
            I64TruncSatF32U => exec_op!(unary f32 => u64, |v| v.trunc() as u64),
            I64TruncSatF64S => exec_op!(unary f64 => i64, |v| v.trunc() as i64),
            I64TruncSatF64U => exec_op!(unary f64 => u64, |v| v.trunc() as u64),

            // SIMD extension
            V128Not => exec_op!(unary Value128 => Value128, |v| v.v128_not()),
            V128And => exec_op!(binary Value128 => Value128, |a, b| a.v128_and(b)),
            V128AndNot => exec_op!(binary Value128 => Value128, |a, b| a.v128_andnot(b)),
            V128Or => exec_op!(binary Value128 => Value128, |a, b| a.v128_or(b)),
            V128Xor => exec_op!(binary Value128 => Value128, |a, b| a.v128_xor(b)),
            V128Bitselect => exec_op!(ternary Value128 => Value128, |a, b, c| Value128::v128_bitselect(a, b, c)),
            V128AnyTrue => exec_op!(unary Value128 => i32, |v| v.v128_any_true() as i32),
            I8x16Swizzle => exec_op!(binary Value128 => Value128, |a, s| a.i8x16_swizzle(s)),
            I8x16RelaxedSwizzle => exec_op!(binary Value128 => Value128, |a, s| a.i8x16_relaxed_swizzle(s)),
            V128Load(arg) => self.exec_mem_load::<Value128, 16, _>(arg.mem_addr(), arg.offset(), |v| v)?,
            V128Load8x8S(arg) => self.exec_mem_load::<u64, 8, Value128>(arg.mem_addr(), arg.offset(), |v| Value128::v128_load8x8_s(v.to_le_bytes()))?,
            V128Load8x8U(arg) => self.exec_mem_load::<u64, 8, Value128>(arg.mem_addr(), arg.offset(), |v| Value128::v128_load8x8_u(v.to_le_bytes()))?,
            V128Load16x4S(arg) => self.exec_mem_load::<u64, 8, Value128>(arg.mem_addr(), arg.offset(), |v| Value128::v128_load16x4_s(v.to_le_bytes()))?,
            V128Load16x4U(arg) => self.exec_mem_load::<u64, 8, Value128>(arg.mem_addr(), arg.offset(), |v| Value128::v128_load16x4_u(v.to_le_bytes()))?,
            V128Load32x2S(arg) => self.exec_mem_load::<u64, 8, Value128>(arg.mem_addr(), arg.offset(), |v| Value128::v128_load32x2_s(v.to_le_bytes()))?,
            V128Load32x2U(arg) => self.exec_mem_load::<u64, 8, Value128>(arg.mem_addr(), arg.offset(), |v| Value128::v128_load32x2_u(v.to_le_bytes()))?,
            V128Load8Splat(arg) => self.exec_mem_load::<i8, 1, Value128>(arg.mem_addr(), arg.offset(), Value128::splat_i8)?,
            V128Load16Splat(arg) => self.exec_mem_load::<i16, 2, Value128>(arg.mem_addr(), arg.offset(), Value128::splat_i16)?,
            V128Load32Splat(arg) => self.exec_mem_load::<i32, 4, Value128>(arg.mem_addr(), arg.offset(), Value128::splat_i32)?,
            V128Load64Splat(arg) => self.exec_mem_load::<i64, 8, Value128>(arg.mem_addr(), arg.offset(), Value128::splat_i64)?,
            V128Store(arg) => self.exec_mem_store::<Value128, Value128, 16>(arg.mem_addr(), arg.offset(), |v| v)?,
            V128Store8Lane(arg, lane) => self.exec_mem_store_lane::<i8, 1>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Store16Lane(arg, lane) => self.exec_mem_store_lane::<i16, 2>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Store32Lane(arg, lane) => self.exec_mem_store_lane::<i32, 4>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Store64Lane(arg, lane) => self.exec_mem_store_lane::<i64, 8>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load32Zero(arg) => self.exec_mem_load::<i32, 4, Value128>(arg.mem_addr(), arg.offset(), |v| Value128::from_i32x4([v, 0, 0, 0]))?,
            V128Load64Zero(arg) => self.exec_mem_load::<i64, 8, Value128>(arg.mem_addr(), arg.offset(), |v| Value128::from_i64x2([v, 0]))?,
            Const128(arg) => Value128::stack_push(&mut self.store.value_stack, Value128(self.func.data.v128_const(*arg)))?,
            I8x16ExtractLaneS(lane) => exec_op!(unary Value128 => i32, |v| v.extract_lane_i8(*lane) as i32),
            I8x16ExtractLaneU(lane) => exec_op!(unary Value128 => i32, |v| v.extract_lane_u8(*lane) as i32),
            I16x8ExtractLaneS(lane) => exec_op!(unary Value128 => i32, |v| v.extract_lane_i16(*lane) as i32),
            I16x8ExtractLaneU(lane) => exec_op!(unary Value128 => i32, |v| v.extract_lane_u16(*lane) as i32),
            I32x4ExtractLane(lane) => exec_op!(unary Value128 => i32, |v| v.extract_lane_i32(*lane)),
            I64x2ExtractLane(lane) => exec_op!(unary Value128 => i64, |v| v.extract_lane_i64(*lane)),
            F32x4ExtractLane(lane) => exec_op!(unary Value128 => f32, |v| v.extract_lane_f32(*lane)),
            F64x2ExtractLane(lane) => exec_op!(unary Value128 => f64, |v| v.extract_lane_f64(*lane)),
            V128Load8Lane(arg, lane) => self.exec_mem_load_lane::<i8, 1>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load16Lane(arg, lane) => self.exec_mem_load_lane::<i16, 2>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load32Lane(arg, lane) => self.exec_mem_load_lane::<i32, 4>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load64Lane(arg, lane) => self.exec_mem_load_lane::<i64, 8>(arg.mem_addr(), arg.offset(), *lane)?,
            I8x16ReplaceLane(lane) => exec_op!(binary_mixed i32, Value128 => Value128, |value, vec| vec.i8x16_replace_lane(*lane, value as i8)),
            I16x8ReplaceLane(lane) => exec_op!(binary_mixed i32, Value128 => Value128, |value, vec| vec.i16x8_replace_lane(*lane, value as i16)),
            I32x4ReplaceLane(lane) => exec_op!(binary_mixed i32, Value128 => Value128, |value, vec| vec.i32x4_replace_lane(*lane, value)),
            I64x2ReplaceLane(lane) => exec_op!(binary_mixed i64, Value128 => Value128, |value, vec| vec.i64x2_replace_lane(*lane, value)),
            F32x4ReplaceLane(lane) => exec_op!(binary_mixed f32, Value128 => Value128, |value, vec| vec.f32x4_replace_lane(*lane, value)),
            F64x2ReplaceLane(lane) => exec_op!(binary_mixed f64, Value128 => Value128, |value, vec| vec.f64x2_replace_lane(*lane, value)),
            I8x16Splat => exec_op!(unary i32 => Value128, |v| Value128::splat_i8(v as i8)),
            I16x8Splat => exec_op!(unary i32 => Value128, |v| Value128::splat_i16(v as i16)),
            I32x4Splat => exec_op!(unary i32 => Value128, |v| Value128::splat_i32(v)),
            I64x2Splat => exec_op!(unary i64 => Value128, |v| Value128::splat_i64(v)),
            F32x4Splat => exec_op!(unary f32 => Value128, |v| Value128::splat_f32(v)),
            F64x2Splat => exec_op!(unary f64 => Value128, |v| Value128::splat_f64(v)),
            I8x16Eq => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_eq(b)),
            I16x8Eq => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_eq(b)),
            I32x4Eq => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_eq(b)),
            I64x2Eq => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_eq(b)),
            F32x4Eq => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_eq(b)),
            F64x2Eq => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_eq(b)),
            I8x16Ne => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_ne(b)),
            I16x8Ne => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_ne(b)),
            I32x4Ne => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_ne(b)),
            I64x2Ne => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_ne(b)),
            F32x4Ne => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_ne(b)),
            F64x2Ne => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_ne(b)),
            I8x16LtS => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_lt_s(b)),
            I16x8LtS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_lt_s(b)),
            I32x4LtS => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_lt_s(b)),
            I64x2LtS => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_lt_s(b)),
            I8x16LtU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_lt_u(b)),
            I16x8LtU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_lt_u(b)),
            I32x4LtU => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_lt_u(b)),
            F32x4Lt => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_lt(b)),
            F64x2Lt => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_lt(b)),
            F32x4Gt => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_gt(b)),
            F64x2Gt => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_gt(b)),
            I8x16GtS => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_gt_s(b)),
            I16x8GtS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_gt_s(b)),
            I32x4GtS => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_gt_s(b)),
            I64x2GtS => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_gt_s(b)),
            I64x2LeS => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_le_s(b)),
            F32x4Le => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_le(b)),
            F64x2Le => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_le(b)),
            I8x16GtU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_gt_u(b)),
            I16x8GtU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_gt_u(b)),
            I32x4GtU => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_gt_u(b)),
            F32x4Ge => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_ge(b)),
            F64x2Ge => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_ge(b)),
            I8x16LeS => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_le_s(b)),
            I16x8LeS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_le_s(b)),
            I32x4LeS => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_le_s(b)),
            I8x16LeU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_le_u(b)),
            I16x8LeU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_le_u(b)),
            I32x4LeU => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_le_u(b)),
            I8x16GeS => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_ge_s(b)),
            I16x8GeS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_ge_s(b)),
            I32x4GeS => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_ge_s(b)),
            I64x2GeS => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_ge_s(b)),
            I8x16GeU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_ge_u(b)),
            I16x8GeU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_ge_u(b)),
            I32x4GeU => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_ge_u(b)),
            I8x16Abs => exec_op!(unary Value128 => Value128, |a| a.i8x16_abs()),
            I16x8Abs => exec_op!(unary Value128 => Value128, |a| a.i16x8_abs()),
            I32x4Abs => exec_op!(unary Value128 => Value128, |a| a.i32x4_abs()),
            I64x2Abs => exec_op!(unary Value128 => Value128, |a| a.i64x2_abs()),
            I8x16Neg => exec_op!(unary Value128 => Value128, |a| a.i8x16_neg()),
            I16x8Neg => exec_op!(unary Value128 => Value128, |a| a.i16x8_neg()),
            I32x4Neg => exec_op!(unary Value128 => Value128, |a| a.i32x4_neg()),
            I64x2Neg => exec_op!(unary Value128 => Value128, |a| a.i64x2_neg()),
            I8x16AllTrue => exec_op!(unary Value128 => i32, |v| v.i8x16_all_true() as i32),
            I16x8AllTrue => exec_op!(unary Value128 => i32, |v| v.i16x8_all_true() as i32),
            I32x4AllTrue => exec_op!(unary Value128 => i32, |v| v.i32x4_all_true() as i32),
            I64x2AllTrue => exec_op!(unary Value128 => i32, |v| v.i64x2_all_true() as i32),
            I8x16Bitmask => exec_op!(unary Value128 => i32, |v| v.i8x16_bitmask() as i32),
            I16x8Bitmask => exec_op!(unary Value128 => i32, |v| v.i16x8_bitmask() as i32),
            I32x4Bitmask => exec_op!(unary Value128 => i32, |v| v.i32x4_bitmask() as i32),
            I64x2Bitmask => exec_op!(unary Value128 => i32, |v| v.i64x2_bitmask() as i32),
            I8x16Shl => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i8x16_shl(a as u32)),
            I16x8Shl => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i16x8_shl(a as u32)),
            I32x4Shl => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i32x4_shl(a as u32)),
            I64x2Shl => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i64x2_shl(a as u32)),
            I8x16ShrS => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i8x16_shr_s(a as u32)),
            I16x8ShrS => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i16x8_shr_s(a as u32)),
            I32x4ShrS => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i32x4_shr_s(a as u32)),
            I64x2ShrS => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i64x2_shr_s(a as u32)),
            I8x16ShrU => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i8x16_shr_u(a as u32)),
            I16x8ShrU => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i16x8_shr_u(a as u32)),
            I32x4ShrU => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i32x4_shr_u(a as u32)),
            I64x2ShrU => exec_op!(binary_mixed i32, Value128 => Value128, |a, b| b.i64x2_shr_u(a as u32)),
            I8x16Add => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_add(b)),
            I16x8Add => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_add(b)),
            I32x4Add => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_add(b)),
            I64x2Add => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_add(b)),
            I8x16Sub => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_sub(b)),
            I16x8Sub => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_sub(b)),
            I32x4Sub => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_sub(b)),
            I64x2Sub => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_sub(b)),
            I8x16MinS => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_min_s(b)),
            I16x8MinS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_min_s(b)),
            I32x4MinS => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_min_s(b)),
            I8x16MinU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_min_u(b)),
            I16x8MinU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_min_u(b)),
            I32x4MinU => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_min_u(b)),
            I8x16MaxS => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_max_s(b)),
            I16x8MaxS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_max_s(b)),
            I32x4MaxS => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_max_s(b)),
            I8x16MaxU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_max_u(b)),
            I16x8MaxU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_max_u(b)),
            I32x4MaxU => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_max_u(b)),
            I64x2Mul => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_mul(b)),
            I16x8Mul => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_mul(b)),
            I32x4Mul => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_mul(b)),
            I8x16NarrowI16x8S => exec_op!(binary Value128 => Value128, |a, b| Value128::i8x16_narrow_i16x8_s(a, b)),
            I8x16NarrowI16x8U => exec_op!(binary Value128 => Value128, |a, b| Value128::i8x16_narrow_i16x8_u(a, b)),
            I16x8NarrowI32x4S => exec_op!(binary Value128 => Value128, |a, b| Value128::i16x8_narrow_i32x4_s(a, b)),
            I16x8NarrowI32x4U => exec_op!(binary Value128 => Value128, |a, b| Value128::i16x8_narrow_i32x4_u(a, b)),
            I8x16AddSatS => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_add_sat_s(b)),
            I16x8AddSatS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_add_sat_s(b)),
            I8x16AddSatU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_add_sat_u(b)),
            I16x8AddSatU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_add_sat_u(b)),
            I8x16SubSatS => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_sub_sat_s(b)),
            I16x8SubSatS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_sub_sat_s(b)),
            I8x16SubSatU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_sub_sat_u(b)),
            I16x8SubSatU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_sub_sat_u(b)),
            I8x16AvgrU => exec_op!(binary Value128 => Value128, |a, b| a.i8x16_avgr_u(b)),
            I16x8AvgrU => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_avgr_u(b)),
            I16x8ExtAddPairwiseI8x16S => exec_op!(unary Value128 => Value128, |a| a.i16x8_extadd_pairwise_i8x16_s()),
            I16x8ExtAddPairwiseI8x16U => exec_op!(unary Value128 => Value128, |a| a.i16x8_extadd_pairwise_i8x16_u()),
            I32x4ExtAddPairwiseI16x8S => exec_op!(unary Value128 => Value128, |a| a.i32x4_extadd_pairwise_i16x8_s()),
            I32x4ExtAddPairwiseI16x8U => exec_op!(unary Value128 => Value128, |a| a.i32x4_extadd_pairwise_i16x8_u()),
            I16x8ExtMulLowI8x16S => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_extmul_low_i8x16_s(b)),
            I16x8ExtMulLowI8x16U => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_extmul_low_i8x16_u(b)),
            I16x8ExtMulHighI8x16S => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_extmul_high_i8x16_s(b)),
            I16x8ExtMulHighI8x16U => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_extmul_high_i8x16_u(b)),
            I32x4ExtMulLowI16x8S => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_extmul_low_i16x8_s(b)),
            I32x4ExtMulLowI16x8U => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_extmul_low_i16x8_u(b)),
            I32x4ExtMulHighI16x8S => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_extmul_high_i16x8_s(b)),
            I32x4ExtMulHighI16x8U => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_extmul_high_i16x8_u(b)),
            I64x2ExtMulLowI32x4S => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_extmul_low_i32x4_s(b)),
            I64x2ExtMulLowI32x4U => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_extmul_low_i32x4_u(b)),
            I64x2ExtMulHighI32x4S => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_extmul_high_i32x4_s(b)),
            I64x2ExtMulHighI32x4U => exec_op!(binary Value128 => Value128, |a, b| a.i64x2_extmul_high_i32x4_u(b)),
            I16x8ExtendLowI8x16S => exec_op!(unary Value128 => Value128, |a| a.i16x8_extend_low_i8x16_s()),
            I16x8ExtendLowI8x16U => exec_op!(unary Value128 => Value128, |a| a.i16x8_extend_low_i8x16_u()),
            I16x8ExtendHighI8x16S => exec_op!(unary Value128 => Value128, |a| a.i16x8_extend_high_i8x16_s()),
            I16x8ExtendHighI8x16U => exec_op!(unary Value128 => Value128, |a| a.i16x8_extend_high_i8x16_u()),
            I32x4ExtendLowI16x8S => exec_op!(unary Value128 => Value128, |a| a.i32x4_extend_low_i16x8_s()),
            I32x4ExtendLowI16x8U => exec_op!(unary Value128 => Value128, |a| a.i32x4_extend_low_i16x8_u()),
            I32x4ExtendHighI16x8S => exec_op!(unary Value128 => Value128, |a| a.i32x4_extend_high_i16x8_s()),
            I32x4ExtendHighI16x8U => exec_op!(unary Value128 => Value128, |a| a.i32x4_extend_high_i16x8_u()),
            I64x2ExtendLowI32x4S => exec_op!(unary Value128 => Value128, |a| a.i64x2_extend_low_i32x4_s()),
            I64x2ExtendLowI32x4U => exec_op!(unary Value128 => Value128, |a| a.i64x2_extend_low_i32x4_u()),
            I64x2ExtendHighI32x4S => exec_op!(unary Value128 => Value128, |a| a.i64x2_extend_high_i32x4_s()),
            I64x2ExtendHighI32x4U => exec_op!(unary Value128 => Value128, |a| a.i64x2_extend_high_i32x4_u()),
            I8x16Popcnt => exec_op!(unary Value128 => Value128, |v| v.i8x16_popcnt()),
            I8x16Shuffle(idx) => exec_op!(binary Value128 => Value128, |a, b| Value128::i8x16_shuffle(a, b, Value128(self.func.data.v128_const(*idx)))),
            I16x8Q15MulrSatS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_q15mulr_sat_s(b)),
            I32x4DotI16x8S => exec_op!(binary Value128 => Value128, |a, b| a.i32x4_dot_i16x8_s(b)),
            I8x16RelaxedLaneselect => exec_op!(ternary Value128 => Value128, |a, b, c| Value128::i8x16_relaxed_laneselect(a, b, c)),
            I16x8RelaxedLaneselect => exec_op!(ternary Value128 => Value128, |a, b, c| Value128::i16x8_relaxed_laneselect(a, b, c)),
            I32x4RelaxedLaneselect => exec_op!(ternary Value128 => Value128, |a, b, c| Value128::i32x4_relaxed_laneselect(a, b, c)),
            I64x2RelaxedLaneselect => exec_op!(ternary Value128 => Value128, |a, b, c| Value128::i64x2_relaxed_laneselect(a, b, c)),
            I16x8RelaxedQ15mulrS => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_relaxed_q15mulr_s(b)),
            I16x8RelaxedDotI8x16I7x16S => exec_op!(binary Value128 => Value128, |a, b| a.i16x8_relaxed_dot_i8x16_i7x16_s(b)),
            I32x4RelaxedDotI8x16I7x16AddS => exec_op!(ternary Value128 => Value128, |a, b, c| a.i32x4_relaxed_dot_i8x16_i7x16_add_s(b, c)),
            F32x4Ceil => exec_op!(unary Value128 => Value128, |v| v.f32x4_ceil()),
            F64x2Ceil => exec_op!(unary Value128 => Value128, |v| v.f64x2_ceil()),
            F32x4Floor => exec_op!(unary Value128 => Value128, |v| v.f32x4_floor()),
            F64x2Floor => exec_op!(unary Value128 => Value128, |v| v.f64x2_floor()),
            F32x4Trunc => exec_op!(unary Value128 => Value128, |v| v.f32x4_trunc()),
            F64x2Trunc => exec_op!(unary Value128 => Value128, |v| v.f64x2_trunc()),
            F32x4Nearest => exec_op!(unary Value128 => Value128, |v| v.f32x4_nearest()),
            F64x2Nearest => exec_op!(unary Value128 => Value128, |v| v.f64x2_nearest()),
            F32x4Abs => exec_op!(unary Value128 => Value128, |v| v.f32x4_abs()),
            F64x2Abs => exec_op!(unary Value128 => Value128, |v| v.f64x2_abs()),
            F32x4Neg => exec_op!(unary Value128 => Value128, |v| v.f32x4_neg()),
            F64x2Neg => exec_op!(unary Value128 => Value128, |v| v.f64x2_neg()),
            F32x4Sqrt => exec_op!(unary Value128 => Value128, |v| v.f32x4_sqrt()),
            F64x2Sqrt => exec_op!(unary Value128 => Value128, |v| v.f64x2_sqrt()),
            F32x4Add => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_add(b)),
            F64x2Add => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_add(b)),
            F32x4Sub => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_sub(b)),
            F64x2Sub => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_sub(b)),
            F32x4Mul => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_mul(b)),
            F64x2Mul => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_mul(b)),
            F32x4Div => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_div(b)),
            F64x2Div => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_div(b)),
            F32x4Min => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_min(b)),
            F64x2Min => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_min(b)),
            F32x4Max => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_max(b)),
            F64x2Max => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_max(b)),
            F32x4PMin => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_pmin(b)),
            F32x4PMax => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_pmax(b)),
            F64x2PMin => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_pmin(b)),
            F64x2PMax => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_pmax(b)),
            F32x4RelaxedMadd => exec_op!(ternary Value128 => Value128, |a, b, c| a.f32x4_relaxed_madd(b, c)),
            F32x4RelaxedNmadd => exec_op!(ternary Value128 => Value128, |a, b, c| a.f32x4_relaxed_nmadd(b, c)),
            F64x2RelaxedMadd => exec_op!(ternary Value128 => Value128, |a, b, c| a.f64x2_relaxed_madd(b, c)),
            F64x2RelaxedNmadd => exec_op!(ternary Value128 => Value128, |a, b, c| a.f64x2_relaxed_nmadd(b, c)),
            F32x4RelaxedMin => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_relaxed_min(b)),
            F32x4RelaxedMax => exec_op!(binary Value128 => Value128, |a, b| a.f32x4_relaxed_max(b)),
            F64x2RelaxedMin => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_relaxed_min(b)),
            F64x2RelaxedMax => exec_op!(binary Value128 => Value128, |a, b| a.f64x2_relaxed_max(b)),
            I32x4TruncSatF32x4S => exec_op!(unary Value128 => Value128, |v| v.i32x4_trunc_sat_f32x4_s()),
            I32x4TruncSatF32x4U => exec_op!(unary Value128 => Value128, |v| v.i32x4_trunc_sat_f32x4_u()),
            F32x4ConvertI32x4S => exec_op!(unary Value128 => Value128, |v| v.f32x4_convert_i32x4_s()),
            F32x4ConvertI32x4U => exec_op!(unary Value128 => Value128, |v| v.f32x4_convert_i32x4_u()),
            F64x2ConvertLowI32x4S => exec_op!(unary Value128 => Value128, |v| v.f64x2_convert_low_i32x4_s()),
            F64x2ConvertLowI32x4U => exec_op!(unary Value128 => Value128, |v| v.f64x2_convert_low_i32x4_u()),
            F32x4DemoteF64x2Zero => exec_op!(unary Value128 => Value128, |v| v.f32x4_demote_f64x2_zero()),
            F64x2PromoteLowF32x4 => exec_op!(unary Value128 => Value128, |v| v.f64x2_promote_low_f32x4()),
            I32x4TruncSatF64x2SZero => exec_op!(unary Value128 => Value128, |v| v.i32x4_trunc_sat_f64x2_s_zero()),
            I32x4TruncSatF64x2UZero => exec_op!(unary Value128 => Value128, |v| v.i32x4_trunc_sat_f64x2_u_zero()),
            I32x4RelaxedTruncF32x4S => exec_op!(unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f32x4_s()),
            I32x4RelaxedTruncF32x4U => exec_op!(unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f32x4_u()),
            I32x4RelaxedTruncF64x2SZero => exec_op!(unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f64x2_s_zero()),
            I32x4RelaxedTruncF64x2UZero => exec_op!(unary Value128 => Value128, |v| v.i32x4_relaxed_trunc_f64x2_u_zero()),
        };

        self.cf.instr_ptr = instr_ptr + 1;

        Ok(None)
    }

    #[inline(always)]
    fn exec_jump_if(cf: &mut CallFrame, target_ip: u32, condition: impl FnOnce(&CallFrame) -> bool) -> bool {
        let condition = condition(cf);
        if condition {
            cf.instr_ptr = target_ip as usize;
        }
        condition
    }

    fn exec_branch_table(&mut self, default_ip: u32, start: u32, len: u32) {
        let idx = <i32>::stack_pop(&mut self.store.value_stack);
        let target_ip = if idx >= 0 && (idx as u32) < len {
            self.func.data.branch_table_targets.get((start + idx as u32) as usize).copied().unwrap_or(default_ip)
        } else {
            default_ip
        };

        self.cf.instr_ptr = target_ip as usize;
    }

    fn exec_drop_keep(&mut self, drop_keep: DropKeep) {
        let mut base = self.cf.stack_base();
        base.s32 += drop_keep.base.c32 as u32;
        base.s64 += drop_keep.base.c64 as u32;
        base.s128 += drop_keep.base.c128 as u32;
        self.store.value_stack.truncate_keep_counts(base, drop_keep.keep);
    }

    fn create_exception(&mut self, tag_index: TagAddr) -> Result<ExnAddr, Trap> {
        let tag_addr = self.module.resolve_tag_addr(tag_index);
        let type_addr = self.store.state.get_tag(tag_addr).type_addr;
        let addr = cold_err!(u32::try_from(self.store.state.exceptions.len())).map_err(|_| Trap::OutOfMemory)?;
        cold_err!(self.store.state.exceptions.try_reserve(1)).map_err(|_| Trap::OutOfMemory)?;
        let params = self.store.state.get_canonical_func_type(type_addr).params();
        let mut payload = Vec::new();
        cold_err!(payload.try_reserve_exact(params.len())).map_err(|_| Trap::OutOfMemory)?;
        let value_stack = &mut self.store.value_stack;
        for &ty in params.iter().rev() {
            payload.push(match ty {
                WasmType::I32 | WasmType::F32 => TinyWasmValue::Value32(Value32::stack_pop(value_stack)),
                WasmType::I64 | WasmType::F64 => TinyWasmValue::Value64(Value64::stack_pop(value_stack)),
                WasmType::V128 => TinyWasmValue::Value128(Value128::stack_pop(value_stack)),
                WasmType::Ref(_) => TinyWasmValue::ValueRef(ValueRef::stack_pop(value_stack)),
            });
        }
        payload.reverse();
        let exception = crate::store::ExceptionInstance { tag_addr, payload: payload.into_boxed_slice() };
        self.store.state.exceptions.push(exception);
        Ok(addr)
    }

    fn exec_throw(&mut self, tag_index: TagAddr) -> Result<()> {
        let exception = self.create_exception(tag_index)?;
        let outcome = match self.dispatch_exception(exception) {
            Ok(outcome) => outcome,
            Err(trap) => {
                _ = self.store.state.exceptions.pop();
                return Err(trap.into());
            }
        };
        match outcome {
            Some(catch) if !catch.with_ref() => {
                debug_assert_eq!(self.store.state.exceptions.len() - 1, exception as usize);
                _ = self.store.state.exceptions.pop();
                Ok(())
            }
            Some(_) => Ok(()),
            None => Err(Error::Exception(ExnRef::new(exception))),
        }
    }

    fn exec_throw_ref(&mut self) -> Result<()> {
        let exception = ValueRef::stack_pop(&mut self.store.value_stack);
        let exception = exception
            .addr()
            .filter(|addr| self.store.state.exceptions.get(*addr as usize).is_some())
            .ok_or(Trap::NullReference)?;
        match self.dispatch_exception(exception)? {
            Some(_) => Ok(()),
            None => Err(Error::Exception(ExnRef::new(exception))),
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

    fn switch_to_frame(&mut self, frame: CallFrame) {
        let previous = core::mem::replace(&mut self.cf, frame);
        if previous.func_addr == self.cf.func_addr {
            return;
        }

        let wasm_func = self.store.state.get_wasm_func(self.cf.func_addr);
        self.func = wasm_func.func.clone();
        if wasm_func.owner != self.module.id() {
            self.module = self
                .store
                .get_module_instance(wasm_func.owner)
                .unwrap_or_else(|| unreachable!("invalid module instance"))
                .clone();
        }
    }

    fn dispatch_exception(&mut self, exception_addr: ExnAddr) -> Result<Option<ExceptionCatch>, Trap> {
        let tag_addr = self.store.state.exceptions[exception_addr as usize].tag_addr;
        let mut protected_ip = self.cf.instr_ptr;
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
                    for value in state.exceptions[exception_addr as usize].payload.iter().copied() {
                        value_stack.push_dyn(value)?;
                    }
                }
                if with_ref {
                    self.store.value_stack.push(ValueRef::from_category_addr(exception_addr))?;
                }
                self.cf.instr_ptr = landing_pad as usize;
                return Ok(Some(catch));
            }

            self.store.value_stack.truncate_to_base(self.cf.locals_base);
            let Some(caller) = self.store.call_stack.pop_frame(self.call_stack_base) else {
                return Ok(None);
            };
            self.switch_to_frame(caller);
            protected_ip = self.cf.instr_ptr.checked_sub(1).unwrap_or_else(|| unreachable!("invalid caller IP"));
        }
    }

    fn exec_call(&mut self, wasm_func: WasmFunctionInstance, func_addr: FuncAddr) -> Result<(), Trap> {
        if !Arc::ptr_eq(&self.func, &wasm_func.func) {
            self.func = wasm_func.func.clone();
        }

        let Ok(locals_base) = self.store.value_stack.enter_locals(&wasm_func.func.params, &wasm_func.func.locals)
        else {
            return cold!(Err(Trap::CallStackOverflow));
        };

        self.store.call_stack.push(self.cf)?;
        self.cf = CallFrame::new(func_addr, locals_base, wasm_func.func.locals);
        if wasm_func.owner != self.module.id() {
            self.module = self
                .store
                .get_module_instance(wasm_func.owner)
                .unwrap_or_else(|| unreachable!("invalid module instance"))
                .clone();
        }

        Ok(())
    }

    fn exec_return_call(&mut self, wasm_func: WasmFunctionInstance, func_addr: FuncAddr) -> Result<(), Trap> {
        if !Arc::ptr_eq(&self.func, &wasm_func.func) {
            self.func = wasm_func.func.clone();
        }

        self.store.value_stack.truncate_keep_counts(self.cf.locals_base, wasm_func.func.params);
        let Ok(locals_base) = self.store.value_stack.enter_locals(&wasm_func.func.params, &wasm_func.func.locals)
        else {
            return cold!(Err(Trap::CallStackOverflow));
        };
        self.cf = CallFrame::new(func_addr, locals_base, wasm_func.func.locals);
        if wasm_func.owner != self.module.id() {
            self.module = self
                .store
                .get_module_instance(wasm_func.owner)
                .unwrap_or_else(|| unreachable!("invalid module instance"))
                .clone();
        }

        Ok(())
    }

    fn exec_call_host<const TAIL: bool>(
        &mut self,
        host_func: HostFunction,
        type_addr: TypeAddr,
        params_may_gc: bool,
    ) -> Result<bool, Trap> {
        if let Some(host_func) = host_func.typed_callback() {
            cold_err!(host_func.call_stack(self.store, self.module.id(), type_addr))
                .map_err(|error| Trap::HostFunction(Box::new(error)))?;
            if TAIL {
                return Ok(self.exec_return());
            }
            self.cf.instr_ptr += 1;
            return Ok(false);
        }

        let param_types = self.store.state.get_canonical_func_type(type_addr).params();
        let mut params = core::mem::take(&mut self.store.host_params);
        debug_assert!(params.is_empty());
        cold_err!(params.try_reserve_exact(param_types.len())).map_err(|_| Trap::OutOfMemory)?;
        for &ty in param_types.iter().rev() {
            params.push(self.store.value_stack.pop_wasmvalue(&self.store.state, ty));
        }
        params.reverse();
        if params_may_gc {
            self.store.state.pin_host_values(&params);
        }
        let result = host_func.call_values(self.store, self.module.id(), type_addr, &params);
        params.clear();
        self.store.host_params = params;
        let res = cold_err!(result).map_err(|error| Trap::HostFunction(Box::new(error)))?;

        self.store.value_stack.extend_wasmvalues(res.iter().copied())?;
        if TAIL {
            Ok(self.exec_return())
        } else {
            self.cf.instr_ptr += 1;
            Ok(false)
        }
    }

    fn exec_call_direct(&mut self, v: u32) -> Result<(), Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        let func = self.store.state.get_func(addr).clone();
        match func.kind {
            crate::store::FunctionKind::Wasm(wasm_func) => self.exec_call(wasm_func, addr),
            crate::store::FunctionKind::Host(host_func) => {
                self.exec_call_host::<false>(host_func, func.type_addr, func.gc.params)?;
                Ok(())
            }
        }
    }

    fn exec_return_call_direct(&mut self, v: u32) -> Result<bool, Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        let func = self.store.state.get_func(addr).clone();
        match func.kind {
            crate::store::FunctionKind::Wasm(wasm_func) => {
                self.exec_return_call(wasm_func, addr)?;
                Ok(false)
            }
            crate::store::FunctionKind::Host(host_func) => {
                self.exec_call_host::<true>(host_func, func.type_addr, func.gc.params)
            }
        }
    }

    fn exec_call_self(&mut self) -> Result<(), Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let Ok(locals_base) = self.store.value_stack.enter_locals(&self.func.params, &self.func.locals) else {
            return cold!(Err(Trap::CallStackOverflow));
        };
        let new = CallFrame::new(self.cf.func_addr, locals_base, self.func.locals);
        self.store.call_stack.push(core::mem::replace(&mut self.cf, new))?;

        Ok(())
    }

    fn exec_return_call_self(&mut self) -> Result<(), Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);

        self.store.value_stack.truncate_keep_counts(self.cf.locals_base, self.func.params);
        let Ok(locals_base) = self.store.value_stack.enter_locals(&self.func.params, &self.func.locals) else {
            return cold!(Err(Trap::CallStackOverflow));
        };
        self.cf = CallFrame::new(self.cf.func_addr, locals_base, self.func.locals);
        Ok(())
    }

    fn exec_call_indirect<const IS_RETURN_CALL: bool>(
        &mut self,
        type_addr: u32,
        table_addr: u32,
    ) -> Result<bool, Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);

        // verify that the table is of the right type, this should be validated by the parser already
        let table_addr = self.module.resolve_table_addr(table_addr);
        let table_idx = self.pop_table_operand(self.store.state.get_table(table_addr).kind.arch())?;
        let table = self.store.state.get_table(table_addr);

        let Ok(table) = table.get(table_idx) else {
            return cold!(Err(Trap::UndefinedElement { index: table_idx }));
        };

        let Some(func_ref) = table.addr() else {
            return cold!(Err(Trap::UninitializedElement { index: table_idx }));
        };

        self.exec_typed_call::<IS_RETURN_CALL>(func_ref, self.module.resolve_type_addr(type_addr))
    }

    fn exec_typed_call<const IS_RETURN_CALL: bool>(
        &mut self,
        func_addr: FuncAddr,
        expected_type_addr: TypeAddr,
    ) -> Result<bool, Trap> {
        let func = self.store.state.get_func(func_addr).clone();
        if !self.store.state.type_addr_is_subtype(func.type_addr, expected_type_addr) {
            return cold!(Err(Trap::IndirectCallTypeMismatch {
                actual: Box::new(self.store.state.get_canonical_func_type(func.type_addr).clone()),
                expected: Box::new(self.store.state.get_canonical_func_type(expected_type_addr).clone()),
            }));
        }
        match func.kind {
            crate::store::FunctionKind::Wasm(wasm_func) => match IS_RETURN_CALL {
                true => self.exec_return_call(wasm_func, func_addr),
                false => self.exec_call(wasm_func, func_addr),
            },
            crate::store::FunctionKind::Host(host_func) => {
                return self.exec_call_host::<IS_RETURN_CALL>(host_func, func.type_addr, func.gc.params);
            }
        }?;
        Ok(false)
    }

    fn exec_call_ref<const IS_RETURN_CALL: bool>(&mut self, type_addr: u32) -> Result<bool, Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let func_ref = ValueRef::stack_pop(&mut self.store.value_stack);
        let Some(func_addr) = func_ref.addr() else {
            return cold!(Err(Trap::NullFunctionReference));
        };

        self.exec_typed_call::<IS_RETURN_CALL>(func_addr, self.module.resolve_type_addr(type_addr))
    }

    fn exec_return(&mut self) -> bool {
        self.store.value_stack.truncate_keep_counts(self.cf.locals_base, self.func.results);
        self.finish_return()
    }

    #[inline(always)]
    fn finish_return(&mut self) -> bool {
        let Some(caller) = self.store.call_stack.pop_frame(self.call_stack_base) else {
            return true;
        };
        if caller.func_addr == self.cf.func_addr {
            self.cf = caller;
        } else {
            self.switch_to_frame(caller);
        }
        false
    }

    #[inline(always)]
    fn exec_return_void(&mut self) -> bool {
        self.store.value_stack.truncate_to_base(self.cf.locals_base);
        self.finish_return()
    }

    #[inline(always)]
    fn exec_return_32(&mut self) -> bool {
        self.store.value_stack.stack_32.truncate_to_one_tail(self.cf.locals_base.s32 as usize);
        self.store.value_stack.stack_64.truncate_to(self.cf.locals_base.s64 as usize);
        self.store.value_stack.stack_128.truncate_to(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    #[inline(always)]
    fn exec_return_64(&mut self) -> bool {
        self.store.value_stack.stack_32.truncate_to(self.cf.locals_base.s32 as usize);
        self.store.value_stack.stack_64.truncate_to_one_tail(self.cf.locals_base.s64 as usize);
        self.store.value_stack.stack_128.truncate_to(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    #[inline(always)]
    fn exec_return_128(&mut self) -> bool {
        self.store.value_stack.stack_32.truncate_to(self.cf.locals_base.s32 as usize);
        self.store.value_stack.stack_64.truncate_to(self.cf.locals_base.s64 as usize);
        self.store.value_stack.stack_128.truncate_to_one_tail(self.cf.locals_base.s128 as usize);
        self.finish_return()
    }

    #[inline(always)]
    fn exec_store_local_local<T: InternalValue + MemValue<N>, const N: usize>(
        &mut self,
        memarg: MemoryArg,
        addr_local: u8,
        value_local: u8,
    ) -> Result<(), Trap> {
        let value = T::local_get(&self.store.value_stack, &self.cf, u16::from(value_local));
        let mem_addr = self.module.resolve_mem_addr(memarg.mem_addr());
        let mem = self.store.state.get_mem(mem_addr);
        let addr = if mem.is_64bit() {
            let base = u64::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local));
            let base = cold_err!(usize::try_from(base).map_err(|_| Trap::MemoryOutOfBounds {
                offset: usize::MAX,
                len: N,
                max: mem.inner.len(),
            }))?;
            mem.effective_addr::<N>(base, memarg.offset())?
        } else {
            let base = u32::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local));
            mem.effective_addr::<N>(base as usize, memarg.offset())?
        };
        let mem = self.store.state.get_mem_mut(mem_addr);
        value.store_at(&mut *mem.inner, addr)
    }

    #[inline(always)]
    fn exec_inc_memory_local<T: MemValue<N>, const N: usize>(
        &mut self,
        memarg: MemoryArg,
        addr_local: u8,
        increment: impl FnOnce(T) -> T,
    ) -> Result<(), Trap> {
        let mem_addr = self.module.resolve_mem_addr(memarg.mem_addr());
        let mem = self.store.state.get_mem(mem_addr);
        let addr = if mem.is_64bit() {
            let base = i64::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local)) as u64;
            let base = cold_err!(usize::try_from(base).map_err(|_| Trap::MemoryOutOfBounds {
                offset: usize::MAX,
                len: N,
                max: mem.inner.len(),
            }))?;
            mem.effective_addr::<N>(base, memarg.offset())?
        } else {
            let base = u32::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local));
            mem.effective_addr::<N>(base as usize, memarg.offset())?
        };

        let mem = self.store.state.get_mem_mut(mem_addr);
        let value = T::load_at(&*mem.inner, addr)?;
        increment(value).store_at(&mut *mem.inner, addr)
    }

    #[inline(always)]
    fn exec_fma_store<
        T: InternalValue + MemValue<N> + core::ops::Add<Output = T> + core::ops::Mul<Output = T>,
        const N: usize,
    >(
        &mut self,
        m: MemoryArg,
    ) -> Result<(), Trap> {
        let rhs = T::stack_pop(&mut self.store.value_stack);
        let lhs = T::stack_pop(&mut self.store.value_stack);
        let acc = T::stack_pop(&mut self.store.value_stack);
        let fma = acc + lhs * rhs;
        let mem_addr = self.module.resolve_mem_addr(m.mem_addr());
        let mem = self.store.state.get_mem(mem_addr);
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = mem.effective_addr::<N>(base, m.offset())?;
        let mem = self.store.state.get_mem_mut(mem_addr);
        fma.store_at(&mut *mem.inner, addr)?;
        Ok(())
    }

    #[inline(always)]
    fn exec_load_local_value<T: MemValue<N>, const N: usize>(
        &self,
        memarg: MemoryArg,
        addr_local: u8,
    ) -> Result<T, Trap> {
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(memarg.mem_addr()));
        let addr = if mem.is_64bit() {
            let base = i64::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local)) as u64;
            let base = cold_err!(usize::try_from(base).map_err(|_| Trap::MemoryOutOfBounds {
                offset: usize::MAX,
                len: N,
                max: mem.inner.len(),
            }))?;
            mem.effective_addr::<N>(base, memarg.offset())?
        } else {
            let base = u32::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local));
            mem.effective_addr::<N>(base as usize, memarg.offset())?
        };
        T::load_at(&*mem.inner, addr)
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
        memarg: MemoryArg,
        addr_local: u8,
        dst_local: u8,
        cast: impl Fn(LOAD) -> TARGET,
    ) -> Result<(), Trap> {
        let value = cast(self.exec_load_local_value::<LOAD, N>(memarg, addr_local)?);
        if SET_LOCAL {
            TARGET::local_set(&mut self.store.value_stack, &self.cf, u16::from(dst_local), value);
        }
        if !SET_LOCAL || TEE {
            TARGET::stack_push(&mut self.store.value_stack, value)?;
        }
        Ok(())
    }

    fn exec_ref_is_null(&mut self) -> Result<(), Trap> {
        let is_null = i32::from(<ValueRef>::stack_pop(&mut self.store.value_stack).is_null());
        self.store.value_stack.push::<i32>(is_null)
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
        self.store.value_stack.push(i32::from(matches))
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
        self.store.value_stack.push(value)
    }

    fn push_gc_object(&mut self, type_addr: TypeAddr, values: Vec<TinyWasmValue>) -> Result<(), Trap> {
        let roots = (&self.store.value_stack.stack_32).into_iter().copied();
        let reference = self.store.state.alloc_gc_object(type_addr, values, roots)?;
        self.store.value_stack.push(reference)
    }

    fn exec_struct_new(&mut self, type_index: TypeAddr, default: bool) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let fields = &self.store.state.get_type(type_addr).as_struct().expect("validated struct.new type").fields;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(fields.len())).map_err(|_| Trap::OutOfMemory)?;
        if default {
            values.extend(fields.iter().map(|field| default_value(field.storage)));
        } else {
            for field in fields.iter().rev() {
                values.push(pop_value(&mut self.store.value_stack, field.storage));
            }
            values.reverse();
        }
        self.push_gc_object(type_addr, values)
    }

    fn exec_struct_get(&mut self, type_index: TypeAddr, field_index: u32, signed: Option<bool>) -> Result<(), Trap> {
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_struct().expect("validated struct.get type").fields
            [field_index as usize]
            .storage;
        let object = self.store.state.gc_object(reference, type_addr)?;
        let value = *object.values.get(field_index as usize).expect("validated struct field index");
        push_value(&mut self.store.value_stack, value, storage, signed)
    }

    fn exec_struct_set(&mut self, type_index: TypeAddr, field_index: u32) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_struct().expect("validated struct.set type").fields
            [field_index as usize]
            .storage;
        let value = pop_value(&mut self.store.value_stack, storage);
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        self.store.state.gc_object(reference, type_addr)?;
        self.store.state.gc.set(reference, field_index as usize, value).expect("live struct field");
        Ok(())
    }

    fn exec_array_new(&mut self, type_index: TypeAddr, default: bool) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array.new type").field.storage;
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let value = if default { default_value(storage) } else { pop_value(&mut self.store.value_stack, storage) };
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        values.resize(len, value);
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_new_fixed(&mut self, type_index: TypeAddr, len: u32) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage =
            self.store.state.get_type(type_addr).as_array().expect("validated array.new_fixed type").field.storage;
        let len = len as usize;
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
        let value = *object.values.get(index).ok_or(Trap::ArrayOutOfBounds)?;
        push_value(&mut self.store.value_stack, value, storage, signed)
    }

    fn exec_array_set(&mut self, type_index: TypeAddr) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array.set type").field.storage;
        let value = pop_value(&mut self.store.value_stack, storage);
        let index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        self.store.state.gc_object(reference, type_addr)?;
        self.store.state.gc.set(reference, index, value).ok_or(Trap::ArrayOutOfBounds)
    }

    fn exec_array_len(&mut self) -> Result<(), Trap> {
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        if reference.is_null() {
            return Err(Trap::NullArrayReference);
        }
        let object = self.store.state.gc.get(reference).ok_or(Trap::Other("invalid GC reference"))?;
        if self.store.state.get_type(object.type_addr).as_array().is_none() {
            return Err(Trap::Other("GC reference is not an array"));
        }
        self.store.value_stack.push(object.values.len() as i32)
    }

    fn exec_array_fill(&mut self, type_index: TypeAddr) -> Result<(), Trap> {
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array.fill type").field.storage;
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let value = pop_value(&mut self.store.value_stack, storage);
        let index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let object = self.store.state.gc_object(reference, type_addr)?;
        let end = index.checked_add(len).filter(|end| *end <= object.values.len()).ok_or(Trap::ArrayOutOfBounds)?;
        self.store.state.gc.fill(reference, index..end, value).expect("live array range");
        Ok(())
    }

    fn exec_array_copy(&mut self, dst_type: TypeAddr, src_type: TypeAddr) -> Result<(), Trap> {
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src_index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = ValueRef::stack_pop(&mut self.store.value_stack);
        let dst_index = u32::stack_pop(&mut self.store.value_stack) as usize;
        let dst = ValueRef::stack_pop(&mut self.store.value_stack);
        let dst_type = self.module.resolve_type_addr(dst_type);
        let src_type = self.module.resolve_type_addr(src_type);
        let dst_len = self.store.state.gc_object(dst, dst_type)?.values.len();
        let src_object = self.store.state.gc_object(src, src_type)?;
        let src_end =
            src_index.checked_add(len).filter(|end| *end <= src_object.values.len()).ok_or(Trap::ArrayOutOfBounds)?;
        dst_index.checked_add(len).filter(|end| *end <= dst_len).ok_or(Trap::ArrayOutOfBounds)?;
        if src == dst {
            self.store.state.gc.copy_within(dst, src_index..src_end, dst_index).expect("live array range");
            return Ok(());
        }
        self.store.state.gc.copy_between(src, src_index..src_end, dst, dst_index).expect("live array ranges");
        Ok(())
    }

    fn exec_array_new_data(&mut self, type_index: TypeAddr, data_index: DataAddr) -> Result<(), Trap> {
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = u32::stack_pop(&mut self.store.value_stack) as usize;
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array type").field.storage;
        let data_addr = self.module.resolve_data_addr(data_index);
        let data = self.store.state.data[data_addr as usize].data.as_deref().unwrap_or(&[]);
        let values = decode_data(storage, data, src, len)?;
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_new_elem(&mut self, type_index: TypeAddr, elem_index: ElemAddr) -> Result<(), Trap> {
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = u32::stack_pop(&mut self.store.value_stack) as usize;
        let elem_addr = self.module.resolve_elem_addr(elem_index);
        let items = self.store.state.elements[elem_addr as usize].items_range(src, len)?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        values.extend(items.iter().copied().map(TinyWasmValue::ValueRef));
        let type_addr = self.module.resolve_type_addr(type_index);
        self.push_gc_object(type_addr, values)
    }

    fn exec_array_init_data(&mut self, type_index: TypeAddr, data_index: DataAddr) -> Result<(), Trap> {
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = u32::stack_pop(&mut self.store.value_stack) as usize;
        let dst = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let storage = self.store.state.get_type(type_addr).as_array().expect("validated array type").field.storage;
        let object_len = self.store.state.gc_object(reference, type_addr)?.values.len();
        dst.checked_add(len).filter(|end| *end <= object_len).ok_or(Trap::ArrayOutOfBounds)?;
        let data =
            self.store.state.data[self.module.resolve_data_addr(data_index) as usize].data.as_deref().unwrap_or(&[]);
        let values = decode_data(storage, data, src, len)?;
        self.store.state.gc.set_slice(reference, dst, &values).expect("live array range");
        Ok(())
    }

    fn exec_array_init_elem(&mut self, type_index: TypeAddr, elem_index: ElemAddr) -> Result<(), Trap> {
        let len = u32::stack_pop(&mut self.store.value_stack) as usize;
        let src = u32::stack_pop(&mut self.store.value_stack) as usize;
        let dst = u32::stack_pop(&mut self.store.value_stack) as usize;
        let reference = ValueRef::stack_pop(&mut self.store.value_stack);
        let type_addr = self.module.resolve_type_addr(type_index);
        let object_len = self.store.state.gc_object(reference, type_addr)?.values.len();
        dst.checked_add(len).filter(|end| *end <= object_len).ok_or(Trap::ArrayOutOfBounds)?;
        let items =
            self.store.state.elements[self.module.resolve_elem_addr(elem_index) as usize].items_range(src, len)?;
        let mut values = Vec::new();
        cold_err!(values.try_reserve_exact(len)).map_err(|_| Trap::OutOfMemory)?;
        values.extend(items.iter().copied().map(TinyWasmValue::ValueRef));
        self.store.state.gc.set_slice(reference, dst, &values).expect("live array range");
        Ok(())
    }

    fn exec_memory_size(&mut self, addr: u32) -> Result<(), Trap> {
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(addr));
        match mem.is_64bit() {
            true => self.store.value_stack.push::<i64>(mem.page_count as i64),
            false => self.store.value_stack.push::<i32>(mem.page_count as i32),
        }
    }

    fn exec_memory_grow(&mut self, addr: u32) -> Result<(), Trap> {
        let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(addr));
        let is_64bit = mem.is_64bit();
        let pages_delta = match is_64bit {
            true => <i64>::stack_pop(&mut self.store.value_stack),
            false => i64::from(<i32>::stack_pop(&mut self.store.value_stack)),
        };

        let size = mem.grow(pages_delta, self.store.engine.config().trap_on_oom())?.unwrap_or(-1);
        match is_64bit {
            true => self.store.value_stack.push::<i64>(size)?,
            false => self.store.value_stack.push::<i32>(size as i32)?,
        };

        Ok(())
    }

    fn exec_memory_copy(&mut self, dst_mem: u32, src_mem: u32) -> Result<(), Trap> {
        let dst_mem_addr = self.module.resolve_mem_addr(dst_mem);
        let src_mem_addr = self.module.resolve_mem_addr(src_mem);
        let dst_arch = self.store.state.get_mem(dst_mem_addr).kind.arch();
        let src_arch = self.store.state.get_mem(src_mem_addr).kind.arch();
        let len_arch =
            if dst_arch == MemoryArch::I32 || src_arch == MemoryArch::I32 { MemoryArch::I32 } else { MemoryArch::I64 };
        let size = self.store.value_stack.pop_memory_operand(len_arch)?;
        let src = self.store.value_stack.pop_memory_operand(src_arch)?;
        let dst = self.store.value_stack.pop_memory_operand(dst_arch)?;

        if dst_mem == src_mem {
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
        let mem_addr = self.module.resolve_mem_addr(addr);
        let arch = self.store.state.get_mem(mem_addr).kind.arch();
        let size = self.store.value_stack.pop_memory_operand(arch)?;
        let val = i32::stack_pop(&mut self.store.value_stack);
        let dst = self.store.value_stack.pop_memory_operand(arch)?;
        self.exec_memory_fill_impl(mem_addr, dst, val as u8, size)
    }

    fn exec_memory_fill_imm(&mut self, addr: u32, val: u8, size: i32) -> Result<(), Trap> {
        let mem_addr = self.module.resolve_mem_addr(addr);
        let arch = self.store.state.get_mem(mem_addr).kind.arch();
        let dst = self.store.value_stack.pop_memory_operand(arch)?;
        self.exec_memory_fill_impl(mem_addr, dst, val, size as u32 as usize)
    }

    fn exec_memory_fill_impl(&mut self, mem_addr: MemAddr, dst: usize, val: u8, size: usize) -> Result<(), Trap> {
        let mem = self.store.state.get_mem_mut(mem_addr);
        let max = mem.inner.len();
        if mem.inner.fill(dst, size, val)?.is_none() {
            return cold!(Err(Trap::MemoryOutOfBounds { offset: dst, len: size, max }));
        }
        Ok(())
    }

    fn exec_memory_init(&mut self, data_index: u32, mem_index: u32) -> Result<(), Trap> {
        let size = u32::stack_pop(&mut self.store.value_stack) as usize;
        let offset = u32::stack_pop(&mut self.store.value_stack) as usize;
        let mem_addr = self.module.resolve_mem_addr(mem_index);
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

        if mem.inner.write_all(dst, &data[offset..offset + size])?.is_none() {
            return cold!(Err(Trap::MemoryOutOfBounds { offset: dst, len: size, max: mem_len }));
        }
        Ok(())
    }

    fn exec_table_copy(&mut self, dst_table: u32, src_table: u32) -> Result<(), Trap> {
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
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        lane: u8,
    ) -> Result<(), Trap> {
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(mem_addr));
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = mem.effective_addr::<LOAD_SIZE>(base, offset)?;
        let val = LOAD::load_at(&*mem.inner, addr)?;
        let offset = lane as usize * LOAD_SIZE;
        let mut imm = <Value128>::stack_pop(&mut self.store.value_stack).to_mem_bytes();
        imm[offset..offset + LOAD_SIZE].copy_from_slice(&val.to_mem_bytes());
        self.store.value_stack.push(Value128(imm))?;
        Ok(())
    }

    #[inline(always)]
    fn exec_mem_load<LOAD: MemValue<LOAD_SIZE>, const LOAD_SIZE: usize, TARGET: InternalValue>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        cast: impl Fn(LOAD) -> TARGET,
    ) -> Result<(), Trap> {
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(mem_addr));
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = mem.effective_addr::<LOAD_SIZE>(base, offset)?;
        let value = cold_err!(LOAD::load_at(&*mem.inner, addr))?;
        self.store.value_stack.push(cast(value))
    }

    fn exec_mem_store_lane<U: MemValue<N> + Copy, const N: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        lane: u8,
    ) -> Result<(), Trap> {
        let bytes = <Value128>::stack_pop(&mut self.store.value_stack).to_mem_bytes();
        let lane_offset = lane as usize * N;
        let mut val_bytes = [0u8; N];
        val_bytes.copy_from_slice(&bytes[lane_offset..lane_offset + N]);
        let val = U::from_mem_bytes(val_bytes);
        let mem_addr = self.module.resolve_mem_addr(mem_addr);
        let mem = self.store.state.get_mem(mem_addr);
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = mem.effective_addr::<N>(base, offset)?;
        let mem = self.store.state.get_mem_mut(mem_addr);
        cold_err!(val.store_at(&mut *mem.inner, addr))?;
        Ok(())
    }

    fn exec_mem_store<T: InternalValue, U: MemValue<N>, const N: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        cast: impl Fn(T) -> U,
    ) -> Result<(), Trap> {
        let val = <T>::stack_pop(&mut self.store.value_stack);
        let val = cast(val);

        let mem_addr = self.module.resolve_mem_addr(mem_addr);
        let mem = self.store.state.get_mem(mem_addr);
        let base = self.store.value_stack.pop_memory_operand(mem.kind.arch())?;
        let addr = mem.effective_addr::<N>(base, offset)?;
        let mem = self.store.state.get_mem_mut(mem_addr);
        cold_err!(val.store_at(&mut *mem.inner, addr))?;
        Ok(())
    }

    fn exec_table_get(&mut self, table_index: u32) -> Result<(), Trap> {
        let table_addr = self.module.resolve_table_addr(table_index);
        let idx = self.pop_table_operand(self.store.state.get_table(table_addr).kind.arch())?;
        let value = *self.store.state.get_table(table_addr).get(idx)?;
        self.store.value_stack.push(value)
    }

    fn exec_table_set(&mut self, table_index: u32) -> Result<(), Trap> {
        let val = <ValueRef>::stack_pop(&mut self.store.value_stack);
        let table_addr = self.module.resolve_table_addr(table_index);
        let idx = self.pop_table_operand(self.store.state.get_table(table_addr).kind.arch())?;
        let table = self.store.state.get_table_mut(table_addr);
        table.set(idx, val)
    }

    fn exec_table_size(&mut self, table_index: u32) -> Result<(), Trap> {
        let table = self.store.state.get_table(self.module.resolve_table_addr(table_index));
        match table.kind.arch() {
            MemoryArch::I32 => self.store.value_stack.push(table.size() as i32),
            MemoryArch::I64 => self.store.value_stack.push(table.size() as i64),
        }
    }

    fn exec_table_init(&mut self, elem_index: u32, table_index: u32) -> Result<(), Trap> {
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
        let val = <ValueRef>::stack_pop(&mut self.store.value_stack);
        let table = self.store.state.get_table_mut(table_addr);
        let sz = table.size();
        let result = table.grow(n, val);
        match (arch, result) {
            (MemoryArch::I32, Ok(())) => self.store.value_stack.push(sz as i32),
            (MemoryArch::I32, Err(_)) => self.store.value_stack.push(-1_i32),
            (MemoryArch::I64, Ok(())) => self.store.value_stack.push(sz as i64),
            (MemoryArch::I64, Err(_)) => self.store.value_stack.push(-1_i64),
        }
    }

    fn exec_table_fill(&mut self, table_index: u32) -> Result<(), Trap> {
        let table_addr = self.module.resolve_table_addr(table_index);
        let arch = self.store.state.get_table(table_addr).kind.arch();
        let n = self.pop_table_operand(arch)?;
        let val = <ValueRef>::stack_pop(&mut self.store.value_stack);
        let i = self.pop_table_operand(arch)?;
        self.store.state.get_table_mut(table_addr).fill(i, n, val)
    }

    fn pop_table_operand(&mut self, arch: MemoryArch) -> Result<usize, Trap> {
        let value = match arch {
            MemoryArch::I32 => <i32>::stack_pop(&mut self.store.value_stack) as u32 as u64,
            MemoryArch::I64 => <i64>::stack_pop(&mut self.store.value_stack) as u64,
        };
        cold_err!(usize::try_from(value).map_err(|_| Trap::TableOutOfBounds {
            offset: usize::MAX,
            len: 1,
            max: usize::MAX,
        }))
    }
}

impl<'store> Executor<'store, false> {
    #[inline(always)]
    pub(crate) fn run_to_completion(mut self) -> Result<()> {
        // ideally we use `loop_match` / `become` once thats stabilized
        loop {
            if self.exec(self.cf.instr_ptr)?.is_some() {
                return Ok(());
            }
        }
    }

    #[cfg(feature = "std")]
    #[inline(always)]
    pub(crate) fn run_with_time_budget(mut self, time_budget: core::time::Duration) -> Result<ExecState> {
        use crate::std::time::Instant;
        let start = Instant::now();
        if time_budget.is_zero() {
            return Ok(ExecState::Suspended(self.cf));
        }

        loop {
            for _ in 0..128 {
                if self.exec(self.cf.instr_ptr)?.is_some() {
                    return Ok(ExecState::Completed);
                }
            }

            if start.elapsed() >= time_budget {
                return Ok(ExecState::Suspended(self.cf));
            }
        }
    }
}

impl<'store> Executor<'store, true> {
    #[inline(always)]
    pub(crate) fn run_with_fuel(mut self, fuel: u32) -> Result<ExecState> {
        self.store.execution_fuel = fuel;
        if self.store.execution_fuel == 0 {
            return Ok(ExecState::Suspended(self.cf));
        }

        loop {
            for _ in 0..128 {
                if self.exec(self.cf.instr_ptr)?.is_some() {
                    return Ok(ExecState::Completed);
                }
            }

            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(128);
            if self.store.execution_fuel == 0 {
                return Ok(ExecState::Suspended(self.cf));
            }
        }
    }
}

#[inline(always)]
fn cmp_i32(lhs: i32, rhs: i32, op: CmpOp) -> bool {
    match op {
        CmpOp::Eq => lhs == rhs,
        CmpOp::Ne => lhs != rhs,
        CmpOp::LtS => lhs < rhs,
        CmpOp::LtU => (lhs as u32) < (rhs as u32),
        CmpOp::GtS => lhs > rhs,
        CmpOp::GtU => (lhs as u32) > (rhs as u32),
        CmpOp::LeS => lhs <= rhs,
        CmpOp::LeU => (lhs as u32) <= (rhs as u32),
        CmpOp::GeS => lhs >= rhs,
        CmpOp::GeU => (lhs as u32) >= (rhs as u32),
    }
}

#[inline(always)]
fn cmp_i64(lhs: i64, rhs: i64, op: CmpOp) -> bool {
    match op {
        CmpOp::Eq => lhs == rhs,
        CmpOp::Ne => lhs != rhs,
        CmpOp::LtS => lhs < rhs,
        CmpOp::LtU => (lhs as u64) < (rhs as u64),
        CmpOp::GtS => lhs > rhs,
        CmpOp::GtU => (lhs as u64) > (rhs as u64),
        CmpOp::LeS => lhs <= rhs,
        CmpOp::LeU => (lhs as u64) <= (rhs as u64),
        CmpOp::GeS => lhs >= rhs,
        CmpOp::GeU => (lhs as u64) >= (rhs as u64),
    }
}

fn exec_binop_32(op: BinOp, lhs: u32, rhs: u32) -> u32 {
    match op {
        BinOp::IAdd => lhs.wrapping_add(rhs),
        BinOp::ISub => lhs.wrapping_sub(rhs),
        BinOp::IMul => lhs.wrapping_mul(rhs),
        BinOp::IAnd => lhs & rhs,
        BinOp::IOr => lhs | rhs,
        BinOp::IXor => lhs ^ rhs,
        BinOp::IShl => (lhs as i32).wrapping_shl(rhs) as u32,
        BinOp::IShrS => (lhs as i32).wrapping_shr(rhs) as u32,
        BinOp::IShrU => lhs.wrapping_shr(rhs),
        BinOp::IRotl => (lhs as i32).rotate_left(rhs) as u32,
        BinOp::IRotr => (lhs as i32).rotate_right(rhs) as u32,
        BinOp::FAdd => (f32::from_bits(lhs) + f32::from_bits(rhs)).to_bits(),
        BinOp::FSub => (f32::from_bits(lhs) - f32::from_bits(rhs)).to_bits(),
        BinOp::FMul => (f32::from_bits(lhs) * f32::from_bits(rhs)).to_bits(),
        BinOp::FDiv => (f32::from_bits(lhs) / f32::from_bits(rhs)).to_bits(),
        BinOp::FMin => f32::from_bits(lhs).tw_minimum(f32::from_bits(rhs)).to_bits(),
        BinOp::FMax => f32::from_bits(lhs).tw_maximum(f32::from_bits(rhs)).to_bits(),
        BinOp::FCopysign => f32::from_bits(lhs).copysign(f32::from_bits(rhs)).to_bits(),
    }
}

fn exec_binop_64(op: BinOp, lhs: u64, rhs: u64) -> u64 {
    match op {
        BinOp::IAdd => lhs.wrapping_add(rhs),
        BinOp::ISub => lhs.wrapping_sub(rhs),
        BinOp::IMul => lhs.wrapping_mul(rhs),
        BinOp::IAnd => lhs & rhs,
        BinOp::IOr => lhs | rhs,
        BinOp::IXor => lhs ^ rhs,
        BinOp::IShl => (lhs as i64).wrapping_shl(rhs as u32) as u64,
        BinOp::IShrS => (lhs as i64).wrapping_shr(rhs as u32) as u64,
        BinOp::IShrU => lhs.wrapping_shr(rhs as u32),
        BinOp::IRotl => (lhs as i64).rotate_left(rhs as u32) as u64,
        BinOp::IRotr => (lhs as i64).rotate_right(rhs as u32) as u64,
        BinOp::FAdd => (f64::from_bits(lhs) + f64::from_bits(rhs)).to_bits(),
        BinOp::FSub => (f64::from_bits(lhs) - f64::from_bits(rhs)).to_bits(),
        BinOp::FMul => (f64::from_bits(lhs) * f64::from_bits(rhs)).to_bits(),
        BinOp::FDiv => (f64::from_bits(lhs) / f64::from_bits(rhs)).to_bits(),
        BinOp::FMin => f64::from_bits(lhs).tw_minimum(f64::from_bits(rhs)).to_bits(),
        BinOp::FMax => f64::from_bits(lhs).tw_maximum(f64::from_bits(rhs)).to_bits(),
        BinOp::FCopysign => f64::from_bits(lhs).copysign(f64::from_bits(rhs)).to_bits(),
    }
}

fn exec_binop_128(op: BinOp128, lhs: Value128, rhs: Value128) -> Value128 {
    match op {
        BinOp128::And => lhs.v128_and(rhs),
        BinOp128::AndNot => lhs.v128_andnot(rhs),
        BinOp128::Or => lhs.v128_or(rhs),
        BinOp128::Xor => lhs.v128_xor(rhs),
        BinOp128::I64x2Add => lhs.i64x2_add(rhs),
        BinOp128::I64x2Mul => lhs.i64x2_mul(rhs),
    }
}
