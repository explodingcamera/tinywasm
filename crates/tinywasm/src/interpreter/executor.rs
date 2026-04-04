#[cfg(not(feature = "std"))]
#[allow(unused_imports)]
use super::no_std_floats::NoStdFloatExt;

use alloc::boxed::Box;
use alloc::{rc::Rc, string::ToString};

use interpreter::stack::CallFrame;
use tinywasm_types::*;

use super::ExecState;
use super::num_helpers::*;
use super::values::*;
use crate::engine::FuelPolicy;
use crate::instance::ModuleInstanceInner;
use crate::interpreter::Value128;
use crate::*;

#[cfg(feature = "std")]
const TIME_BUDGET_CHECK_INTERVAL: usize = 2048;
const FUEL_ACCOUNTING_INTERVAL: usize = 1024;
const FUEL_COST_CALL_TOTAL: u32 = 5;

pub(crate) struct Executor<'store, const BUDGETED: bool> {
    cf: CallFrame,
    func: Rc<WasmFunction>,
    module: Rc<ModuleInstanceInner>,
    store: &'store mut Store,
}

impl<'store, const BUDGETED: bool> Executor<'store, BUDGETED> {
    pub(crate) fn new(store: &'store mut Store, cf: CallFrame) -> Result<Self> {
        let module = store.get_module_instance_raw(cf.module_addr).clone();
        let func = store.state.get_wasm_func(cf.func_addr).clone();
        Ok(Self { module, store, cf, func })
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
    fn exec<const ITERATIONS: usize>(&mut self) -> Result<Option<()>> {
        macro_rules! stack_op {
            (simd_unary $method:ident) => { stack_op!(unary Value128, |v| v.$method()) };
            (simd_binary $method:ident) => { stack_op!(binary Value128, |a, b| a.$method(b)) };
            (unary $ty:ty, |$v:ident| $expr:expr) => { self.store.stack.values.unary::<$ty>(|$v| Ok($expr))? };
            (binary $ty:ty, |$a:ident, $b:ident| $expr:expr) => { self.store.stack.values.binary::<$ty>(|$a, $b| Ok($expr))? };
            (binary try $ty:ty, |$a:ident, $b:ident| $expr:expr) => { self.store.stack.values.binary::<$ty>(|$a, $b| $expr)? };
            (unary $from:ty => $to:ty, |$v:ident| $expr:expr) => { self.store.stack.values.unary_into::<$from, $to>(|$v| Ok($expr))? };
            (binary $from:ty => $to:ty, |$a:ident, $b:ident| $expr:expr) => { self.store.stack.values.binary_into::<$from, $to>(|$a, $b| Ok($expr))? };
            (binary_into2 $from:ty => $to:ty, |$a:ident, $b:ident| $expr:expr) => {{
                let $b = self.store.stack.values.pop::<$from>();
                let $a = self.store.stack.values.pop::<$from>();
                let out = $expr;
                self.store.stack.values.push::<$to>(out.0)?;
                self.store.stack.values.push::<$to>(out.1)?;
            }};
            (binary $a:ty, $b:ty, |$lhs:ident, $rhs:ident| $expr:expr) => { stack_op!(binary $a, $b => $b, |$lhs, $rhs| $expr) };
            (binary $a:ty, $b:ty => $res:ty, |$lhs:ident, $rhs:ident| $expr:expr) => { self.store.stack.values.binary_mixed::<$a, $b, $res>(|$lhs, $rhs| Ok($expr))? };
            (ternary $ty:ty, |$a:ident, $b:ident, $c:ident| $expr:expr) => { self.store.stack.values.ternary::<$ty>(|$a, $b, $c| Ok($expr))? };
            (quaternary_into2 $from:ty => $to:ty, |$a:ident, $b:ident, $c:ident, $d:ident| $expr:expr) => {{
                let $d = self.store.stack.values.pop::<$from>();
                let $c = self.store.stack.values.pop::<$from>();
                let $b = self.store.stack.values.pop::<$from>();
                let $a = self.store.stack.values.pop::<$from>();
                let out = $expr;
                self.store.stack.values.push::<$to>(out.0)?;
                self.store.stack.values.push::<$to>(out.1)?;
            }};
            (local_set_pop $ty:ty, $local_index:expr) => {{
                let val = self.store.stack.values.pop::<$ty>();
                self.store.stack.values.local_set(&self.cf, *$local_index, val);
            }};
            (local_tee $ty:ty, $local_index:expr) => {{
                let val = self.store.stack.values.peek::<$ty>();
                self.store.stack.values.local_set(&self.cf, *$local_index, val);
            }};
        }

        for _ in 0..ITERATIONS {
            use tinywasm_types::Instruction::*;

            let next = match self.func.instructions.0.get(self.cf.instr_ptr as usize) {
                Some(instr) => instr,
                None => unreachable!(
                    "Instruction pointer out of bounds: {} ({} instructions)",
                    self.cf.instr_ptr,
                    self.func.instructions.0.len()
                ),
            };

            #[rustfmt::skip]
            match next {
                Nop | I32ReinterpretF32 | I64ReinterpretF64 | F32ReinterpretI32 | F64ReinterpretI64 => {}
                Unreachable => return Err(Trap::Unreachable.into()),
                Drop32 => self.store.stack.values.drop::<Value32>(),
                Drop64 => self.store.stack.values.drop::<Value64>(),
                Drop128 => self.store.stack.values.drop::<Value128>(),
                DropRef => self.store.stack.values.drop::<ValueRef>(),
                Select32 => self.store.stack.values.select::<Value32>()?,
                Select64 => self.store.stack.values.select::<Value64>()?,
                Select128 => self.store.stack.values.select::<Value128>()?,
                SelectRef => self.store.stack.values.select::<ValueRef>()?,
                SelectMulti(counts) => self.store.stack.values.select_multi(*counts),
                Call(v) => { self.exec_call_direct::<false>(*v)?; continue; }
                CallSelf => { self.exec_call_self::<false>()?; continue; }
                CallIndirect(ty, table) => { self.exec_call_indirect::<false>(*ty, *table)?; continue; }
                ReturnCall(v) => { self.exec_call_direct::<true>(*v)?; continue; }
                ReturnCallSelf => { self.exec_call_self::<true>()?; continue; }
                ReturnCallIndirect(ty, table) => { self.exec_call_indirect::<true>(*ty, *table)?; continue; }
                Jump(ip) => { self.exec_jump(*ip); continue; }
                JumpIfZero(ip) => if self.exec_jump_if_zero(*ip) { continue; },
                DropKeepSmall { base32, keep32, base64, keep64, base128, keep128, base_ref, keep_ref } => {
                    let b32 = self.cf.stack_base().s32 + *base32 as u32;
                    let k32 = *keep32 as usize;
                    self.store.stack.values.stack_32.truncate_keep(b32 as usize, k32);
                    let b64 = self.cf.stack_base().s64 + *base64 as u32;
                    let k64 = *keep64 as usize;
                    self.store.stack.values.stack_64.truncate_keep(b64 as usize, k64);
                    let b128 = self.cf.stack_base().s128 + *base128 as u32;
                    let k128 = *keep128 as usize;
                    self.store.stack.values.stack_128.truncate_keep(b128 as usize, k128);
                    let bref = self.cf.stack_base().sref + *base_ref as u32;
                    let kref = *keep_ref as usize;
                    self.store.stack.values.stack_ref.truncate_keep(bref as usize, kref);
                }
                DropKeep32(base, keep) => {
                    let b = self.cf.stack_base().s32 + *base as u32;
                    let k = *keep as usize;
                    self.store.stack.values.stack_32.truncate_keep(b as usize, k);
                }
                DropKeep64(base, keep) => {
                    let b = self.cf.stack_base().s64 + *base as u32;
                    let k = *keep as usize;
                    self.store.stack.values.stack_64.truncate_keep(b as usize, k);
                }
                DropKeep128(base, keep) => {
                    let b = self.cf.stack_base().s128 + *base as u32;
                    let k = *keep as usize;
                    self.store.stack.values.stack_128.truncate_keep(b as usize, k);
                }
                DropKeepRef(base, keep) => {
                    let b = self.cf.stack_base().sref + *base as u32;
                    let k = *keep as usize;
                    self.store.stack.values.stack_ref.truncate_keep(b as usize, k);
                }
                BranchTable(default_ip, len) => { self.exec_branch_table(*default_ip, *len); continue; }
                BranchTableTarget {..} => {},
                Return => { if self.exec_return() { return Ok(Some(())); } continue; }
                LocalGet32(local_index) => self.store.stack.values.push(self.store.stack.values.local_get::<Value32>(&self.cf, *local_index))?,
                LocalGet64(local_index) => self.store.stack.values.push(self.store.stack.values.local_get::<Value64>(&self.cf, *local_index))?,
                LocalGet128(local_index) => self.store.stack.values.push(self.store.stack.values.local_get::<Value128>(&self.cf, *local_index))?,
                LocalGetRef(local_index) => self.store.stack.values.push(self.store.stack.values.local_get::<ValueRef>(&self.cf, *local_index))?,
                LocalSet32(local_index) => stack_op!(local_set_pop Value32, local_index),
                LocalSet64(local_index) => stack_op!(local_set_pop Value64, local_index),
                LocalSet128(local_index) => stack_op!(local_set_pop Value128, local_index),
                LocalSetRef(local_index) => stack_op!(local_set_pop ValueRef, local_index),
                LocalCopy32(from, to) => self.store.stack.values.local_set(&self.cf, *to, self.store.stack.values.local_get::<Value32>(&self.cf, *from)),
                LocalCopy64(from, to) => self.store.stack.values.local_set(&self.cf, *to, self.store.stack.values.local_get::<Value64>(&self.cf, *from)),
                LocalCopy128(from, to) => self.store.stack.values.local_set(&self.cf, *to, self.store.stack.values.local_get::<Value128>(&self.cf, *from)),
                LocalCopyRef(from, to) => self.store.stack.values.local_set(&self.cf, *to, self.store.stack.values.local_get::<ValueRef>(&self.cf, *from)),
                I32AddLocals(a, b) => self.store.stack.values.push(self.store.stack.values.local_get::<i32>(&self.cf, *a).wrapping_add(self.store.stack.values.local_get::<i32>(&self.cf, *b)))?,
                I64AddLocals(a, b) => self.store.stack.values.push(self.store.stack.values.local_get::<i64>(&self.cf, *a).wrapping_add(self.store.stack.values.local_get::<i64>(&self.cf, *b)))?,
                I32AddConst(c) => stack_op!(unary i32, |v| v.wrapping_add(*c)),
                I64AddConst(c) => stack_op!(unary i64, |v| v.wrapping_add(*c)),
                I32StoreLocalLocal(m, addr_local, value_local) => {
                    let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(m.mem_addr()));
                    let addr_local = u16::from(*addr_local);
                    let value_local = u16::from(*value_local);
                    let addr = u64::from(self.store.stack.values.local_get::<u32>(&self.cf, addr_local));
                    let value = self.store.stack.values.local_get::<u32>(&self.cf, value_local).to_mem_bytes();
                    mem.store((m.offset() + addr) as usize, value.len(), &value)?;
                }
                I32LoadLocalTee(m, addr_local, dst_local) => {
                    let mem = self.store.state.get_mem(self.module.resolve_mem_addr(m.mem_addr()));
                    let addr_local = u16::from(*addr_local);
                    let dst_local = u16::from(*dst_local);
                    let addr = u64::from(self.store.stack.values.local_get::<u32>(&self.cf, addr_local));
                    let Some(Ok(addr)) = m.offset().checked_add(addr).map(|a| a.try_into()) else {
                        return Err(Error::Trap(Trap::MemoryOutOfBounds {
                            offset: addr as usize,
                            len: 4,
                            max: 0,
                        }));
                    };
                    let value = mem.load_as::<4, i32>(addr)?;
                    self.store.stack.values.local_set(&self.cf, dst_local, value);
                    self.store.stack.values.push(value)?;
                }
                I64XorRotlConst(c) => stack_op!(binary i64, |lhs, rhs| (lhs ^ rhs).rotate_left(*c as u32)),
                I64XorRotlConstTee(c, local_index) => {
                    stack_op!(binary i64, |lhs, rhs| (lhs ^ rhs).rotate_left(*c as u32));
                    stack_op!(local_tee i64, local_index);
                }
                LocalTee32(local_index) => stack_op!(local_tee Value32, local_index),
                LocalTee64(local_index) => stack_op!(local_tee Value64, local_index),
                LocalTee128(local_index) => stack_op!(local_tee Value128, local_index),
                LocalTeeRef(local_index) => stack_op!(local_tee ValueRef, local_index),
                GlobalGet(global_index) => self.exec_global_get(*global_index)?,
                GlobalSet32(global_index) => self.exec_global_set::<Value32>(*global_index),
                GlobalSet64(global_index) => self.exec_global_set::<Value64>(*global_index),
                GlobalSet128(global_index) => self.exec_global_set::<Value128>(*global_index),
                GlobalSetRef(global_index) => self.exec_global_set::<ValueRef>(*global_index),
                I32Const(val) => self.exec_const(*val)?,
                I64Const(val) => self.exec_const(*val)?,
                F32Const(val) => self.exec_const(*val)?,
                F64Const(val) => self.exec_const(*val)?,
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
                RefFunc(func_idx) => self.exec_const::<ValueRef>(Some(*func_idx))?,
                RefNull(_) => self.exec_const::<ValueRef>(None)?,
                RefIsNull => self.exec_ref_is_null()?,
                MemorySize(addr) => self.exec_memory_size(*addr)?,
                MemoryGrow(addr) => self.exec_memory_grow(*addr)?,

                // Bulk memory operations
                MemoryCopy(from, to) => self.exec_memory_copy(*from, *to)?,
                MemoryFill(addr) => self.exec_memory_fill(*addr)?,
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
                TableCopy { from, to } => self.exec_table_copy(*from, *to)?,

                // Core memory load/store operations
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
                V128Bitselect => stack_op!(ternary Value128, |v1, v2, c| Value128::v128_bitselect(v1, v2, c)),
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
                V128Const(arg) => self.exec_const::<Value128>(self.func.data.v128_constants[*arg as usize].into())?,
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
                I8x16Shuffle(idx) => { let idx = self.func.data.v128_constants[*idx as usize].to_le_bytes(); stack_op!(binary Value128, |a, b| Value128::i8x16_shuffle(a, b, idx)) }
                I16x8Q15MulrSatS => stack_op!(binary Value128, |a, b| a.i16x8_q15mulr_sat_s(b)),
                I32x4DotI16x8S => stack_op!(binary Value128, |a, b| a.i32x4_dot_i16x8_s(b)),
                I8x16RelaxedLaneselect => stack_op!(ternary Value128, |v1, v2, c| Value128::i8x16_relaxed_laneselect(v1, v2, c)),
                I16x8RelaxedLaneselect => stack_op!(ternary Value128, |v1, v2, c| Value128::i16x8_relaxed_laneselect(v1, v2, c)),
                I32x4RelaxedLaneselect => stack_op!(ternary Value128, |v1, v2, c| Value128::i32x4_relaxed_laneselect(v1, v2, c)),
                I64x2RelaxedLaneselect => stack_op!(ternary Value128, |v1, v2, c| Value128::i64x2_relaxed_laneselect(v1, v2, c)),
                I16x8RelaxedQ15mulrS => stack_op!(binary Value128, |a, b| a.i16x8_relaxed_q15mulr_s(b)),
                I16x8RelaxedDotI8x16I7x16S => stack_op!(binary Value128, |a, b| a.i16x8_relaxed_dot_i8x16_i7x16_s(b)),
                I32x4RelaxedDotI8x16I7x16AddS => stack_op!(ternary Value128, |a, b, c| a.i32x4_relaxed_dot_i8x16_i7x16_add_s(b, c)),
                F32x4Ceil => stack_op!(simd_unary f32x4_ceil),
                F64x2Ceil => stack_op!(simd_unary f64x2_ceil),
                F32x4Floor => stack_op!(simd_unary f32x4_floor),
                F64x2Floor => stack_op!(simd_unary f64x2_floor),
                F32x4Trunc => stack_op!(simd_unary f32x4_trunc),
                F64x2Trunc => stack_op!(simd_unary f64x2_trunc),
                F32x4Nearest => stack_op!(simd_unary f32x4_nearest),
                F64x2Nearest => stack_op!(simd_unary f64x2_nearest),
                F32x4Abs => stack_op!(simd_unary f32x4_abs),
                F64x2Abs => stack_op!(simd_unary f64x2_abs),
                F32x4Neg => stack_op!(simd_unary f32x4_neg),
                F64x2Neg => stack_op!(simd_unary f64x2_neg),
                F32x4Sqrt => stack_op!(simd_unary f32x4_sqrt),
                F64x2Sqrt => stack_op!(simd_unary f64x2_sqrt),
                F32x4Add => stack_op!(simd_binary f32x4_add),
                F64x2Add => stack_op!(simd_binary f64x2_add),
                F32x4Sub => stack_op!(simd_binary f32x4_sub),
                F64x2Sub => stack_op!(simd_binary f64x2_sub),
                F32x4Mul => stack_op!(simd_binary f32x4_mul),
                F64x2Mul => stack_op!(simd_binary f64x2_mul),
                F32x4Div => stack_op!(simd_binary f32x4_div),
                F64x2Div => stack_op!(simd_binary f64x2_div),
                F32x4Min => stack_op!(simd_binary f32x4_min),
                F64x2Min => stack_op!(simd_binary f64x2_min),
                F32x4Max => stack_op!(simd_binary f32x4_max),
                F64x2Max => stack_op!(simd_binary f64x2_max),
                F32x4PMin => stack_op!(simd_binary f32x4_pmin),
                F32x4PMax => stack_op!(simd_binary f32x4_pmax),
                F64x2PMin => stack_op!(simd_binary f64x2_pmin),
                F64x2PMax => stack_op!(simd_binary f64x2_pmax),
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
        }

        Ok(None)
    }

    #[inline(always)]
    fn exec_jump(&mut self, ip: u32) {
        self.cf.instr_ptr = ip;
    }

    #[inline(always)]
    fn exec_jump_if_zero(&mut self, ip: u32) -> bool {
        if self.store.stack.values.pop::<i32>() == 0 {
            self.cf.instr_ptr = ip;
            return true;
        }
        false
    }

    #[inline(always)]
    fn exec_branch_table(&mut self, default_ip: u32, len: u32) {
        let idx = self.store.stack.values.pop::<i32>();
        let start = self.cf.instr_ptr + 1;

        let target_ip = if idx >= 0 && (idx as u32) < len {
            match self.func.instructions.0.get((start + idx as u32) as usize) {
                Some(Instruction::BranchTableTarget(ip)) => *ip,
                _ => default_ip,
            }
        } else {
            default_ip
        };

        self.cf.instr_ptr = target_ip;
    }

    fn exec_call<const IS_RETURN_CALL: bool>(
        &mut self,
        wasm_func: Rc<WasmFunction>,
        func_addr: FuncAddr,
        owner: ModuleInstanceAddr,
    ) -> Result<()> {
        if !Rc::ptr_eq(&self.func, &wasm_func) {
            self.func = wasm_func.clone();
        }

        if IS_RETURN_CALL {
            self.store.stack.values.truncate_keep_counts(self.cf.locals_base, wasm_func.params);
        }

        let res = self.store.stack.values.enter_locals(&wasm_func.params, &wasm_func.locals);
        let locals_base = res.map_err(|err| if IS_RETURN_CALL { err } else { Error::Trap(Trap::CallStackOverflow) })?;
        let new_call_frame = CallFrame::new(func_addr, owner, locals_base, wasm_func.locals);

        if !IS_RETURN_CALL {
            self.cf.incr_instr_ptr(); // skip the call instruction
            self.store.stack.call_stack.push(self.cf)?;
        }
        self.cf = new_call_frame;

        if self.cf.module_addr != self.module.idx {
            self.module = self.store.get_module_instance_raw(self.cf.module_addr).clone();
        }

        Ok(())
    }
    fn exec_call_host(&mut self, host_func: Rc<imports::HostFunction>) -> Result<()> {
        let params = self.store.stack.values.pop_types(&host_func.ty.params).collect::<Box<_>>();
        let res = host_func.call(FuncContext { store: self.store, module_addr: self.module.idx }, &params)?;
        self.store.stack.values.extend_from_wasmvalues(&res)?;
        self.cf.incr_instr_ptr();
        Ok(())
    }
    fn exec_call_direct<const IS_RETURN_CALL: bool>(&mut self, v: u32) -> Result<()> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let addr = self.module.resolve_func_addr(v);
        let func_inst = self.store.state.get_func(addr);
        match &func_inst.func {
            crate::Function::Wasm(wasm_func) => {
                self.exec_call::<IS_RETURN_CALL>(wasm_func.clone(), addr, func_inst.owner)
            }
            crate::Function::Host(host_func) => self.exec_call_host(host_func.clone()),
        }
    }

    fn exec_call_self<const IS_RETURN_CALL: bool>(&mut self) -> Result<()> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        let params = self.func.params;
        let locals = self.func.locals;

        if IS_RETURN_CALL {
            self.store.stack.values.truncate_keep_counts(self.cf.locals_base, params);
        }

        let res = self.store.stack.values.enter_locals(&params, &locals);
        let locals_base = res.map_err(|err| if IS_RETURN_CALL { err } else { Error::Trap(Trap::CallStackOverflow) })?;
        let new_call_frame = CallFrame::new(self.cf.func_addr, self.cf.module_addr, locals_base, locals);

        if !IS_RETURN_CALL {
            self.cf.incr_instr_ptr();
            self.store.stack.call_stack.push(self.cf)?;
        }
        self.cf = new_call_frame;
        Ok(())
    }

    fn exec_call_indirect<const IS_RETURN_CALL: bool>(&mut self, type_addr: u32, table_addr: u32) -> Result<()> {
        self.charge_call_fuel(FUEL_COST_CALL_TOTAL);
        // verify that the table is of the right type, this should be validated by the parser already
        let func_ref = {
            let table_idx: u32 = self.store.stack.values.pop::<i32>() as u32;
            let table = self.store.state.get_table(self.module.resolve_table_addr(table_addr));
            assert!(table.kind.element_type == ValType::RefFunc, "table is not of type funcref");
            let table =
                table.get(table_idx).map_err(|_| Error::from(Trap::UndefinedElement { index: table_idx as usize }))?;
            table.addr().ok_or_else(|| Error::from(Trap::UninitializedElement { index: table_idx as usize }))?
        };

        let func_inst = self.store.state.get_func(func_ref);
        let call_ty = self.module.func_ty(type_addr);

        match &func_inst.func {
            crate::Function::Wasm(wasm_func) => {
                if wasm_func.ty != *call_ty {
                    return Err(Trap::IndirectCallTypeMismatch {
                        actual: wasm_func.ty.clone(),
                        expected: call_ty.clone(),
                    }
                    .into());
                }

                self.exec_call::<IS_RETURN_CALL>(wasm_func.clone(), func_ref, func_inst.owner)
            }
            crate::Function::Host(host_func) => {
                if host_func.ty != *call_ty {
                    return Err(Trap::IndirectCallTypeMismatch {
                        actual: host_func.ty.clone(),
                        expected: call_ty.clone(),
                    }
                    .into());
                }

                self.exec_call_host(host_func.clone())
            }
        }
    }

    fn exec_return(&mut self) -> bool {
        let result_counts = ValueCountsSmall::from(self.func.ty.results.iter());
        self.store.stack.values.truncate_keep_counts(self.cf.locals_base, result_counts);

        let Some(cf) = self.store.stack.call_stack.pop() else { return true };

        if cf.func_addr != self.cf.func_addr {
            self.func = self.store.state.get_wasm_func(cf.func_addr).clone();

            if cf.module_addr != self.module.idx {
                self.module = self.store.get_module_instance_raw(cf.module_addr).clone();
            }
        }
        self.cf = cf;
        false
    }
    fn exec_global_get(&mut self, global_index: u32) -> Result<()> {
        self.store.stack.values.push_dyn(self.store.state.get_global_val(self.module.resolve_global_addr(global_index)))
    }

    fn exec_global_set<T: InternalValue>(&mut self, global_index: u32) {
        let val = self.store.stack.values.pop::<T>().into();
        self.store.state.set_global_val(self.module.resolve_global_addr(global_index), val);
    }
    fn exec_const<T: InternalValue>(&mut self, val: T) -> Result<()> {
        self.store.stack.values.push(val)
    }
    fn exec_ref_is_null(&mut self) -> Result<()> {
        let is_null = i32::from(self.store.stack.values.pop::<ValueRef>().is_none());
        self.store.stack.values.push::<i32>(is_null)
    }

    fn exec_memory_size(&mut self, addr: u32) -> Result<()> {
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(addr));
        match mem.is_64bit() {
            true => self.store.stack.values.push::<i64>(mem.page_count as i64),
            false => self.store.stack.values.push::<i32>(mem.page_count as i32),
        }
    }
    fn exec_memory_grow(&mut self, addr: u32) -> Result<()> {
        let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(addr));
        let is_64bit = mem.is_64bit();
        let pages_delta = match is_64bit {
            true => self.store.stack.values.pop::<i64>(),
            false => i64::from(self.store.stack.values.pop::<i32>()),
        };

        let size = mem.grow(pages_delta).unwrap_or(-1);
        match is_64bit {
            true => self.store.stack.values.push::<i64>(size)?,
            false => self.store.stack.values.push::<i32>(size as i32)?,
        };

        Ok(())
    }

    fn exec_memory_copy(&mut self, from: u32, to: u32) -> Result<()> {
        let size: i32 = self.store.stack.values.pop();
        let src: i32 = self.store.stack.values.pop();
        let dst: i32 = self.store.stack.values.pop();

        if from == to {
            let mem_from = self.store.state.get_mem_mut(self.module.resolve_mem_addr(from));
            // copy within the same memory
            mem_from.copy_within(dst as usize, src as usize, size as usize)?;
        } else {
            // copy between two memories
            let (mem_from, mem_to) =
                self.store.state.get_mems_mut(self.module.resolve_mem_addr(from), self.module.resolve_mem_addr(to))?;

            mem_from.copy_from_slice(dst as usize, mem_to.load(src as usize, size as usize)?)?;
        }
        Ok(())
    }
    fn exec_memory_fill(&mut self, addr: u32) -> Result<()> {
        let size: i32 = self.store.stack.values.pop();
        let val: i32 = self.store.stack.values.pop();
        let dst: i32 = self.store.stack.values.pop();

        let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(addr));
        mem.fill(dst as usize, size as usize, val as u8)
    }
    fn exec_memory_init(&mut self, data_index: u32, mem_index: u32) -> Result<()> {
        let size: i32 = self.store.stack.values.pop();
        let offset: i32 = self.store.stack.values.pop();
        let dst: i32 = self.store.stack.values.pop();

        let data_addr = self.module.resolve_data_addr(data_index) as usize;
        let data = self.store.state.data.get(data_addr).ok_or_else(|| Error::Other("data not found".to_string()))?;

        let mem_addr = self.module.resolve_mem_addr(mem_index) as usize;
        let mem =
            self.store.state.memories.get_mut(mem_addr).ok_or_else(|| Error::Other("memory not found".to_string()))?;

        let data_len = data.data.as_ref().map_or(0, |d| d.len());

        if ((size + offset) as usize > data_len) || ((dst + size) as usize > mem.len()) {
            return Err(Trap::MemoryOutOfBounds { offset: offset as usize, len: size as usize, max: data_len }.into());
        }

        if size == 0 {
            return Ok(());
        }

        let Some(data) = &data.data else { return Err(Trap::MemoryOutOfBounds { offset: 0, len: 0, max: 0 }.into()) };
        mem.store(dst as usize, size as usize, &data[offset as usize..((offset + size) as usize)])
    }
    fn exec_table_copy(&mut self, from: u32, to: u32) -> Result<()> {
        let size: i32 = self.store.stack.values.pop();
        let src: i32 = self.store.stack.values.pop();
        let dst: i32 = self.store.stack.values.pop();

        if from == to {
            // copy within the same memory
            self.store.state.get_table_mut(self.module.resolve_table_addr(from)).copy_within(
                dst as usize,
                src as usize,
                size as usize,
            )
        } else {
            // copy between two memories
            let (table_from, table_to) = self
                .store
                .state
                .get_tables_mut(self.module.resolve_table_addr(from), self.module.resolve_table_addr(to))?;
            table_to.copy_from_slice(dst as usize, table_from.load(src as usize, size as usize)?)
        }
    }

    fn exec_mem_load_lane<LOAD: MemValue<LOAD_SIZE>, const LOAD_SIZE: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        lane: u8,
    ) -> Result<()> {
        let mut imm = self.store.stack.values.pop::<Value128>().to_mem_bytes();
        let val = self.store.stack.values.pop::<i32>() as u64;
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(mem_addr));
        let Some(Ok(addr)) = offset.checked_add(val).map(TryInto::try_into) else {
            return Err(Error::Trap(Trap::MemoryOutOfBounds { offset: val as usize, len: LOAD_SIZE, max: 0 }));
        };
        let val = mem.load_as::<LOAD_SIZE, LOAD>(addr)?.to_mem_bytes();

        let offset = lane as usize * LOAD_SIZE;
        imm[offset..offset + LOAD_SIZE].copy_from_slice(&val);

        self.store.stack.values.push(Value128::from_mem_bytes(imm))?;
        Ok(())
    }

    #[inline(always)]
    fn exec_mem_load<LOAD: MemValue<LOAD_SIZE>, const LOAD_SIZE: usize, TARGET: InternalValue>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        cast: fn(LOAD) -> TARGET,
    ) -> Result<()> {
        let mem = self.store.state.get_mem(self.module.resolve_mem_addr(mem_addr));

        let base: u64 = if mem.is_64bit() {
            self.store.stack.values.pop::<i64>() as u64
        } else {
            self.store.stack.values.pop::<i32>() as u32 as u64
        };

        let Some(addr) = base.checked_add(offset) else {
            return Err(Error::Trap(Trap::MemoryOutOfBounds { offset: base as usize, len: LOAD_SIZE, max: 0 }));
        };

        let Ok(addr) = usize::try_from(addr) else {
            return Err(Error::Trap(Trap::MemoryOutOfBounds { offset: base as usize, len: LOAD_SIZE, max: 0 }));
        };

        let val = mem.load_as::<LOAD_SIZE, LOAD>(addr)?;
        self.store.stack.values.push(cast(val))?;
        Ok(())
    }

    fn exec_mem_store_lane<U: MemValue<N> + Copy, const N: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        lane: u8,
    ) -> Result<()> {
        let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(mem_addr));
        let bytes = self.store.stack.values.pop::<Value128>().to_mem_bytes();
        let lane_offset = lane as usize * N;
        let mut val = [0u8; N];
        val.copy_from_slice(&bytes[lane_offset..lane_offset + N]);

        let addr = match mem.is_64bit() {
            true => self.store.stack.values.pop::<i64>() as u64,
            false => self.store.stack.values.pop::<i32>() as u32 as u64,
        };

        mem.store((offset + addr) as usize, val.len(), &val)?;

        Ok(())
    }

    fn exec_mem_store<T: InternalValue, U: MemValue<N>, const N: usize>(
        &mut self,
        mem_addr: tinywasm_types::MemAddr,
        offset: u64,
        cast: fn(T) -> U,
    ) -> Result<()> {
        let val = self.store.stack.values.pop::<T>();
        let mem = self.store.state.get_mem_mut(self.module.resolve_mem_addr(mem_addr));
        let val = (cast(val)).to_mem_bytes();

        let addr = match mem.is_64bit() {
            true => self.store.stack.values.pop::<i64>() as u64,
            false => u64::from(self.store.stack.values.pop::<i32>() as u32),
        };

        mem.store((offset + addr) as usize, val.len(), &val)?;

        Ok(())
    }

    fn exec_table_get(&mut self, table_index: u32) -> Result<()> {
        let idx: i32 = self.store.stack.values.pop::<i32>();
        let table = self.store.state.get_table(self.module.resolve_table_addr(table_index));
        let v = table.get_wasm_val(idx as u32)?;
        self.store.stack.values.push_dyn(v.into())?;
        Ok(())
    }
    fn exec_table_set(&mut self, table_index: u32) -> Result<()> {
        let val = self.store.stack.values.pop::<ValueRef>();
        let idx = self.store.stack.values.pop::<i32>() as u32;
        let table = self.store.state.get_table_mut(self.module.resolve_table_addr(table_index));
        table.set(idx, val.into())
    }
    fn exec_table_size(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.state.get_table(self.module.resolve_table_addr(table_index));
        self.store.stack.values.push(table.size())?;
        Ok(())
    }
    fn exec_table_init(&mut self, elem_index: u32, table_index: u32) -> Result<()> {
        let size: i32 = self.store.stack.values.pop(); // n
        let offset: i32 = self.store.stack.values.pop(); // s
        let dst: i32 = self.store.stack.values.pop(); // d

        let elem = self
            .store
            .state
            .elements
            .get(self.module.resolve_elem_addr(elem_index) as usize)
            .ok_or_else(|| Error::Other("element not found".to_string()))?;

        let table = self
            .store
            .state
            .tables
            .get_mut(self.module.resolve_table_addr(table_index) as usize)
            .ok_or_else(|| Error::Other("table not found".to_string()))?;

        let elem_len = elem.items.as_ref().map_or(0, alloc::vec::Vec::len);
        let table_len = table.size();

        if size < 0 || ((size + offset) as usize > elem_len) || ((dst + size) > table_len) {
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

        table.init(i64::from(dst), &items[offset as usize..(offset + size) as usize])
    }
    fn exec_table_grow(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.state.get_table_mut(self.module.resolve_table_addr(table_index));
        let sz = table.size();

        let n = self.store.stack.values.pop::<i32>();
        let val = self.store.stack.values.pop::<ValueRef>();

        match table.grow(n, val.into()) {
            Ok(()) => self.store.stack.values.push(sz)?,
            Err(_) => self.store.stack.values.push(-1_i32)?,
        }

        Ok(())
    }
    fn exec_table_fill(&mut self, table_index: u32) -> Result<()> {
        let table = self.store.state.get_table_mut(self.module.resolve_table_addr(table_index));

        let n = self.store.stack.values.pop::<i32>();
        let val = self.store.stack.values.pop::<ValueRef>();
        let i = self.store.stack.values.pop::<i32>();

        if i + n > table.size() {
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
}

impl<'store> Executor<'store, false> {
    #[inline(always)]
    pub(crate) fn run_to_completion(&mut self) -> Result<()> {
        loop {
            // for some reason, using a iteration count of 4096 here seems to be a sweet spot for performance
            if self.exec::<1024>()?.is_some() {
                return Ok(());
            }
        }
    }

    #[cfg(feature = "std")]
    #[inline(always)]
    pub(crate) fn run_with_time_budget(&mut self, time_budget: core::time::Duration) -> Result<ExecState> {
        use crate::std::time::Instant;
        let start = Instant::now();
        if time_budget.is_zero() {
            return Ok(ExecState::Suspended(self.cf));
        }

        loop {
            if self.exec::<TIME_BUDGET_CHECK_INTERVAL>()?.is_some() {
                return Ok(ExecState::Completed);
            }

            if start.elapsed() >= time_budget {
                return Ok(ExecState::Suspended(self.cf));
            }
        }
    }
}

impl<'store> Executor<'store, true> {
    #[inline(always)]
    pub(crate) fn run_with_fuel(&mut self, fuel: u32) -> Result<ExecState> {
        self.store.execution_fuel = fuel;
        if self.store.execution_fuel == 0 {
            return Ok(ExecState::Suspended(self.cf));
        }

        loop {
            if self.exec::<FUEL_ACCOUNTING_INTERVAL>()?.is_some() {
                return Ok(ExecState::Completed);
            }

            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(FUEL_ACCOUNTING_INTERVAL as u32);
            if self.store.execution_fuel == 0 {
                return Ok(ExecState::Suspended(self.cf));
            }
        }
    }
}
