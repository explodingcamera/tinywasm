use core::hint::cold_path;

#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use super::no_std_floats::NoStdFloatExt;

use alloc::boxed::Box;
use alloc::rc::Rc;

use alloc::sync::Arc;
use interpreter::stack::CallFrame;
use tinywasm_types::*;

use super::ExecState;
use super::num_helpers::*;
use super::values::*;
use crate::engine::FuelPolicy;
use crate::func::{FuncContext, HostFunction};
use crate::interpreter::Value128;
use crate::*;

const FUEL_COST_CALL_TOTAL: u32 = 5;

pub(crate) struct Executor<'store, const BUDGETED: bool> {
    cf: CallFrame,
    func: Arc<WasmFunction>,
    module: ModuleInstance,
    store: &'store mut Store,
}

impl<'store, const BUDGETED: bool> Executor<'store, BUDGETED> {
    pub(crate) fn new(store: &'store mut Store, cf: CallFrame) -> Self {
        let wasm_func = store.state.get_wasm_func(cf.func_addr);
        let module = store.get_module_instance_internal(wasm_func.owner);
        Self { module, cf, func: wasm_func.func.clone(), store }
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
    fn exec_binop_32(&self, op: BinOp, lhs: Value32, rhs: Value32) -> Value32 {
        match op {
            BinOp::IAdd => ((lhs as i32).wrapping_add(rhs as i32)) as u32,
            BinOp::ISub => ((lhs as i32).wrapping_sub(rhs as i32)) as u32,
            BinOp::IMul => ((lhs as i32).wrapping_mul(rhs as i32)) as u32,
            BinOp::IAnd => lhs & rhs,
            BinOp::IOr => lhs | rhs,
            BinOp::IXor => lhs ^ rhs,
            BinOp::IShl => ((lhs as i32).wasm_shl(rhs as i32)) as u32,
            BinOp::IShrS => ((lhs as i32).wasm_shr(rhs as i32)) as u32,
            BinOp::IShrU => lhs.wasm_shr(rhs),
            BinOp::IRotl => ((lhs as i32).wasm_rotl(rhs as i32)) as u32,
            BinOp::IRotr => ((lhs as i32).wasm_rotr(rhs as i32)) as u32,
            BinOp::FAdd => (f32::from_bits(lhs) + f32::from_bits(rhs)).to_bits(),
            BinOp::FSub => (f32::from_bits(lhs) - f32::from_bits(rhs)).to_bits(),
            BinOp::FMul => (f32::from_bits(lhs) * f32::from_bits(rhs)).to_bits(),
            BinOp::FDiv => (f32::from_bits(lhs) / f32::from_bits(rhs)).to_bits(),
            BinOp::FMin => f32::from_bits(lhs).tw_minimum(f32::from_bits(rhs)).to_bits(),
            BinOp::FMax => f32::from_bits(lhs).tw_maximum(f32::from_bits(rhs)).to_bits(),
            BinOp::FCopysign => f32::from_bits(lhs).copysign(f32::from_bits(rhs)).to_bits(),
        }
    }

    #[inline(always)]
    fn exec_binop_64(&self, op: BinOp, lhs: Value64, rhs: Value64) -> Value64 {
        match op {
            BinOp::IAdd => ((lhs as i64).wrapping_add(rhs as i64)) as u64,
            BinOp::ISub => ((lhs as i64).wrapping_sub(rhs as i64)) as u64,
            BinOp::IMul => ((lhs as i64).wrapping_mul(rhs as i64)) as u64,
            BinOp::IAnd => lhs & rhs,
            BinOp::IOr => lhs | rhs,
            BinOp::IXor => lhs ^ rhs,
            BinOp::IShl => ((lhs as i64).wasm_shl(rhs as i64)) as u64,
            BinOp::IShrS => ((lhs as i64).wasm_shr(rhs as i64)) as u64,
            BinOp::IShrU => lhs.wasm_shr(rhs),
            BinOp::IRotl => ((lhs as i64).wasm_rotl(rhs as i64)) as u64,
            BinOp::IRotr => ((lhs as i64).wasm_rotr(rhs as i64)) as u64,
            BinOp::FAdd => (f64::from_bits(lhs) + f64::from_bits(rhs)).to_bits(),
            BinOp::FSub => (f64::from_bits(lhs) - f64::from_bits(rhs)).to_bits(),
            BinOp::FMul => (f64::from_bits(lhs) * f64::from_bits(rhs)).to_bits(),
            BinOp::FDiv => (f64::from_bits(lhs) / f64::from_bits(rhs)).to_bits(),
            BinOp::FMin => f64::from_bits(lhs).tw_minimum(f64::from_bits(rhs)).to_bits(),
            BinOp::FMax => f64::from_bits(lhs).tw_maximum(f64::from_bits(rhs)).to_bits(),
            BinOp::FCopysign => f64::from_bits(lhs).copysign(f64::from_bits(rhs)).to_bits(),
        }
    }

    #[inline(always)]
    fn exec_binop_128(&self, op: BinOp128, lhs: Value128, rhs: Value128) -> Value128 {
        match op {
            BinOp128::And => lhs.v128_and(rhs),
            BinOp128::AndNot => lhs.v128_andnot(rhs),
            BinOp128::Or => lhs.v128_or(rhs),
            BinOp128::Xor => lhs.v128_xor(rhs),
            BinOp128::I64x2Add => lhs.i64x2_add(rhs),
            BinOp128::I64x2Mul => lhs.i64x2_mul(rhs),
        }
    }

    #[inline(always)]
    fn exec(&mut self) -> Result<Option<()>, Trap> {
        macro_rules! stack_op {
            (unary $ty:ty, |$v:ident| $expr:expr) => {{
                let $v = <$ty>::stack_pop(&mut self.store.value_stack);
                <$ty>::stack_push(&mut self.store.value_stack, $expr)?;
            }};
            (binary $ty:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                let $rhs = <$ty>::stack_pop(&mut self.store.value_stack);
                let $lhs = <$ty>::stack_pop(&mut self.store.value_stack);
                <$ty>::stack_push(&mut self.store.value_stack, $expr)?;
            }};
            (binary try $ty:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
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
            (binary_into2 $from:ty => $to:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                let $rhs = <$from>::stack_pop(&mut self.store.value_stack);
                let $lhs = <$from>::stack_pop(&mut self.store.value_stack);
                let out = $expr;
                <$to>::stack_push(&mut self.store.value_stack, out.0)?;
                <$to>::stack_push(&mut self.store.value_stack, out.1)?;
            }};
            (binary $lhs_ty:ty, $rhs_ty:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {
                stack_op!(binary $lhs_ty, $rhs_ty => $rhs_ty, |$lhs, $rhs| $expr)
            };
            (binary $lhs_ty:ty, $rhs_ty:ty => $res:ty, |$lhs:ident, $rhs:ident| $expr:expr) => {{
                let $rhs = <$rhs_ty>::stack_pop(&mut self.store.value_stack);
                let $lhs = <$lhs_ty>::stack_pop(&mut self.store.value_stack);
                <$res>::stack_push(&mut self.store.value_stack, $expr)?;
            }};
            (ternary $ty:ty, |$a:ident, $b:ident, $c:ident| $expr:expr) => {{
                let $c = <$ty>::stack_pop(&mut self.store.value_stack);
                let $b = <$ty>::stack_pop(&mut self.store.value_stack);
                let $a = <$ty>::stack_pop(&mut self.store.value_stack);
                <$ty>::stack_push(&mut self.store.value_stack, $expr)?;
            }};
            (quaternary_into2 $from:ty => $to:ty, |$a:ident, $b:ident, $c:ident, $d:ident| $expr:expr) => {{
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
        }

        macro_rules! binop {
            (local_local $vt:ty, $exec:ident, $op:ident, $a:ident, $b:ident) => {{
                self.store.value_stack.push(self.$exec(
                    *$op,
                    <$vt>::local_get(&self.store.value_stack, &self.cf, *$a),
                    <$vt>::local_get(&self.store.value_stack, &self.cf, *$b),
                ))?
            }};
            (local_local_set $vt:ty, $exec:ident, $op:ident, $a:ident, $b:ident, $dst:ident) => {{
                let value = self.$exec(
                    *$op,
                    <$vt>::local_get(&self.store.value_stack, &self.cf, *$a),
                    <$vt>::local_get(&self.store.value_stack, &self.cf, *$b),
                );
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
            }};
            (local_local_tee $vt:ty, $exec:ident, $op:ident, $a:ident, $b:ident, $dst:ident) => {{
                let value = self.$exec(
                    *$op,
                    <$vt>::local_get(&self.store.value_stack, &self.cf, *$a),
                    <$vt>::local_get(&self.store.value_stack, &self.cf, *$b),
                );
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
                self.store.value_stack.push(value)?;
            }};
            (local_const $vt:ty, $exec:ident, $op:ident, $local:ident, $rhs:expr) => {{
                self.store.value_stack.push(self.$exec(
                    *$op,
                    <$vt>::local_get(&self.store.value_stack, &self.cf, *$local),
                    $rhs,
                ))?
            }};
            (local_const_set $vt:ty, $exec:ident, $op:ident, $local:ident, $rhs:expr, $dst:ident) => {{
                let value = self.$exec(*$op, <$vt>::local_get(&self.store.value_stack, &self.cf, *$local), $rhs);
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
            }};
            (local_const_tee $vt:ty, $exec:ident, $op:ident, $local:ident, $rhs:expr, $dst:ident) => {{
                let value = self.$exec(*$op, <$vt>::local_get(&self.store.value_stack, &self.cf, *$local), $rhs);
                <$vt>::local_set(&mut self.store.value_stack, &self.cf, *$dst, value);
                self.store.value_stack.push(value)?;
            }};
            (stack_global $vt:ty, $as_fn:ident, $exec:ident, $op:ident, $global:ident) => {{
                let global_val =
                    self.store.state.get_global_val(self.module.resolve_global_addr(*$global)).$as_fn().unwrap();
                let stack_val = <$vt>::stack_pop(&mut self.store.value_stack);
                self.store.value_stack.push(self.$exec(*$op, stack_val, global_val))?;
            }};
        }

        let next = match self.func.instructions.get(self.cf.instr_ptr as usize) {
            Some(instr) => instr,
            None => {
                cold_path();
                unreachable!(
                    "Instruction pointer out of bounds: {} ({} instructions)",
                    self.cf.instr_ptr,
                    self.func.instructions.len()
                )
            }
        };

        use tinywasm_types::Instruction::*;
        #[rustfmt::skip]
        match next {
            Nop | MergeBarrier => {}
            Unreachable => return Err(Trap::Unreachable),
            Drop32 => { _ = Value32::stack_pop(&mut self.store.value_stack)},
            Drop64 => { _ = Value64::stack_pop(&mut self.store.value_stack)},
            Drop128 => { _ = Value128::stack_pop(&mut self.store.value_stack)},
            Select32 => Value32::stack_select(&mut self.store.value_stack)?,
            Select64 => Value64::stack_select(&mut self.store.value_stack)?,
            Select128 => Value128::stack_select(&mut self.store.value_stack)?,
            SelectMulti(counts) => self.store.value_stack.select_multi(*counts),
            Call(v) => { self.exec_call_direct(*v)?; return Ok(None); }
            CallSelf => { self.exec_call_self()?; return Ok(None); }
            CallIndirect(ty, table) => { self.exec_call_indirect::<false>(*ty, *table)?; return Ok(None); }
            ReturnCall(v) => { self.exec_return_call_direct(*v)?; return Ok(None); }
            ReturnCallSelf => { self.exec_return_call_self()?; return Ok(None); }
            ReturnCallIndirect(ty, table) => { self.exec_call_indirect::<true>(*ty, *table)?; return Ok(None); }
            Jump(ip) => { self.exec_jump(*ip); return Ok(None); }
            JumpIfZero(ip) => { let condition = <i32>::stack_pop(&mut self.store.value_stack) == 0; if self.jump_if(condition, *ip) { return Ok(None) }}
            JumpIfNonZero(ip) => { let condition = <i32>::stack_pop(&mut self.store.value_stack) != 0; if self.jump_if(condition, *ip) { return Ok(None) }}
            JumpIfZero32(ip) => { let condition = <Value32>::stack_pop(&mut self.store.value_stack) == 0; if self.jump_if(condition, *ip) { return Ok(None) }}
            JumpIfNonZero32(ip) => { let condition = <Value32>::stack_pop(&mut self.store.value_stack) != 0; if self.jump_if(condition, *ip) { return Ok(None) }}
            JumpIfZero64(ip) => { let condition = <Value64>::stack_pop(&mut self.store.value_stack) == 0; if self.jump_if(condition, *ip) { return Ok(None) }}
            JumpIfNonZero64(ip) => { let condition = <Value64>::stack_pop(&mut self.store.value_stack) != 0; if self.jump_if(condition, *ip) { return Ok(None) }}
            JumpIfLocalZero32 { target_ip, local } => if self.exec_jump_local_zero_32(*target_ip, *local) { return Ok(None) },
            JumpIfLocalNonZero32 { target_ip, local } => if self.exec_jump_local_non_zero_32(*target_ip, *local) { return Ok(None) },
            JumpIfLocalZero64 { target_ip, local } => if self.exec_jump_local_zero_64(*target_ip, *local) { return Ok(None) },
            JumpIfLocalNonZero64 { target_ip, local } => if self.exec_jump_local_non_zero_64(*target_ip, *local) { return Ok(None) },
            JumpCmpStackConst32 { target_ip, imm, op } => if self.exec_jump_cmp_stack_const_32(*target_ip, *imm, *op) { return Ok(None) },
            JumpCmpStackConst64 { target_ip, imm, op } => if self.exec_jump_cmp_stack_const_64(*target_ip, *imm, *op) { return Ok(None) },
            JumpCmpLocalConst32 { target_ip, local, imm, op } => if self.exec_jump_cmp_local_const_32(*target_ip, *local, *imm, *op) { return Ok(None) },
            JumpCmpLocalConst64 { target_ip, local, imm, op } => if self.exec_jump_cmp_local_const_64(*target_ip, *local, *imm, *op) { return Ok(None) },
            JumpCmpLocalLocal32 { target_ip, left, right, op } => if self.exec_jump_cmp_local_local_32(*target_ip, *left, *right, *op) { return Ok(None) },
            JumpCmpLocalLocal64 { target_ip, left, right, op } => if self.exec_jump_cmp_local_local_64(*target_ip, *left, *right, *op) { return Ok(None) },
            DropKeep { base32, keep32, base64, keep64, base128, keep128 } => {
                let mut base = self.cf.stack_base(); base.s32 += *base32 as u32; base.s64 += *base64 as u32; base.s128 += *base128 as u32;
                self.store.value_stack.truncate_keep_counts(base, ValueCounts { c32: *keep32 as u16, c64: *keep64 as u16, c128: *keep128 as u16 });
            }
            DropKeep32(base, keep) => self.store.value_stack.stack_32.truncate_keep((self.cf.stack_base().s32 + *base as u32) as usize, *keep as usize),
            DropKeep64(base, keep) => self.store.value_stack.stack_64.truncate_keep((self.cf.stack_base().s64 + *base as u32) as usize, *keep as usize),
            DropKeep128(base, keep) => self.store.value_stack.stack_128.truncate_keep((self.cf.stack_base().s128 + *base as u32) as usize, *keep as usize),
            BranchTable(default_ip, start, len) => { self.exec_branch_table(*default_ip, *start, *len); return Ok(None); }
            Return => { if self.exec_return() { return Ok(Some(())); } return Ok(None); }
            LocalGet32(local_index) => self.store.value_stack.push(Value32::local_get(&self.store.value_stack, &self.cf, *local_index))?,
            LocalGet64(local_index) => self.store.value_stack.push(Value64::local_get(&self.store.value_stack, &self.cf, *local_index))?,
            LocalGet128(local_index) => self.store.value_stack.push(Value128::local_get(&self.store.value_stack, &self.cf, *local_index))?,
            LocalSet32(local_index) => stack_op!(local_set_pop Value32, local_index),
            LocalSet64(local_index) => stack_op!(local_set_pop Value64, local_index),
            LocalSet128(local_index) => stack_op!(local_set_pop Value128, local_index),
            LocalCopy32(from, to) => Value32::local_copy(&mut self.store.value_stack, &self.cf, *from, *to),
            LocalCopy64(from, to) => Value64::local_copy(&mut self.store.value_stack, &self.cf, *from, *to),
            LocalCopy128(from, to) => Value128::local_copy(&mut self.store.value_stack, &self.cf, *from, *to),
            AddConst32(c) => stack_op!(unary i32, |v| v.wrapping_add(*c)),
            AddConst64(c) => stack_op!(unary i64, |v| v.wrapping_add(*c)),
            IncLocal32(local_index, delta) => i32::local_update(&mut self.store.value_stack, &self.cf, *local_index, |v| v.wrapping_add(*delta)),
            IncLocal64(local_index, delta) => i64::local_update(&mut self.store.value_stack, &self.cf, *local_index, |v| v.wrapping_add(*delta )),
            I32Add3 => stack_op!(ternary i32, |a, b, c| a.wrapping_add(b).wrapping_add(c)),
            I64Add3 => stack_op!(ternary i64, |a, b, c| a.wrapping_add(b).wrapping_add(c)),
            MulAccLocal32(acc) => self.exec_binop_acc_local::<i32, _, _>(*acc, |a, b| a.wrapping_mul(b), |a, b| a.wrapping_add(b)),
            MulAccLocal64(acc) => self.exec_binop_acc_local::<i64, _, _>(*acc, |a, b| a.wrapping_mul(b), |a, b| a.wrapping_add(b)),
            FMulAccLocal32(acc) => self.exec_binop_acc_local::<f32, _, _>(*acc, |a, b| a * b, |a, b| a + b),
            FMulAccLocal64(acc) => self.exec_binop_acc_local::<f64, _, _>(*acc, |a, b| a * b, |a, b| a + b),
            BinOpLocalLocal32(op, a, b) => binop!(local_local Value32, exec_binop_32, op, a, b),
            BinOpLocalLocal64(op, a, b) => binop!(local_local Value64, exec_binop_64, op, a, b),
            BinOpLocalLocal128(op, a, b) => binop!(local_local Value128, exec_binop_128, op, a, b),
            BinOpLocalLocalSet32(op, a, b, dst) => binop!(local_local_set Value32, exec_binop_32, op, a, b, dst),
            BinOpLocalLocalSet64(op, a, b, dst) => binop!(local_local_set Value64, exec_binop_64, op, a, b, dst),
            BinOpLocalLocalSet128(op, a, b, dst) => binop!(local_local_set Value128, exec_binop_128, op, a, b, dst),
            BinOpLocalLocalTee32(op, a, b, dst) => binop!(local_local_tee Value32, exec_binop_32, op, a, b, dst),
            BinOpLocalLocalTee64(op, a, b, dst) => binop!(local_local_tee Value64, exec_binop_64, op, a, b, dst),
            BinOpLocalLocalTee128(op, a, b, dst) => binop!(local_local_tee Value128, exec_binop_128, op, a, b, dst),
            BinOpLocalConst32(op, local_index, c) => binop!(local_const Value32, exec_binop_32, op, local_index, *c as u32),
            BinOpLocalConst64(op, local_index, c) => binop!(local_const Value64, exec_binop_64, op, local_index, *c as u64),
            BinOpLocalConst128(op, local_index, c) => binop!(local_const Value128, exec_binop_128, op, local_index, Value128(self.func.data.v128_const(*c))),
            BinOpLocalConstSet32(op, local_index, c, dst) => binop!(local_const_set Value32, exec_binop_32, op, local_index, *c as u32, dst),
            BinOpLocalConstSet64(op, local_index, c, dst) => binop!(local_const_set Value64, exec_binop_64, op, local_index, *c as u64, dst),
            BinOpLocalConstSet128(op, local_index, c, dst) => binop!(local_const_set Value128, exec_binop_128, op, local_index, Value128(self.func.data.v128_const(*c)), dst),
            BinOpLocalConstTee32(op, local_index, c, dst) => binop!(local_const_tee Value32, exec_binop_32, op, local_index, *c as u32, dst),
            BinOpLocalConstTee64(op, local_index, c, dst) => binop!(local_const_tee Value64, exec_binop_64, op, local_index, *c as u64, dst),
            BinOpLocalConstTee128(op, local_index, c, dst) => binop!(local_const_tee Value128, exec_binop_128, op, local_index, Value128(self.func.data.v128_const(*c)), dst),
            BinOpStackGlobal32(op, global_index) => binop!(stack_global Value32, as_32, exec_binop_32, op, global_index),
            BinOpStackGlobal64(op, global_index) => binop!(stack_global Value64, as_64, exec_binop_64, op, global_index),
            SetLocalConst32(local_index, c) => i32::local_set(&mut self.store.value_stack, &self.cf, *local_index, *c),
            SetLocalConst64(local_index, c) => i64::local_set(&mut self.store.value_stack, &self.cf, *local_index, *c),
            SetLocalConst128(local_index, c) => Value128::local_set(&mut self.store.value_stack, &self.cf, *local_index, Value128(self.func.data.v128_const(*c))),
            StoreLocalLocal32(m, addr_local, value_local) => self.exec_store_local_local::<u32, 4>(*m, *addr_local, *value_local)?,
            StoreLocalLocal64(m, addr_local, value_local) => self.exec_store_local_local::<i64, 8>(*m, *addr_local, *value_local)?,
            StoreLocalLocal128(m, addr_local, value_local) => self.exec_store_local_local::<Value128, 16>(*m, *addr_local, *value_local)?,
            LoadLocal32(m, addr_local) => self.store.value_stack.push(self.exec_load_local_value::<i32, 4>(*m, *addr_local)?)?,
            LoadLocalTee32(m, addr_local, dst_local) => self.exec_load_local_tee::<i32, 4>(*m, *addr_local, *dst_local)?,
            LoadLocalSet32(m, addr_local, dst_local) => self.exec_load_local_set::<i32, 4>(*m, *addr_local, *dst_local)?,
            LoadLocalTee128(m, addr_local, dst_local) => self.exec_load_local_tee::<Value128, 16>(*m, *addr_local, *dst_local)?,
            LoadLocalSet128(m, addr_local, dst_local) => self.exec_load_local_set::<Value128, 16>(*m, *addr_local, *dst_local)?,
            AndConstTee32(c, local_index) => { stack_op!(unary i32, |v| v & *c); stack_op!(local_tee i32, local_index); }
            SubConstTee32(c, local_index) => { stack_op!(unary i32, |v| v.wrapping_sub(*c)); stack_op!(local_tee i32, local_index); }
            AndConstTee64(c, local_index) => { stack_op!(unary i64, |v| v & *c); stack_op!(local_tee i64, local_index); }
            SubConstTee64(c, local_index) => { stack_op!(unary i64, |v| v.wrapping_sub(*c)); stack_op!(local_tee i64, local_index); }
            LocalTee32(local_index) => stack_op!(local_tee Value32, local_index),
            LocalTee64(local_index) => stack_op!(local_tee Value64, local_index),
            LocalTee128(local_index) => stack_op!(local_tee Value128, local_index),
            GlobalGet(global_index) => self.exec_global_get(*global_index)?,
            GlobalSet32(global_index) => self.exec_global_set_32(*global_index),
            GlobalSet64(global_index) => self.exec_global_set::<Value64>(*global_index),
            GlobalSet128(global_index) => self.exec_global_set::<Value128>(*global_index),
            Const32(val) => self.exec_const(*val)?,
            Const64(val) => self.exec_const(*val)?,
            I64Eqz => stack_op!(unary i64 => i32, |v| i32::from(v == 0)),
            I32Eqz => stack_op!(unary i32, |v| i32::from(v == 0)),
            I32Eq => stack_op!(binary i32, |a, b| i32::from(a == b)),
            I64Eq => stack_op!(binary i64 => i32, |a, b| i32::from(a == b)),
            F32Eq => stack_op!(binary f32 => i32, |a, b| i32::from(a == b)),
            F64Eq => stack_op!(binary f64 => i32, |a, b| i32::from(a == b)),
            I32Ne => stack_op!(binary i32, |a, b| i32::from(a != b)),
            I64Ne => stack_op!(binary i64 => i32, |a, b| i32::from(a != b)),
            F32Ne => stack_op!(binary f32 => i32, |a, b| i32::from(a != b)),
            F64Ne => stack_op!(binary f64 => i32, |a, b| i32::from(a != b)),
            I32LtS => stack_op!(binary i32, |a, b| i32::from(a < b)),
            I64LtS => stack_op!(binary i64 => i32, |a, b| i32::from(a < b)),
            I32LtU => stack_op!(binary u32 => i32, |a, b| i32::from(a < b)),
            I64LtU => stack_op!(binary u64 => i32, |a, b| i32::from(a < b)),
            F32Lt => stack_op!(binary f32 => i32, |a, b| i32::from(a < b)),
            F64Lt => stack_op!(binary f64 => i32, |a, b| i32::from(a < b)),
            I32LeS => stack_op!(binary i32, |a, b| i32::from(a <= b)),
            I64LeS => stack_op!(binary i64 => i32, |a, b| i32::from(a <= b)),
            I32LeU => stack_op!(binary u32 => i32, |a, b| i32::from(a <= b)),
            I64LeU => stack_op!(binary u64 => i32, |a, b| i32::from(a <= b)),
            F32Le => stack_op!(binary f32 => i32, |a, b| i32::from(a <= b)),
            F64Le => stack_op!(binary f64 => i32, |a, b| i32::from(a <= b)),
            I32GeS => stack_op!(binary i32, |a, b| i32::from(a >= b)),
            I64GeS => stack_op!(binary i64 => i32, |a, b| i32::from(a >= b)),
            I32GeU => stack_op!(binary u32 => i32, |a, b| i32::from(a >= b)),
            I64GeU => stack_op!(binary u64 => i32, |a, b| i32::from(a >= b)),
            F32Ge => stack_op!(binary f32 => i32, |a, b| i32::from(a >= b)),
            F64Ge => stack_op!(binary f64 => i32, |a, b| i32::from(a >= b)),
            I32GtS => stack_op!(binary i32, |a, b| i32::from(a > b)),
            I64GtS => stack_op!(binary i64 => i32, |a, b| i32::from(a > b)),
            I32GtU => stack_op!(binary u32 => i32, |a, b| i32::from(a > b)),
            I64GtU => stack_op!(binary u64 => i32, |a, b| i32::from(a > b)),
            F32Gt => stack_op!(binary f32 => i32, |a, b| i32::from(a > b)),
            F64Gt => stack_op!(binary f64 => i32, |a, b| i32::from(a > b)),
            I32Add => stack_op!(binary i32, |a, b| a.wrapping_add(b)),
            I64Add => stack_op!(binary i64, |a, b| a.wrapping_add(b)),
            F32Add => stack_op!(binary f32, |a, b| a + b),
            F64Add => stack_op!(binary f64, |a, b| a + b),
            I32Sub => stack_op!(binary i32, |a, b| a.wrapping_sub(b)),
            I64Sub => stack_op!(binary i64, |a, b| a.wrapping_sub(b)),
            F32Sub => stack_op!(binary f32, |a, b| a - b),
            F64Sub => stack_op!(binary f64, |a, b| a - b),
            F32Div => stack_op!(binary f32, |a, b| a / b),
            F64Div => stack_op!(binary f64, |a, b| a / b),
            I32Mul => stack_op!(binary i32, |a, b| a.wrapping_mul(b)),
            I64Mul => stack_op!(binary i64, |a, b| a.wrapping_mul(b)),
            F32Mul => stack_op!(binary f32, |a, b| a * b),
            F64Mul => stack_op!(binary f64, |a, b| a * b),
            I32DivS => stack_op!(binary try i32, |a, b| a.wasm_checked_div(b)),
            I64DivS => stack_op!(binary try i64, |a, b| a.wasm_checked_div(b)),
            I32DivU => stack_op!(binary try u32, |a, b| a.checked_div(b).ok_or_else(trap_0)),
            I64DivU => stack_op!(binary try u64, |a, b| a.checked_div(b).ok_or_else(trap_0)),
            I32RemS => stack_op!(binary try i32, |a, b| a.checked_wrapping_rem(b)),
            I64RemS => stack_op!(binary try i64, |a, b| a.checked_wrapping_rem(b)),
            I32RemU => stack_op!(binary try u32, |a, b| a.checked_wrapping_rem(b)),
            I64RemU => stack_op!(binary try u64, |a, b| a.checked_wrapping_rem(b)),
            I32And => stack_op!(binary i32, |a, b| a & b),
            I64And => stack_op!(binary i64, |a, b| a & b),
            I32Or => stack_op!(binary i32, |a, b| a | b),
            I64Or => stack_op!(binary i64, |a, b| a | b),
            I32Xor => stack_op!(binary i32, |a, b| a ^ b),
            I64Xor => stack_op!(binary i64, |a, b| a ^ b),
            I32Shl => stack_op!(binary i32, |a, b| a.wasm_shl(b)),
            I64Shl => stack_op!(binary i64, |a, b| a.wasm_shl(b)),
            I32ShrS => stack_op!(binary i32, |a, b| a.wasm_shr(b)),
            I64ShrS => stack_op!(binary i64, |a, b| a.wasm_shr(b)),
            I32ShrU => stack_op!(binary u32, |a, b| a.wasm_shr(b)),
            I64ShrU => stack_op!(binary u64, |a, b| a.wasm_shr(b)),
            I32Rotl => stack_op!(binary i32, |a, b| a.wasm_rotl(b)),
            I64Rotl => stack_op!(binary i64, |a, b| a.wasm_rotl(b)),
            I32Rotr => stack_op!(binary i32, |a, b| a.wasm_rotr(b)),
            I64Rotr => stack_op!(binary i64, |a, b| a.wasm_rotr(b)),
            I64Add128 => stack_op!(quaternary_into2 i64 => i64, |a_lo, a_hi, b_lo, b_hi| {
                let lo = a_lo.wrapping_add(b_lo);
                let carry = u64::from((lo as u64) < (a_lo as u64));
                let hi = a_hi.wrapping_add(b_hi).wrapping_add(carry as i64);
                (lo, hi)
            }),
            I64Sub128 => stack_op!(quaternary_into2 i64 => i64, |a_lo, a_hi, b_lo, b_hi| {
                let lo = a_lo.wrapping_sub(b_lo);
                let borrow = u64::from((a_lo as u64) < (b_lo as u64));
                let hi = a_hi.wrapping_sub(b_hi).wrapping_sub(borrow as i64);
                (lo, hi)
            }),
            I64MulWideS => stack_op!(binary_into2 i64 => i64, |a, b| {
                let product = (a as i128).wrapping_mul(b as i128);
                (product as i64, (product >> 64) as i64)
            }),
            I64MulWideU => stack_op!(binary_into2 i64 => i64, |a, b| {
                let product = (a as u64 as u128).wrapping_mul(b as u64 as u128);
                (product as u64 as i64, (product >> 64) as u64 as i64)
            }),
            I32Clz => stack_op!(unary i32, |v| v.leading_zeros() as i32),
            I64Clz => stack_op!(unary i64, |v| i64::from(v.leading_zeros())),
            I32Ctz => stack_op!(unary i32, |v| v.trailing_zeros() as i32),
            I64Ctz => stack_op!(unary i64, |v| i64::from(v.trailing_zeros())),
            I32Popcnt => stack_op!(unary i32, |v| v.count_ones() as i32),
            I64Popcnt => stack_op!(unary i64, |v| i64::from(v.count_ones())),

            // Reference types
            RefFunc(func_idx) => self.exec_const(ValueRef::from_addr(Some(self.module.resolve_func_addr(*func_idx))))?,
            RefNull(_) => self.exec_const(ValueRef::NULL)?,
            RefIsNull => self.exec_ref_is_null()?,
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
            F32ConvertI32S => stack_op!(unary i32 => f32, |v| v as f32),
            F32ConvertI64S => stack_op!(unary i64 => f32, |v| v as f32),
            F64ConvertI32S => stack_op!(unary i32 => f64, |v| f64::from(v)),
            F64ConvertI64S => stack_op!(unary i64 => f64, |v| v as f64),
            F32ConvertI32U => stack_op!(unary u32 => f32, |v| v as f32),
            F32ConvertI64U => stack_op!(unary u64 => f32, |v| v as f32),
            F64ConvertI32U => stack_op!(unary u32 => f64, |v| f64::from(v)),
            F64ConvertI64U => stack_op!(unary u64 => f64, |v| v as f64),

            // Sign-extension operations
            I32Extend8S => stack_op!(unary i32, |v| i32::from(v as i8)),
            I32Extend16S => stack_op!(unary i32, |v| i32::from(v as i16)),
            I64Extend8S => stack_op!(unary i64, |v| i64::from(v as i8)),
            I64Extend16S => stack_op!(unary i64, |v| i64::from(v as i16)),
            I64Extend32S => stack_op!(unary i64, |v| i64::from(v as i32)),
            I64ExtendI32U => stack_op!(unary u32 => i64, |v| i64::from(v)),
            I64ExtendI32S => stack_op!(unary i32 => i64, |v| i64::from(v)),
            I32WrapI64 => stack_op!(unary i64 => i32, |v| v as i32),
            F32DemoteF64 => stack_op!(unary f64 => f32, |v| v as f32),
            F64PromoteF32 => stack_op!(unary f32 => f64, |v| f64::from(v)),
            F32Abs => stack_op!(unary f32, |v| v.abs()),
            F64Abs => stack_op!(unary f64, |v| v.abs()),
            F32Neg => stack_op!(unary f32, |v| -v),
            F64Neg => stack_op!(unary f64, |v| -v),
            F32Ceil => stack_op!(unary f32, |v| v.ceil()),
            F64Ceil => stack_op!(unary f64, |v| v.ceil()),
            F32Floor => stack_op!(unary f32, |v| v.floor()),
            F64Floor => stack_op!(unary f64, |v| v.floor()),
            F32Trunc => stack_op!(unary f32, |v| v.trunc()),
            F64Trunc => stack_op!(unary f64, |v| v.trunc()),
            F32Nearest => stack_op!(unary f32, |v| v.tw_nearest()),
            F64Nearest => stack_op!(unary f64, |v| v.tw_nearest()),
            F32Sqrt => stack_op!(unary f32, |v| v.sqrt()),
            F64Sqrt => stack_op!(unary f64, |v| v.sqrt()),
            F32Min => stack_op!(binary f32, |a, b| a.tw_minimum(b)),
            F64Min => stack_op!(binary f64, |a, b| a.tw_minimum(b)),
            F32Max => stack_op!(binary f32, |a, b| a.tw_maximum(b)),
            F64Max => stack_op!(binary f64, |a, b| a.tw_maximum(b)),
            F32Copysign => stack_op!(binary f32, |a, b| a.copysign(b)),
            F64Copysign => stack_op!(binary f64, |a, b| a.copysign(b)),
            I32TruncF32S => checked_conv_float!(f32, i32, self),
            I32TruncF64S => checked_conv_float!(f64, i32, self),
            I32TruncF32U => checked_conv_float!(f32, u32, i32, self),
            I32TruncF64U => checked_conv_float!(f64, u32, i32, self),
            I64TruncF32S => checked_conv_float!(f32, i64, self),
            I64TruncF64S => checked_conv_float!(f64, i64, self),
            I64TruncF32U => checked_conv_float!(f32, u64, i64, self),
            I64TruncF64U => checked_conv_float!(f64, u64, i64, self),

            // Non-trapping float-to-int conversions
            I32TruncSatF32S => stack_op!(unary f32 => i32, |v| v.trunc() as i32),
            I32TruncSatF32U => stack_op!(unary f32 => u32, |v| v.trunc() as u32),
            I32TruncSatF64S => stack_op!(unary f64 => i32, |v| v.trunc() as i32),
            I32TruncSatF64U => stack_op!(unary f64 => u32, |v| v.trunc() as u32),
            I64TruncSatF32S => stack_op!(unary f32 => i64, |v| v.trunc() as i64),
            I64TruncSatF32U => stack_op!(unary f32 => u64, |v| v.trunc() as u64),
            I64TruncSatF64S => stack_op!(unary f64 => i64, |v| v.trunc() as i64),
            I64TruncSatF64U => stack_op!(unary f64 => u64, |v| v.trunc() as u64),

            // SIMD extension
            V128Not => stack_op!(unary Value128, |v| v.v128_not()),
            V128And => stack_op!(binary Value128, |a, b| a.v128_and(b)),
            V128AndNot => stack_op!(binary Value128, |a, b| a.v128_andnot(b)),
            V128Or => stack_op!(binary Value128, |a, b| a.v128_or(b)),
            V128Xor => stack_op!(binary Value128, |a, b| a.v128_xor(b)),
            V128Bitselect => stack_op!(ternary Value128, |a, b, c| Value128::v128_bitselect(a, b, c)),
            V128AnyTrue => stack_op!(unary Value128 => i32, |v| v.v128_any_true() as i32),
            I8x16Swizzle => stack_op!(binary Value128, |a, s| a.i8x16_swizzle(s)),
            I8x16RelaxedSwizzle => stack_op!(binary Value128, |a, s| a.i8x16_relaxed_swizzle(s)),
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
            Const128(arg) => self.exec_const(Value128(self.func.data.v128_const(*arg)))?,
            I8x16ExtractLaneS(lane) => stack_op!(unary Value128 => i32, |v| v.extract_lane_i8(*lane) as i32),
            I8x16ExtractLaneU(lane) => stack_op!(unary Value128 => i32, |v| v.extract_lane_u8(*lane) as i32),
            I16x8ExtractLaneS(lane) => stack_op!(unary Value128 => i32, |v| v.extract_lane_i16(*lane) as i32),
            I16x8ExtractLaneU(lane) => stack_op!(unary Value128 => i32, |v| v.extract_lane_u16(*lane) as i32),
            I32x4ExtractLane(lane) => stack_op!(unary Value128 => i32, |v| v.extract_lane_i32(*lane)),
            I64x2ExtractLane(lane) => stack_op!(unary Value128 => i64, |v| v.extract_lane_i64(*lane)),
            F32x4ExtractLane(lane) => stack_op!(unary Value128 => f32, |v| v.extract_lane_f32(*lane)),
            F64x2ExtractLane(lane) => stack_op!(unary Value128 => f64, |v| v.extract_lane_f64(*lane)),
            V128Load8Lane(arg, lane) => self.exec_mem_load_lane::<i8, 1>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load16Lane(arg, lane) => self.exec_mem_load_lane::<i16, 2>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load32Lane(arg, lane) => self.exec_mem_load_lane::<i32, 4>(arg.mem_addr(), arg.offset(), *lane)?,
            V128Load64Lane(arg, lane) => self.exec_mem_load_lane::<i64, 8>(arg.mem_addr(), arg.offset(), *lane)?,
            I8x16ReplaceLane(lane) => stack_op!(binary i32, Value128, |value, vec| vec.i8x16_replace_lane(*lane, value as i8)),
            I16x8ReplaceLane(lane) => stack_op!(binary i32, Value128, |value, vec| vec.i16x8_replace_lane(*lane, value as i16)),
            I32x4ReplaceLane(lane) => stack_op!(binary i32, Value128, |value, vec| vec.i32x4_replace_lane(*lane, value)),
            I64x2ReplaceLane(lane) => stack_op!(binary i64, Value128, |value, vec| vec.i64x2_replace_lane(*lane, value)),
            F32x4ReplaceLane(lane) => stack_op!(binary f32, Value128, |value, vec| vec.f32x4_replace_lane(*lane, value)),
            F64x2ReplaceLane(lane) => stack_op!(binary f64, Value128, |value, vec| vec.f64x2_replace_lane(*lane, value)),
            I8x16Splat => stack_op!(unary i32 => Value128, |v| Value128::splat_i8(v as i8)),
            I16x8Splat => stack_op!(unary i32 => Value128, |v| Value128::splat_i16(v as i16)),
            I32x4Splat => stack_op!(unary i32 => Value128, |v| Value128::splat_i32(v)),
            I64x2Splat => stack_op!(unary i64 => Value128, |v| Value128::splat_i64(v)),
            F32x4Splat => stack_op!(unary f32 => Value128, |v| Value128::splat_f32(v)),
            F64x2Splat => stack_op!(unary f64 => Value128, |v| Value128::splat_f64(v)),
            I8x16Eq => stack_op!(binary Value128, |a, b| a.i8x16_eq(b)),
            I16x8Eq => stack_op!(binary Value128, |a, b| a.i16x8_eq(b)),
            I32x4Eq => stack_op!(binary Value128, |a, b| a.i32x4_eq(b)),
            I64x2Eq => stack_op!(binary Value128, |a, b| a.i64x2_eq(b)),
            F32x4Eq => stack_op!(binary Value128, |a, b| a.f32x4_eq(b)),
            F64x2Eq => stack_op!(binary Value128, |a, b| a.f64x2_eq(b)),
            I8x16Ne => stack_op!(binary Value128, |a, b| a.i8x16_ne(b)),
            I16x8Ne => stack_op!(binary Value128, |a, b| a.i16x8_ne(b)),
            I32x4Ne => stack_op!(binary Value128, |a, b| a.i32x4_ne(b)),
            I64x2Ne => stack_op!(binary Value128, |a, b| a.i64x2_ne(b)),
            F32x4Ne => stack_op!(binary Value128, |a, b| a.f32x4_ne(b)),
            F64x2Ne => stack_op!(binary Value128, |a, b| a.f64x2_ne(b)),
            I8x16LtS => stack_op!(binary Value128, |a, b| a.i8x16_lt_s(b)),
            I16x8LtS => stack_op!(binary Value128, |a, b| a.i16x8_lt_s(b)),
            I32x4LtS => stack_op!(binary Value128, |a, b| a.i32x4_lt_s(b)),
            I64x2LtS => stack_op!(binary Value128, |a, b| a.i64x2_lt_s(b)),
            I8x16LtU => stack_op!(binary Value128, |a, b| a.i8x16_lt_u(b)),
            I16x8LtU => stack_op!(binary Value128, |a, b| a.i16x8_lt_u(b)),
            I32x4LtU => stack_op!(binary Value128, |a, b| a.i32x4_lt_u(b)),
            F32x4Lt => stack_op!(binary Value128, |a, b| a.f32x4_lt(b)),
            F64x2Lt => stack_op!(binary Value128, |a, b| a.f64x2_lt(b)),
            F32x4Gt => stack_op!(binary Value128, |a, b| a.f32x4_gt(b)),
            F64x2Gt => stack_op!(binary Value128, |a, b| a.f64x2_gt(b)),
            I8x16GtS => stack_op!(binary Value128, |a, b| a.i8x16_gt_s(b)),
            I16x8GtS => stack_op!(binary Value128, |a, b| a.i16x8_gt_s(b)),
            I32x4GtS => stack_op!(binary Value128, |a, b| a.i32x4_gt_s(b)),
            I64x2GtS => stack_op!(binary Value128, |a, b| a.i64x2_gt_s(b)),
            I64x2LeS => stack_op!(binary Value128, |a, b| a.i64x2_le_s(b)),
            F32x4Le => stack_op!(binary Value128, |a, b| a.f32x4_le(b)),
            F64x2Le => stack_op!(binary Value128, |a, b| a.f64x2_le(b)),
            I8x16GtU => stack_op!(binary Value128, |a, b| a.i8x16_gt_u(b)),
            I16x8GtU => stack_op!(binary Value128, |a, b| a.i16x8_gt_u(b)),
            I32x4GtU => stack_op!(binary Value128, |a, b| a.i32x4_gt_u(b)),
            F32x4Ge => stack_op!(binary Value128, |a, b| a.f32x4_ge(b)),
            F64x2Ge => stack_op!(binary Value128, |a, b| a.f64x2_ge(b)),
            I8x16LeS => stack_op!(binary Value128, |a, b| a.i8x16_le_s(b)),
            I16x8LeS => stack_op!(binary Value128, |a, b| a.i16x8_le_s(b)),
            I32x4LeS => stack_op!(binary Value128, |a, b| a.i32x4_le_s(b)),
            I8x16LeU => stack_op!(binary Value128, |a, b| a.i8x16_le_u(b)),
            I16x8LeU => stack_op!(binary Value128, |a, b| a.i16x8_le_u(b)),
            I32x4LeU => stack_op!(binary Value128, |a, b| a.i32x4_le_u(b)),
            I8x16GeS => stack_op!(binary Value128, |a, b| a.i8x16_ge_s(b)),
            I16x8GeS => stack_op!(binary Value128, |a, b| a.i16x8_ge_s(b)),
            I32x4GeS => stack_op!(binary Value128, |a, b| a.i32x4_ge_s(b)),
            I64x2GeS => stack_op!(binary Value128, |a, b| a.i64x2_ge_s(b)),
            I8x16GeU => stack_op!(binary Value128, |a, b| a.i8x16_ge_u(b)),
            I16x8GeU => stack_op!(binary Value128, |a, b| a.i16x8_ge_u(b)),
            I32x4GeU => stack_op!(binary Value128, |a, b| a.i32x4_ge_u(b)),
            I8x16Abs => stack_op!(unary Value128, |a| a.i8x16_abs()),
            I16x8Abs => stack_op!(unary Value128, |a| a.i16x8_abs()),
            I32x4Abs => stack_op!(unary Value128, |a| a.i32x4_abs()),
            I64x2Abs => stack_op!(unary Value128, |a| a.i64x2_abs()),
            I8x16Neg => stack_op!(unary Value128, |a| a.i8x16_neg()),
            I16x8Neg => stack_op!(unary Value128, |a| a.i16x8_neg()),
            I32x4Neg => stack_op!(unary Value128, |a| a.i32x4_neg()),
            I64x2Neg => stack_op!(unary Value128, |a| a.i64x2_neg()),
            I8x16AllTrue => stack_op!(unary Value128 => i32, |v| v.i8x16_all_true() as i32),
            I16x8AllTrue => stack_op!(unary Value128 => i32, |v| v.i16x8_all_true() as i32),
            I32x4AllTrue => stack_op!(unary Value128 => i32, |v| v.i32x4_all_true() as i32),
            I64x2AllTrue => stack_op!(unary Value128 => i32, |v| v.i64x2_all_true() as i32),
            I8x16Bitmask => stack_op!(unary Value128 => i32, |v| v.i8x16_bitmask() as i32),
            I16x8Bitmask => stack_op!(unary Value128 => i32, |v| v.i16x8_bitmask() as i32),
            I32x4Bitmask => stack_op!(unary Value128 => i32, |v| v.i32x4_bitmask() as i32),
            I64x2Bitmask => stack_op!(unary Value128 => i32, |v| v.i64x2_bitmask() as i32),
            I8x16Shl => stack_op!(binary i32, Value128, |a, b| b.i8x16_shl(a as u32)),
            I16x8Shl => stack_op!(binary i32, Value128, |a, b| b.i16x8_shl(a as u32)),
            I32x4Shl => stack_op!(binary i32, Value128, |a, b| b.i32x4_shl(a as u32)),
            I64x2Shl => stack_op!(binary i32, Value128, |a, b| b.i64x2_shl(a as u32)),
            I8x16ShrS => stack_op!(binary i32, Value128, |a, b| b.i8x16_shr_s(a as u32)),
            I16x8ShrS => stack_op!(binary i32, Value128, |a, b| b.i16x8_shr_s(a as u32)),
            I32x4ShrS => stack_op!(binary i32, Value128, |a, b| b.i32x4_shr_s(a as u32)),
            I64x2ShrS => stack_op!(binary i32, Value128, |a, b| b.i64x2_shr_s(a as u32)),
            I8x16ShrU => stack_op!(binary i32, Value128, |a, b| b.i8x16_shr_u(a as u32)),
            I16x8ShrU => stack_op!(binary i32, Value128, |a, b| b.i16x8_shr_u(a as u32)),
            I32x4ShrU => stack_op!(binary i32, Value128, |a, b| b.i32x4_shr_u(a as u32)),
            I64x2ShrU => stack_op!(binary i32, Value128, |a, b| b.i64x2_shr_u(a as u32)),
            I8x16Add => stack_op!(binary Value128, |a, b| a.i8x16_add(b)),
            I16x8Add => stack_op!(binary Value128, |a, b| a.i16x8_add(b)),
            I32x4Add => stack_op!(binary Value128, |a, b| a.i32x4_add(b)),
            I64x2Add => stack_op!(binary Value128, |a, b| a.i64x2_add(b)),
            I8x16Sub => stack_op!(binary Value128, |a, b| a.i8x16_sub(b)),
            I16x8Sub => stack_op!(binary Value128, |a, b| a.i16x8_sub(b)),
            I32x4Sub => stack_op!(binary Value128, |a, b| a.i32x4_sub(b)),
            I64x2Sub => stack_op!(binary Value128, |a, b| a.i64x2_sub(b)),
            I8x16MinS => stack_op!(binary Value128, |a, b| a.i8x16_min_s(b)),
            I16x8MinS => stack_op!(binary Value128, |a, b| a.i16x8_min_s(b)),
            I32x4MinS => stack_op!(binary Value128, |a, b| a.i32x4_min_s(b)),
            I8x16MinU => stack_op!(binary Value128, |a, b| a.i8x16_min_u(b)),
            I16x8MinU => stack_op!(binary Value128, |a, b| a.i16x8_min_u(b)),
            I32x4MinU => stack_op!(binary Value128, |a, b| a.i32x4_min_u(b)),
            I8x16MaxS => stack_op!(binary Value128, |a, b| a.i8x16_max_s(b)),
            I16x8MaxS => stack_op!(binary Value128, |a, b| a.i16x8_max_s(b)),
            I32x4MaxS => stack_op!(binary Value128, |a, b| a.i32x4_max_s(b)),
            I8x16MaxU => stack_op!(binary Value128, |a, b| a.i8x16_max_u(b)),
            I16x8MaxU => stack_op!(binary Value128, |a, b| a.i16x8_max_u(b)),
            I32x4MaxU => stack_op!(binary Value128, |a, b| a.i32x4_max_u(b)),
            I64x2Mul => stack_op!(binary Value128, |a, b| a.i64x2_mul(b)),
            I16x8Mul => stack_op!(binary Value128, |a, b| a.i16x8_mul(b)),
            I32x4Mul => stack_op!(binary Value128, |a, b| a.i32x4_mul(b)),
            I8x16NarrowI16x8S => stack_op!(binary Value128, |a, b| Value128::i8x16_narrow_i16x8_s(a, b)),
            I8x16NarrowI16x8U => stack_op!(binary Value128, |a, b| Value128::i8x16_narrow_i16x8_u(a, b)),
            I16x8NarrowI32x4S => stack_op!(binary Value128, |a, b| Value128::i16x8_narrow_i32x4_s(a, b)),
            I16x8NarrowI32x4U => stack_op!(binary Value128, |a, b| Value128::i16x8_narrow_i32x4_u(a, b)),
            I8x16AddSatS => stack_op!(binary Value128, |a, b| a.i8x16_add_sat_s(b)),
            I16x8AddSatS => stack_op!(binary Value128, |a, b| a.i16x8_add_sat_s(b)),
            I8x16AddSatU => stack_op!(binary Value128, |a, b| a.i8x16_add_sat_u(b)),
            I16x8AddSatU => stack_op!(binary Value128, |a, b| a.i16x8_add_sat_u(b)),
            I8x16SubSatS => stack_op!(binary Value128, |a, b| a.i8x16_sub_sat_s(b)),
            I16x8SubSatS => stack_op!(binary Value128, |a, b| a.i16x8_sub_sat_s(b)),
            I8x16SubSatU => stack_op!(binary Value128, |a, b| a.i8x16_sub_sat_u(b)),
            I16x8SubSatU => stack_op!(binary Value128, |a, b| a.i16x8_sub_sat_u(b)),
            I8x16AvgrU => stack_op!(binary Value128, |a, b| a.i8x16_avgr_u(b)),
            I16x8AvgrU => stack_op!(binary Value128, |a, b| a.i16x8_avgr_u(b)),
            I16x8ExtAddPairwiseI8x16S => stack_op!(unary Value128, |a| a.i16x8_extadd_pairwise_i8x16_s()),
            I16x8ExtAddPairwiseI8x16U => stack_op!(unary Value128, |a| a.i16x8_extadd_pairwise_i8x16_u()),
            I32x4ExtAddPairwiseI16x8S => stack_op!(unary Value128, |a| a.i32x4_extadd_pairwise_i16x8_s()),
            I32x4ExtAddPairwiseI16x8U => stack_op!(unary Value128, |a| a.i32x4_extadd_pairwise_i16x8_u()),
            I16x8ExtMulLowI8x16S => stack_op!(binary Value128, |a, b| a.i16x8_extmul_low_i8x16_s(b)),
            I16x8ExtMulLowI8x16U => stack_op!(binary Value128, |a, b| a.i16x8_extmul_low_i8x16_u(b)),
            I16x8ExtMulHighI8x16S => stack_op!(binary Value128, |a, b| a.i16x8_extmul_high_i8x16_s(b)),
            I16x8ExtMulHighI8x16U => stack_op!(binary Value128, |a, b| a.i16x8_extmul_high_i8x16_u(b)),
            I32x4ExtMulLowI16x8S => stack_op!(binary Value128, |a, b| a.i32x4_extmul_low_i16x8_s(b)),
            I32x4ExtMulLowI16x8U => stack_op!(binary Value128, |a, b| a.i32x4_extmul_low_i16x8_u(b)),
            I32x4ExtMulHighI16x8S => stack_op!(binary Value128, |a, b| a.i32x4_extmul_high_i16x8_s(b)),
            I32x4ExtMulHighI16x8U => stack_op!(binary Value128, |a, b| a.i32x4_extmul_high_i16x8_u(b)),
            I64x2ExtMulLowI32x4S => stack_op!(binary Value128, |a, b| a.i64x2_extmul_low_i32x4_s(b)),
            I64x2ExtMulLowI32x4U => stack_op!(binary Value128, |a, b| a.i64x2_extmul_low_i32x4_u(b)),
            I64x2ExtMulHighI32x4S => stack_op!(binary Value128, |a, b| a.i64x2_extmul_high_i32x4_s(b)),
            I64x2ExtMulHighI32x4U => stack_op!(binary Value128, |a, b| a.i64x2_extmul_high_i32x4_u(b)),
            I16x8ExtendLowI8x16S => stack_op!(unary Value128, |a| a.i16x8_extend_low_i8x16_s()),
            I16x8ExtendLowI8x16U => stack_op!(unary Value128, |a| a.i16x8_extend_low_i8x16_u()),
            I16x8ExtendHighI8x16S => stack_op!(unary Value128, |a| a.i16x8_extend_high_i8x16_s()),
            I16x8ExtendHighI8x16U => stack_op!(unary Value128, |a| a.i16x8_extend_high_i8x16_u()),
            I32x4ExtendLowI16x8S => stack_op!(unary Value128, |a| a.i32x4_extend_low_i16x8_s()),
            I32x4ExtendLowI16x8U => stack_op!(unary Value128, |a| a.i32x4_extend_low_i16x8_u()),
            I32x4ExtendHighI16x8S => stack_op!(unary Value128, |a| a.i32x4_extend_high_i16x8_s()),
            I32x4ExtendHighI16x8U => stack_op!(unary Value128, |a| a.i32x4_extend_high_i16x8_u()),
            I64x2ExtendLowI32x4S => stack_op!(unary Value128, |a| a.i64x2_extend_low_i32x4_s()),
            I64x2ExtendLowI32x4U => stack_op!(unary Value128, |a| a.i64x2_extend_low_i32x4_u()),
            I64x2ExtendHighI32x4S => stack_op!(unary Value128, |a| a.i64x2_extend_high_i32x4_s()),
            I64x2ExtendHighI32x4U => stack_op!(unary Value128, |a| a.i64x2_extend_high_i32x4_u()),
            I8x16Popcnt => stack_op!(unary Value128, |v| v.i8x16_popcnt()),
            I8x16Shuffle(idx) => {
                let Some(mask) = self.func.data.v128_constants.get(*idx as usize) else {
                    cold_path();
                    unreachable!("invalid i128 constant index")
                };
                stack_op!(binary Value128, |a, b| Value128::i8x16_shuffle(a, b, Value128(*mask)))
            },
            I16x8Q15MulrSatS => stack_op!(binary Value128, |a, b| a.i16x8_q15mulr_sat_s(b)),
            I32x4DotI16x8S => stack_op!(binary Value128, |a, b| a.i32x4_dot_i16x8_s(b)),
            I8x16RelaxedLaneselect => stack_op!(ternary Value128, |a, b, c| Value128::i8x16_relaxed_laneselect(a, b, c)),
            I16x8RelaxedLaneselect => stack_op!(ternary Value128, |a, b, c| Value128::i16x8_relaxed_laneselect(a, b, c)),
            I32x4RelaxedLaneselect => stack_op!(ternary Value128, |a, b, c| Value128::i32x4_relaxed_laneselect(a, b, c)),
            I64x2RelaxedLaneselect => stack_op!(ternary Value128, |a, b, c| Value128::i64x2_relaxed_laneselect(a, b, c)),
            I16x8RelaxedQ15mulrS => stack_op!(binary Value128, |a, b| a.i16x8_relaxed_q15mulr_s(b)),
            I16x8RelaxedDotI8x16I7x16S => stack_op!(binary Value128, |a, b| a.i16x8_relaxed_dot_i8x16_i7x16_s(b)),
            I32x4RelaxedDotI8x16I7x16AddS => stack_op!(ternary Value128, |a, b, c| a.i32x4_relaxed_dot_i8x16_i7x16_add_s(b, c)),
            F32x4Ceil => stack_op!(unary Value128, |v| v.f32x4_ceil()),
            F64x2Ceil => stack_op!(unary Value128, |v| v.f64x2_ceil()),
            F32x4Floor => stack_op!(unary Value128, |v| v.f32x4_floor()),
            F64x2Floor => stack_op!(unary Value128, |v| v.f64x2_floor()),
            F32x4Trunc => stack_op!(unary Value128, |v| v.f32x4_trunc()),
            F64x2Trunc => stack_op!(unary Value128, |v| v.f64x2_trunc()),
            F32x4Nearest => stack_op!(unary Value128, |v| v.f32x4_nearest()),
            F64x2Nearest => stack_op!(unary Value128, |v| v.f64x2_nearest()),
            F32x4Abs => stack_op!(unary Value128, |v| v.f32x4_abs()),
            F64x2Abs => stack_op!(unary Value128, |v| v.f64x2_abs()),
            F32x4Neg => stack_op!(unary Value128, |v| v.f32x4_neg()),
            F64x2Neg => stack_op!(unary Value128, |v| v.f64x2_neg()),
            F32x4Sqrt => stack_op!(unary Value128, |v| v.f32x4_sqrt()),
            F64x2Sqrt => stack_op!(unary Value128, |v| v.f64x2_sqrt()),
            F32x4Add => stack_op!(binary Value128, |a, b| a.f32x4_add(b)),
            F64x2Add => stack_op!(binary Value128, |a, b| a.f64x2_add(b)),
            F32x4Sub => stack_op!(binary Value128, |a, b| a.f32x4_sub(b)),
            F64x2Sub => stack_op!(binary Value128, |a, b| a.f64x2_sub(b)),
            F32x4Mul => stack_op!(binary Value128, |a, b| a.f32x4_mul(b)),
            F64x2Mul => stack_op!(binary Value128, |a, b| a.f64x2_mul(b)),
            F32x4Div => stack_op!(binary Value128, |a, b| a.f32x4_div(b)),
            F64x2Div => stack_op!(binary Value128, |a, b| a.f64x2_div(b)),
            F32x4Min => stack_op!(binary Value128, |a, b| a.f32x4_min(b)),
            F64x2Min => stack_op!(binary Value128, |a, b| a.f64x2_min(b)),
            F32x4Max => stack_op!(binary Value128, |a, b| a.f32x4_max(b)),
            F64x2Max => stack_op!(binary Value128, |a, b| a.f64x2_max(b)),
            F32x4PMin => stack_op!(binary Value128, |a, b| a.f32x4_pmin(b)),
            F32x4PMax => stack_op!(binary Value128, |a, b| a.f32x4_pmax(b)),
            F64x2PMin => stack_op!(binary Value128, |a, b| a.f64x2_pmin(b)),
            F64x2PMax => stack_op!(binary Value128, |a, b| a.f64x2_pmax(b)),
            F32x4RelaxedMadd => stack_op!(ternary Value128, |a, b, c| a.f32x4_relaxed_madd(b, c)),
            F32x4RelaxedNmadd => stack_op!(ternary Value128, |a, b, c| a.f32x4_relaxed_nmadd(b, c)),
            F64x2RelaxedMadd => stack_op!(ternary Value128, |a, b, c| a.f64x2_relaxed_madd(b, c)),
            F64x2RelaxedNmadd => stack_op!(ternary Value128, |a, b, c| a.f64x2_relaxed_nmadd(b, c)),
            F32x4RelaxedMin => stack_op!(binary Value128, |a, b| a.f32x4_relaxed_min(b)),
            F32x4RelaxedMax => stack_op!(binary Value128, |a, b| a.f32x4_relaxed_max(b)),
            F64x2RelaxedMin => stack_op!(binary Value128, |a, b| a.f64x2_relaxed_min(b)),
            F64x2RelaxedMax => stack_op!(binary Value128, |a, b| a.f64x2_relaxed_max(b)),
            I32x4TruncSatF32x4S => stack_op!(unary Value128, |v| v.i32x4_trunc_sat_f32x4_s()),
            I32x4TruncSatF32x4U => stack_op!(unary Value128, |v| v.i32x4_trunc_sat_f32x4_u()),
            F32x4ConvertI32x4S => stack_op!(unary Value128, |v| v.f32x4_convert_i32x4_s()),
            F32x4ConvertI32x4U => stack_op!(unary Value128, |v| v.f32x4_convert_i32x4_u()),
            F64x2ConvertLowI32x4S => stack_op!(unary Value128, |v| v.f64x2_convert_low_i32x4_s()),
            F64x2ConvertLowI32x4U => stack_op!(unary Value128, |v| v.f64x2_convert_low_i32x4_u()),
            F32x4DemoteF64x2Zero => stack_op!(unary Value128, |v| v.f32x4_demote_f64x2_zero()),
            F64x2PromoteLowF32x4 => stack_op!(unary Value128, |v| v.f64x2_promote_low_f32x4()),
            I32x4TruncSatF64x2SZero => stack_op!(unary Value128, |v| v.i32x4_trunc_sat_f64x2_s_zero()),
            I32x4TruncSatF64x2UZero => stack_op!(unary Value128, |v| v.i32x4_trunc_sat_f64x2_u_zero()),
            I32x4RelaxedTruncF32x4S => stack_op!(unary Value128, |v| v.i32x4_relaxed_trunc_f32x4_s()),
            I32x4RelaxedTruncF32x4U => stack_op!(unary Value128, |v| v.i32x4_relaxed_trunc_f32x4_u()),
            I32x4RelaxedTruncF64x2SZero => stack_op!(unary Value128, |v| v.i32x4_relaxed_trunc_f64x2_s_zero()),
            I32x4RelaxedTruncF64x2UZero => stack_op!(unary Value128, |v| v.i32x4_relaxed_trunc_f64x2_u_zero()),
        };

        self.cf.incr_instr_ptr();

        Ok(None)
    }

    #[inline(always)]
    fn exec_jump(&mut self, ip: u32) {
        self.cf.instr_ptr = ip;
    }

    #[inline(always)]
    fn jump_if(&mut self, condition: bool, ip: u32) -> bool {
        if condition {
            self.cf.instr_ptr = ip;
        }
        condition
    }

    #[inline(always)]
    fn exec_jump_local_zero_32(&mut self, target_ip: u32, local: LocalAddr) -> bool {
        self.jump_if(Value32::local_get(&self.store.value_stack, &self.cf, local) == 0, target_ip)
    }

    #[inline(always)]
    fn exec_jump_local_non_zero_32(&mut self, target_ip: u32, local: LocalAddr) -> bool {
        self.jump_if(Value32::local_get(&self.store.value_stack, &self.cf, local) != 0, target_ip)
    }

    #[inline(always)]
    fn exec_jump_local_zero_64(&mut self, target_ip: u32, local: LocalAddr) -> bool {
        self.jump_if(Value64::local_get(&self.store.value_stack, &self.cf, local) == 0, target_ip)
    }

    #[inline(always)]
    fn exec_jump_local_non_zero_64(&mut self, target_ip: u32, local: LocalAddr) -> bool {
        self.jump_if(Value64::local_get(&self.store.value_stack, &self.cf, local) != 0, target_ip)
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_const_32(&mut self, target_ip: u32, imm: i32, op: CmpOp) -> bool {
        let condition = cmp_i32(<i32>::stack_pop(&mut self.store.value_stack), imm, op);
        self.jump_if(condition, target_ip)
    }

    #[inline(always)]
    fn exec_jump_cmp_stack_const_64(&mut self, target_ip: u32, imm: i64, op: CmpOp) -> bool {
        let condition = cmp_i64(<i64>::stack_pop(&mut self.store.value_stack), imm, op);
        self.jump_if(condition, target_ip)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_const_32(&mut self, target_ip: u32, local: LocalAddr, imm: i32, op: CmpOp) -> bool {
        self.jump_if(cmp_i32(i32::local_get(&self.store.value_stack, &self.cf, local), imm, op), target_ip)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_const_64(&mut self, target_ip: u32, local: LocalAddr, imm: i32, op: CmpOp) -> bool {
        self.jump_if(cmp_i64(i64::local_get(&self.store.value_stack, &self.cf, local), i64::from(imm), op), target_ip)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_local_32(&mut self, target_ip: u32, left: LocalAddr, right: LocalAddr, op: CmpOp) -> bool {
        let lhs = i32::local_get(&self.store.value_stack, &self.cf, left);
        let rhs = i32::local_get(&self.store.value_stack, &self.cf, right);
        self.jump_if(cmp_i32(lhs, rhs, op), target_ip)
    }

    #[inline(always)]
    fn exec_jump_cmp_local_local_64(&mut self, target_ip: u32, left: LocalAddr, right: LocalAddr, op: CmpOp) -> bool {
        let lhs = i64::local_get(&self.store.value_stack, &self.cf, left);
        let rhs = i64::local_get(&self.store.value_stack, &self.cf, right);
        self.jump_if(cmp_i64(lhs, rhs, op), target_ip)
    }

    fn exec_branch_table(&mut self, default_ip: u32, start: u32, len: u32) {
        let idx = <i32>::stack_pop(&mut self.store.value_stack);
        let target_ip = if idx >= 0 && (idx as u32) < len {
            self.func.data.branch_table_targets.get((start + idx as u32) as usize).copied().unwrap_or(default_ip)
        } else {
            default_ip
        };

        self.cf.instr_ptr = target_ip;
    }

    fn exec_call(&mut self, wasm_func: WasmFunctionInstance, func_addr: FuncAddr) -> Result<(), Trap> {
        if !Arc::ptr_eq(&self.func, &wasm_func.func) {
            self.func = wasm_func.func.clone();
        }

        let Ok(locals_base) = self.store.value_stack.enter_locals(&wasm_func.func.params, &wasm_func.func.locals)
        else {
            cold_path();
            return Err(Trap::CallStackOverflow);
        };

        self.store.call_stack.push(self.cf)?;
        self.cf = CallFrame::new(func_addr, locals_base, wasm_func.func.locals);
        if wasm_func.owner != self.module.idx() {
            self.module = self.store.get_module_instance_internal(wasm_func.owner);
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
            cold_path();
            return Err(Trap::CallStackOverflow);
        };
        self.cf = CallFrame::new(func_addr, locals_base, wasm_func.func.locals);
        if wasm_func.owner != self.module.idx() {
            self.module = self.store.get_module_instance_internal(wasm_func.owner);
        }

        Ok(())
    }

    fn exec_call_host(&mut self, host_func: Rc<HostFunction>) -> Result<(), Trap> {
        let params = self.store.value_stack.pop_types(host_func.ty.params()).collect::<Box<_>>();
        let res = match host_func.call(FuncContext { store: self.store, module_addr: self.module.idx() }, &params) {
            Ok(res) => res,
            Err(err) => {
                cold_path();
                return Err(Trap::HostFunction(Box::new(err)));
            }
        };

        self.store.value_stack.extend_from_wasmvalues(&res)?;
        self.cf.incr_instr_ptr();
        Ok(())
    }

    fn exec_call_direct(&mut self, v: u32) -> Result<(), Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        match self.store.state.get_func(addr) {
            crate::FunctionInstance::Wasm(wasm_func) => self.exec_call(wasm_func.clone(), addr),
            crate::FunctionInstance::Host(host_func) => self.exec_call_host(host_func.clone()),
        }
    }

    fn exec_return_call_direct(&mut self, v: u32) -> Result<(), Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        match self.store.state.get_func(addr) {
            crate::FunctionInstance::Wasm(wasm_func) => self.exec_return_call(wasm_func.clone(), addr),
            crate::FunctionInstance::Host(host_func) => self.exec_call_host(host_func.clone()),
        }
    }

    fn exec_call_self(&mut self) -> Result<(), Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);

        self.store.call_stack.push(self.cf)?;
        let Ok(locals_base) = self.store.value_stack.enter_locals(&self.func.params, &self.func.locals) else {
            cold_path();
            return Err(Trap::CallStackOverflow);
        };
        self.cf = CallFrame::new(self.cf.func_addr, locals_base, self.func.locals);

        Ok(())
    }

    fn exec_return_call_self(&mut self) -> Result<(), Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);

        self.store.value_stack.truncate_keep_counts(self.cf.locals_base, self.func.params);
        let Ok(locals_base) = self.store.value_stack.enter_locals(&self.func.params, &self.func.locals) else {
            cold_path();
            return Err(Trap::CallStackOverflow);
        };
        self.cf = CallFrame::new(self.cf.func_addr, locals_base, self.func.locals);
        Ok(())
    }

    fn exec_call_indirect<const IS_RETURN_CALL: bool>(&mut self, type_addr: u32, table_addr: u32) -> Result<(), Trap> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);

        // verify that the table is of the right type, this should be validated by the parser already
        let table_idx: u32 = <i32>::stack_pop(&mut self.store.value_stack) as u32;
        let table = self.store.state.get_table(self.module.resolve_table_addr(table_addr));
        debug_assert!(table.kind.element_type == WasmType::RefFunc, "table is not of type funcref");

        let Ok(table) = table.get(table_idx) else {
            cold_path();
            return Err(Trap::UndefinedElement { index: table_idx as usize });
        };

        let Some(func_ref) = table.addr() else {
            cold_path();
            return Err(Trap::UninitializedElement { index: table_idx as usize });
        };

        let call_ty = self.module.func_ty(type_addr);
        match self.store.state.get_func(func_ref) {
            crate::FunctionInstance::Wasm(wasm_func) => {
                if wasm_func.ty() != call_ty {
                    cold_path();
                    return Err(Trap::IndirectCallTypeMismatch {
                        actual: wasm_func.ty().clone(),
                        expected: call_ty.clone(),
                    });
                }

                match IS_RETURN_CALL {
                    true => self.exec_return_call(wasm_func.clone(), func_ref),
                    false => self.exec_call(wasm_func.clone(), func_ref),
                }
            }
            crate::FunctionInstance::Host(host_func) => {
                if host_func.ty != *call_ty {
                    cold_path();
                    return Err(Trap::IndirectCallTypeMismatch {
                        actual: host_func.ty.clone(),
                        expected: call_ty.clone(),
                    });
                }

                self.exec_call_host(host_func.clone())
            }
        }
    }

    fn exec_return(&mut self) -> bool {
        self.store.value_stack.truncate_keep_counts(self.cf.locals_base, self.func.results);
        let Some(cf) = self.store.call_stack.pop() else {
            cold_path();
            return true;
        };

        if cf.func_addr != self.cf.func_addr {
            let wasm_func = self.store.state.get_wasm_func(cf.func_addr);
            self.func = wasm_func.func.clone();
            if wasm_func.owner != self.module.idx() {
                self.module = self.store.get_module_instance_internal(wasm_func.owner);
            }
        }

        self.cf = cf;
        false
    }

    fn exec_store_local_local<T: InternalValue + MemValue<N>, const N: usize>(
        &mut self,
        memarg: MemoryArg,
        addr_local: u8,
        value_local: u8,
    ) -> Result<(), Trap> {
        let addr = u64::from(u32::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local)));
        let value = T::local_get(&self.store.value_stack, &self.cf, u16::from(value_local)).to_mem_bytes();
        let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(memarg.mem_addr()));
        mem.store(addr, memarg.offset(), value)?;
        Ok(())
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
        let addr = i32::stack_pop(&mut self.store.value_stack);
        let fma = acc + lhs * rhs;
        let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(m.mem_addr()));
        mem.store(addr as u32 as u64, m.offset(), fma.to_mem_bytes())?;
        Ok(())
    }

    #[inline(always)]
    fn exec_binop_acc_local<T, M, A>(&mut self, acc: LocalAddr, mul: M, add: A)
    where
        T: InternalValue,
        M: Fn(T, T) -> T,
        A: Fn(T, T) -> T,
    {
        let rhs = T::stack_pop(&mut self.store.value_stack);
        let lhs = T::stack_pop(&mut self.store.value_stack);
        T::local_update(&mut self.store.value_stack, &self.cf, acc, |v| add(mul(lhs, rhs), v));
    }

    fn exec_load_local_value<T: MemValue<N>, const N: usize>(
        &self,
        memarg: MemoryArg,
        addr_local: u8,
    ) -> Result<T, Trap> {
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(memarg.mem_addr()));
        let addr = u64::from(u32::local_get(&self.store.value_stack, &self.cf, u16::from(addr_local)));
        let bytes = mem.load(addr, memarg.offset())?;
        Ok(T::from_mem_bytes(bytes))
    }

    fn exec_load_local_tee<T: InternalValue + MemValue<N>, const N: usize>(
        &mut self,
        memarg: MemoryArg,
        addr_local: u8,
        dst_local: u8,
    ) -> Result<(), Trap> {
        let value = self.exec_load_local_value::<T, N>(memarg, addr_local)?;
        T::local_set(&mut self.store.value_stack, &self.cf, u16::from(dst_local), value);
        self.store.value_stack.push(value)?;
        Ok(())
    }

    fn exec_load_local_set<T: InternalValue + MemValue<N>, const N: usize>(
        &mut self,
        memarg: MemoryArg,
        addr_local: u8,
        dst_local: u8,
    ) -> Result<(), Trap> {
        let value = self.exec_load_local_value::<T, N>(memarg, addr_local)?;
        T::local_set(&mut self.store.value_stack, &self.cf, u16::from(dst_local), value);
        Ok(())
    }

    fn exec_global_get(&mut self, global_index: u32) -> Result<(), Trap> {
        self.store.value_stack.push_dyn(self.store.state.get_global_val(self.module.resolve_global_addr(global_index)))
    }

    fn exec_global_set<T: InternalValue>(&mut self, global_index: u32) {
        let global_addr = self.module.resolve_global_addr(global_index);
        let value = <T>::stack_pop(&mut self.store.value_stack).into();
        self.store.state.set_global_val(global_addr, value);
    }

    fn exec_global_set_32(&mut self, global_index: u32) {
        let global_addr = self.module.resolve_global_addr(global_index);
        let raw = <Value32>::stack_pop(&mut self.store.value_stack);
        let value = match self.store.state.get_global(global_addr).ty.ty {
            WasmType::I32 | WasmType::F32 => TinyWasmValue::Value32(raw),
            WasmType::RefExtern | WasmType::RefFunc => TinyWasmValue::ValueRef(ValueRef::from_raw(raw)),
            WasmType::I64 | WasmType::F64 | WasmType::V128 => unreachable!("invalid global.set.32 target type"),
        };
        self.store.state.set_global_val(global_addr, value);
    }

    fn exec_const<T: InternalValue>(&mut self, val: T) -> Result<(), Trap> {
        self.store.value_stack.push(val)
    }
    fn exec_ref_is_null(&mut self) -> Result<(), Trap> {
        let is_null = i32::from(<ValueRef>::stack_pop(&mut self.store.value_stack).is_null());
        self.store.value_stack.push::<i32>(is_null)
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
        let size = i32::stack_pop(&mut self.store.value_stack);
        let src = i32::stack_pop(&mut self.store.value_stack);
        let dst = i32::stack_pop(&mut self.store.value_stack);
        let dst_mem_addr = self.module.resolve_mem_addr(dst_mem);

        if dst_mem == src_mem {
            // copy within the same memory
            let mem = self.store.state.get_mem_mut(dst_mem_addr);
            mem.copy_within(dst as usize, src as usize, size as usize)?;
        } else {
            // copy between two memories
            let src_mem_addr = self.module.resolve_mem_addr(src_mem);
            let (dst_memory, src_memory) = self.store.state.get_mems_mut(dst_mem_addr, src_mem_addr);
            dst_memory.copy_from_memory(dst as usize, src_memory, src as usize, size as usize)?;
        }
        Ok(())
    }
    fn exec_memory_fill(&mut self, addr: u32) -> Result<(), Trap> {
        let size = i32::stack_pop(&mut self.store.value_stack);
        let val = i32::stack_pop(&mut self.store.value_stack);
        let dst = i32::stack_pop(&mut self.store.value_stack);
        self.exec_memory_fill_impl(addr, dst, val as u8, size)
    }

    fn exec_memory_fill_imm(&mut self, addr: u32, val: u8, size: i32) -> Result<(), Trap> {
        let dst = i32::stack_pop(&mut self.store.value_stack);
        self.exec_memory_fill_impl(addr, dst, val, size)
    }

    fn exec_memory_fill_impl(&mut self, addr: u32, dst: i32, val: u8, size: i32) -> Result<(), Trap> {
        let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(addr));
        if mem.inner.fill(dst as usize, size as usize, val).is_none() {
            cold_path();
            return Err(Trap::MemoryOutOfBounds {
                offset: dst as usize,
                len: size as usize,
                max: self.store.state.get_mem(self.module.resolve_mem_addr(addr)).inner.len(),
            });
        }
        Ok(())
    }

    fn exec_memory_init(&mut self, data_index: u32, mem_index: u32) -> Result<(), Trap> {
        let size = i32::stack_pop(&mut self.store.value_stack);
        let offset = i32::stack_pop(&mut self.store.value_stack);
        let dst = i32::stack_pop(&mut self.store.value_stack);

        let data_addr = self.module.resolve_data_addr(data_index) as usize;
        let Some(data) = self.store.state.data.get(data_addr) else {
            unreachable!("data segment not found, should have been validated by the parser")
        };

        let mem_addr = self.module.resolve_mem_addr(mem_index) as usize;
        let Some(mem) = self.store.state.memories.get_mut(mem_addr) else {
            unreachable!("memory not found, should have been validated by the parser")
        };

        let data_len = data.data.as_ref().map_or(0, |d| d.len());
        if ((size + offset) as usize > data_len) || ((dst + size) as usize > mem.inner.len()) {
            cold_path();
            return Err(Trap::MemoryOutOfBounds { offset: offset as usize, len: size as usize, max: data_len });
        }

        if size == 0 {
            return Ok(());
        }

        let Some(data) = &data.data else {
            cold_path();
            return Err(Trap::MemoryOutOfBounds { offset: 0, len: 0, max: 0 });
        };

        if mem.inner.write_all(dst as usize, &data[offset as usize..((offset + size) as usize)]).is_none() {
            cold_path();
            return Err(Trap::MemoryOutOfBounds { offset: dst as usize, len: size as usize, max: mem.inner.len() });
        }
        Ok(())
    }
    fn exec_table_copy(&mut self, dst_table: u32, src_table: u32) -> Result<(), Trap> {
        let size = i32::stack_pop(&mut self.store.value_stack);
        let src = i32::stack_pop(&mut self.store.value_stack);
        let dst = i32::stack_pop(&mut self.store.value_stack);
        let dst_table_addr = self.module.resolve_table_addr(dst_table);

        if dst_table == src_table {
            // copy within the same table
            self.store.state.get_table_mut(dst_table_addr).copy_within(dst as usize, src as usize, size as usize)
        } else {
            // copy between two tables
            let src_table_addr = self.module.resolve_table_addr(src_table);
            let (dst_table_ref, src_table_ref) = self.store.state.get_tables_mut(dst_table_addr, src_table_addr);
            dst_table_ref.copy_from_slice(dst as usize, src_table_ref.load(src as usize, size as usize)?)
        }
    }

    fn exec_mem_load_lane<LOAD: MemValue<LOAD_SIZE>, const LOAD_SIZE: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        lane: u8,
    ) -> Result<(), Trap> {
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(mem_addr));
        let base = match mem.is_64bit() {
            true => <i64>::stack_pop(&mut self.store.value_stack) as u64,
            false => <i32>::stack_pop(&mut self.store.value_stack) as u32 as u64,
        };
        let val = match mem.load::<LOAD_SIZE>(base, offset) {
            Ok(val) => val,
            Err(e) => {
                cold_path();
                return Err(e);
            }
        };
        let offset = lane as usize * LOAD_SIZE;
        let mut imm = <Value128>::stack_pop(&mut self.store.value_stack).to_mem_bytes();
        imm[offset..offset + LOAD_SIZE].copy_from_slice(&val);
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
        let base = match mem.is_64bit() {
            true => <i64>::stack_pop(&mut self.store.value_stack) as u64,
            false => <i32>::stack_pop(&mut self.store.value_stack) as u32 as u64,
        };

        match LOAD::load(&*mem.inner, base, offset) {
            Ok(val) => {
                self.store.value_stack.push(cast(val))?;
                Ok(())
            }
            Err(e) => {
                cold_path();
                Err(e)
            }
        }
    }

    fn exec_mem_store_lane<U: MemValue<N> + Copy, const N: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        lane: u8,
    ) -> Result<(), Trap> {
        let bytes = <Value128>::stack_pop(&mut self.store.value_stack).to_mem_bytes();
        let lane_offset = lane as usize * N;
        let mut val = [0u8; N];
        val.copy_from_slice(&bytes[lane_offset..lane_offset + N]);
        let mem_addr = self.module.resolve_mem_addr(mem_addr);
        let mem = self.store.state.get_mem_mut(mem_addr);
        let addr = match mem.is_64bit() {
            true => <i64>::stack_pop(&mut self.store.value_stack) as u64,
            false => <i32>::stack_pop(&mut self.store.value_stack) as u32 as u64,
        };
        match mem.store(addr, offset, val) {
            Ok(()) => Ok(()),
            Err(e) => {
                cold_path();
                Err(e)
            }
        }
    }

    fn exec_mem_store<T: InternalValue, U: MemValue<N>, const N: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        cast: impl Fn(T) -> U,
    ) -> Result<(), Trap> {
        let val = <T>::stack_pop(&mut self.store.value_stack);
        let val = cast(val).to_mem_bytes();

        let mem_addr = self.module.resolve_mem_addr(mem_addr);
        let mem = self.store.state.get_mem_mut(mem_addr);
        let addr = match mem.is_64bit() {
            true => <i64>::stack_pop(&mut self.store.value_stack) as u64,
            false => <i32>::stack_pop(&mut self.store.value_stack) as u32 as u64,
        };
        match mem.store(addr, offset, val) {
            Ok(()) => Ok(()),
            Err(e) => {
                cold_path();
                Err(e)
            }
        }
    }

    fn exec_table_get(&mut self, table_index: u32) -> Result<(), Trap> {
        let idx: i32 = <i32>::stack_pop(&mut self.store.value_stack);
        let table = self.store.state.get_table(self.module.resolve_table_addr(table_index));
        let v = table.get_wasm_val(idx as u32)?;
        self.store.value_stack.push_dyn(v.into())
    }
    fn exec_table_set(&mut self, table_index: u32) -> Result<(), Trap> {
        let val = <ValueRef>::stack_pop(&mut self.store.value_stack);
        let idx = <i32>::stack_pop(&mut self.store.value_stack) as u32;
        let table = self.store.state.get_table_mut(self.module.resolve_table_addr(table_index));
        table.set(idx, val.addr().into())
    }
    fn exec_table_size(&mut self, table_index: u32) -> Result<(), Trap> {
        let table = self.store.state.get_table(self.module.resolve_table_addr(table_index));
        self.store.value_stack.push(table.size())
    }
    fn exec_table_init(&mut self, elem_index: u32, table_index: u32) -> Result<(), Trap> {
        let size = i32::stack_pop(&mut self.store.value_stack); // n
        let offset = i32::stack_pop(&mut self.store.value_stack); // s
        let dst = i32::stack_pop(&mut self.store.value_stack); // d
        let elem_addr = self.module.resolve_elem_addr(elem_index) as usize;
        let elem = self.store.state.elements.get(elem_addr).ok_or_else(|| Trap::Other("element not found"))?;

        let table_addr = self.module.resolve_table_addr(table_index) as usize;
        let table = self.store.state.tables.get_mut(table_addr).ok_or_else(|| Trap::Other("table not found"))?;

        let elem_len = elem.items.as_ref().map_or(0, alloc::vec::Vec::len);
        let table_len = table.size();

        if size < 0 || ((size + offset) as usize > elem_len) || ((dst + size) > table_len) {
            cold_path();
            return Err(Trap::TableOutOfBounds { offset: offset as usize, len: size as usize, max: elem_len });
        }

        if size == 0 {
            return Ok(());
        }

        if let ElementKind::Active { .. } = elem.kind {
            cold_path();
            return Err(Trap::Other("table.init with active element"));
        }

        let Some(items) = elem.items.as_ref() else {
            cold_path();
            return Err(Trap::TableOutOfBounds { offset: 0, len: 0, max: 0 });
        };

        table.init(i64::from(dst), &items[offset as usize..(offset + size) as usize])
    }
    fn exec_table_grow(&mut self, table_index: u32) -> Result<(), Trap> {
        let table = self.store.state.get_table_mut(self.module.resolve_table_addr(table_index));
        let sz = table.size();
        let n = <i32>::stack_pop(&mut self.store.value_stack);
        let val = <ValueRef>::stack_pop(&mut self.store.value_stack);
        match table.grow(n, val.addr().into()) {
            Ok(()) => self.store.value_stack.push(sz),
            Err(_) => self.store.value_stack.push(-1_i32),
        }
    }
    fn exec_table_fill(&mut self, table_index: u32) -> Result<(), Trap> {
        let table = self.store.state.get_table_mut(self.module.resolve_table_addr(table_index));

        let n = <i32>::stack_pop(&mut self.store.value_stack);
        let val = <ValueRef>::stack_pop(&mut self.store.value_stack);
        let i = <i32>::stack_pop(&mut self.store.value_stack);

        if i + n > table.size() {
            cold_path();
            return Err(Trap::TableOutOfBounds { offset: i as usize, len: n as usize, max: table.size() as usize });
        }

        if n == 0 {
            return Ok(());
        }

        table.fill(self.module.func_addrs(), i as usize, n as usize, val.addr().into())
    }
}

impl<'store> Executor<'store, false> {
    #[inline(always)]
    pub(crate) fn run_to_completion(&mut self) -> Result<(), Trap> {
        // ideally we use `loop_match` / `become` once thats stabilized
        loop {
            if self.exec()?.is_some() {
                return Ok(());
            }
        }
    }

    #[cfg(feature = "std")]
    #[inline(always)]
    pub(crate) fn run_with_time_budget(&mut self, time_budget: core::time::Duration) -> Result<ExecState, Trap> {
        use crate::std::time::Instant;
        let start = Instant::now();
        if time_budget.is_zero() {
            return Ok(ExecState::Suspended(self.cf));
        }

        loop {
            for _ in 0..1024 {
                if self.exec()?.is_some() {
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
    pub(crate) fn run_with_fuel(&mut self, fuel: u32) -> Result<ExecState, Trap> {
        self.store.execution_fuel = fuel;
        if self.store.execution_fuel == 0 {
            return Ok(ExecState::Suspended(self.cf));
        }

        loop {
            for _ in 0..1024 {
                if self.exec()?.is_some() {
                    return Ok(ExecState::Completed);
                }
            }

            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(1024_u32);
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
