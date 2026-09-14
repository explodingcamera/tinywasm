//! Ordered rule families selected by lowering, not by the incoming opcode.
//! Replacements are contiguous, non-growing suffixes and are not dispatched again.
//! Small literals stay inline until a selected encoding needs a pooled operand.

mod encoding;
use crate::{Result, emitter::PendingTail, visit::FunctionDataBuilder};
use encoding::*;
use tinywasm_types::{Instruction, Instruction::*, *};

// Patterns include the incoming instruction. First match wins, including identity
// replacements. Parenthesized guards support expressions and let chains.
macro_rules! families {
    ($($name:ident($tail:ident, $data:ident $(, $arg:ident: $ty:ty)*) {
        $([$($pattern:pat),+ $(,)?] $(if ($($guard:tt)+))? => $replacement:expr),+ $(,)?
    })*) => {$(
        #[doc = concat!("Applies the ordered `", stringify!($name), "` rules to the pending suffix.")]
        #[inline]
        pub(crate) fn $name($tail: &mut PendingTail<'_>, $data: &mut FunctionDataBuilder $(, $arg: $ty)*) -> Result<()> {
            let _ = &$data;
            $(
                if let Some([$($pattern),+]) = $tail.suffix::<{ [$(stringify!($pattern)),+].len() }>()
                    $( && $($guard)+ )?
                {
                    $tail.replace::<{ [$(stringify!($pattern)),+].len() }, _>($replacement);
                    return Ok(());
                }
            )+
            Ok(())
        }
    )*};
}

#[rustfmt::skip]
families! {
    integer32(tail, data, op: BinOp, commutative: bool) {
        [LocalGet32(a), LocalGet32(b), _] => [BinOpLocalLocal32(op, a, b)],
        [GlobalGet32(global), _] => [BinOpStackGlobal32(op, global)],
        [LocalGet32(local), _] => [BinOpStackLocal32(op, local)],
        // Both producers only read locals, and the outer integer operation commutes.
        [LocalGet32(local), producer, _] if (commutative && local_const32(data, producer).is_some()) => [producer, BinOpStackLocal32(op, local)],
        [LocalGet32(local), Const32(value), _] => [encode_local_const32(data, op, local, value)?],
        [GlobalGet32(global), Const32(value), _] => [BinOpGlobalConst32(PackedOp::new(op, data.push64(Operand64::<(u32, u32)>::new(global, value as u32))?))],
        [Const32(value), LocalGet32(local), _] if (commutative) => [encode_local_const32(data, op, local, value)?],
        [Const32(value), GlobalGet32(global), _] if (commutative) => [BinOpGlobalConst32(PackedOp::new(op, data.push64(Operand64::<(u32, u32)>::new(global, value as u32))?))],
        [Const32(value), I32Add] => [AddConst32(value)],
        [I32Add, I32Add] => [I32Add3],
        [BinOpStackLocal32(BinOp::IAdd, local), I32Add] => [LocalGet32(local), I32Add3],
        [left, Const32(right), _] if (op == BinOp::IShrS
            && let Some((BinOp::IShl, local, shift)) = local_const32(data, left)
            && shift == right && matches!(shift, 16 | 24)) =>
            [LocalGet32(local), if shift == 24 { I32Extend8S } else { I32Extend16S }],
        [Const32(value), _] if (op != BinOp::IAdd) => [stack_const32(op, value)],
    }
    integer64(tail, data, op: BinOp, commutative: bool) {
        [LocalGet64(a), LocalGet64(b), _] => [BinOpLocalLocal64(op, a, b)],
        [GlobalGet64(global), _] => [BinOpStackGlobal64(op, global)],
        [LocalGet64(local), constant, _] if (let Some(value) = const64(data, constant)) => [BinOpLocalConst64(PackedOp::new(op, data.push128(Operand128::<(u16, u64)>::new(local, value as u64))?))],
        [GlobalGet64(global), constant, _] if (let Some(value) = const64(data, constant)) => [BinOpGlobalConst64(PackedOp::new(op, data.push128(Operand128::<(u32, u64)>::new(global, value as u64))?))],
        [constant, LocalGet64(local), _] if (commutative && let Some(value) = const64(data, constant)) => [BinOpLocalConst64(PackedOp::new(op, data.push128(Operand128::<(u16, u64)>::new(local, value as u64))?))],
        [constant, GlobalGet64(global), _] if (commutative && let Some(value) = const64(data, constant)) => [BinOpGlobalConst64(PackedOp::new(op, data.push128(Operand128::<(u32, u64)>::new(global, value as u64))?))],
        [constant @ (Const64(_) | Const64Imm(_)), I64Add] => [AddConst64(pool64(data, constant)?)],
        [I64Add, I64Add] => [I64Add3],
        [left, constant, _] if (op == BinOp::IShrS
            && let Some((BinOp::IShl, local, shift)) = local_const64(data, left)
            && Some(shift) == const64(data, constant) && matches!(shift, 56 | 48 | 32)) =>
            [LocalGet64(local), match shift { 56 => I64Extend8S, 48 => I64Extend16S, _ => I64Extend32S }],
        [constant @ (Const64(_) | Const64Imm(_)), _] if (op != BinOp::IAdd) => [BinOpStackConst64(PackedOp::new(op, pool64(data, constant)?))],
    }
    compare(tail, data, op: CmpOp) {
        [LocalGet32(a), LocalGet32(b), _] => [CmpLocalLocal32(op, a, b)],
        [LocalGet64(a), LocalGet64(b), _] => [CmpLocalLocal64(op, a, b)],
    }
    extend_i32(tail, data, signed: bool) {
        [Const32(value), _] => [{
            let value = if signed { i64::from(value) } else { i64::from(value as u32) };
            match i32::try_from(value) {
                Ok(value) => Const64Imm(value),
                Err(_) => Const64(data.push64(Operand64::<i64>::new(value))?),
            }
        }],
    }
    float32(tail, data, op: BinOp, commutative: bool) {
        [LocalGet32(a), LocalGet32(b), _] => [BinOpLocalLocal32(op, a, b)],
        [LocalGet32(local), _] => [BinOpStackLocal32(op, local)],
        [LocalGet32(local), Const32(value), _] => [encode_local_const32(data, op, local, value)?],
        [GlobalGet32(global), Const32(value), _] => [BinOpGlobalConst32(PackedOp::new(op, data.push64(Operand64::<(u32, u32)>::new(global, value as u32))?))],
        [Const32(value), LocalGet32(local), _] if (commutative) => [encode_local_const32(data, op, local, value)?],
        [Const32(value), GlobalGet32(global), _] if (commutative) => [BinOpGlobalConst32(PackedOp::new(op, data.push64(Operand64::<(u32, u32)>::new(global, value as u32))?))],
        [Const32(value), _] => [stack_const32(op, value)],
    }
    float64(tail, data, op: BinOp, commutative: bool) {
        [LocalGet64(a), LocalGet64(b), _] => [BinOpLocalLocal64(op, a, b)],
        [LocalGet64(local), constant, _] if (let Some(value) = const64(data, constant)) => [BinOpLocalConst64(PackedOp::new(op, data.push128(Operand128::<(u16, u64)>::new(local, value as u64))?))],
        [GlobalGet64(global), constant, _] if (let Some(value) = const64(data, constant)) => [BinOpGlobalConst64(PackedOp::new(op, data.push128(Operand128::<(u32, u64)>::new(global, value as u64))?))],
        [constant, LocalGet64(local), _] if (commutative && let Some(value) = const64(data, constant)) => [BinOpLocalConst64(PackedOp::new(op, data.push128(Operand128::<(u16, u64)>::new(local, value as u64))?))],
        [constant, GlobalGet64(global), _] if (commutative && let Some(value) = const64(data, constant)) => [BinOpGlobalConst64(PackedOp::new(op, data.push128(Operand128::<(u32, u64)>::new(global, value as u64))?))],
        [constant @ (Const64(_) | Const64Imm(_)), _] => [BinOpStackConst64(PackedOp::new(op, pool64(data, constant)?))],
    }
    vector(tail, data, op: BinOp128, commutative: bool) {
        // Stack/local deliberately shadows local/local and reversed local constants.
        [LocalGet128(local), _] => [BinOpStackLocal128(op, local)],
        [LocalGet128(a), LocalGet128(b), _] => [BinOpLocalLocal128(op, a, b)],
        [LocalGet128(local), constant @ (Const128(_) | Const128Imm(_)), _] => [{
            let value = pool128(data, constant)?;
            BinOpLocalConst128(PackedOp::new(op, data.push64(Operand64::<(u16, Operand128Idx<[u8; 16]>)>::new(local, value))?))
        }],
        [GlobalGet128(global), constant @ (Const128(_) | Const128Imm(_)), _] => [{
            let value = pool128(data, constant)?;
            BinOpGlobalConst128(PackedOp::new(op, data.push64(Operand64::<(u32, Operand128Idx<[u8; 16]>)>::new(global, value))?))
        }],
        [constant @ (Const128(_) | Const128Imm(_)), LocalGet128(local), _] if (commutative) => [{
            let value = pool128(data, constant)?;
            BinOpLocalConst128(PackedOp::new(op, data.push64(Operand64::<(u16, Operand128Idx<[u8; 16]>)>::new(local, value))?))
        }],
        [constant @ (Const128(_) | Const128Imm(_)), GlobalGet128(global), _] if (commutative) => [{
            let value = pool128(data, constant)?;
            BinOpGlobalConst128(PackedOp::new(op, data.push64(Operand64::<(u32, Operand128Idx<[u8; 16]>)>::new(global, value))?))
        }],
    }
    store32(tail, data, index: Operand128Idx<MemoryOperand>) {
        [Select32, _] => [SelectStore32(index)],
        [LocalGet32(addr) | LocalGet64(addr), LoadLocal32(arg), AddConst32(1), _]
            if (CompactMemoryArg::try_from(data.operand128(index)).ok().map(Operand64::from)
                == Some(data.operand64(arg.memory_arg_idx)) && addr == u16::from(arg.local1)) => [IncMemoryLocal32(arg)],
        [F32Mul, F32Add, _] if (let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index))) => [FMaStoreF32(arg)],
        [BinOpStackLocal32(BinOp::FMul, local), F32Add, _]
            if (let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index))) => [LocalGet32(local), FMaStoreF32(arg)],
        [LocalGet32(addr), LocalGet32(value), _]
            if (let (Ok(addr), Ok(value), Ok(arg)) = (u8::try_from(addr), u8::try_from(value),
                CompactMemoryArg::try_from(data.operand128(index)))) => [StoreLocalLocal32(MemoryLocalArg {
                    memory_arg_idx: data.push64(Operand64::from(arg))?, local1: addr, local2: value })],
    }
    store64(tail, data, index: Operand128Idx<MemoryOperand>) {
        [Select64, _] => [SelectStore64(index)],
        [LocalGet32(addr) | LocalGet64(addr), LoadLocal64(arg), AddConst64(one), _]
            if (CompactMemoryArg::try_from(data.operand128(index)).ok().map(Operand64::from)
                == Some(data.operand64(arg.memory_arg_idx)) && addr == u16::from(arg.local1)
                && data.operand64(one).value() == 1) => [IncMemoryLocal64(arg)],
        [F64Mul, F64Add, _] if (let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index))) => [FMaStoreF64(arg)],
        [LocalGet32(addr), LocalGet64(value), _]
            if (let (Ok(addr), Ok(value), Ok(arg)) = (u8::try_from(addr), u8::try_from(value),
                CompactMemoryArg::try_from(data.operand128(index)))) => [StoreLocalLocal64(MemoryLocalArg {
                    memory_arg_idx: data.push64(Operand64::from(arg))?, local1: addr, local2: value })],
    }
    memory_fill(tail, data, memory: u32) {
        [Const32(value), Const32(size), _] => [MemoryFillConst(data.push128(Operand128::<MemoryFillOperand>::new(memory, value as u8, size))?)],
    }
    global_get32(tail, data, dst: u32) { [GlobalSet32(src), _] if (src == dst) => [GlobalTee32(src)] }
    global_get64(tail, data, dst: u32) { [GlobalSet64(src), _] if (src == dst) => [GlobalTee64(src)] }
    global_get128(tail, data, dst: u32) { [GlobalSet128(src), _] if (src == dst) => [GlobalTee128(src)] }
    local_get32(tail, data, dst: u16) { [LocalSet32(src), _] if (src == dst) => [LocalTee32(src)] }
    local_get64(tail, data, dst: u16) { [LocalSet64(src), _] if (src == dst) => [LocalTee64(src)] }
    local_get128(tail, data, dst: u16) { [LocalSet128(src), _] if (src == dst) => [LocalTee128(src)] }
    local_set32(tail, data, dst: u16) {
        [I32Mul, BinOpStackLocal32(BinOp::IAdd, acc), _] if (acc == dst) => [MulAccLocal32(dst)],
        [F32Mul, BinOpStackLocal32(BinOp::FAdd, acc), _] if (acc == dst) => [FMulAccLocal32(dst)],
        [LocalGet32(src), _] if (src == dst) => [],
        [LocalGet32(src), _] => [LocalCopy32(src, dst)],
        [Const32(value), _] => [SetLocalConst32(I32LocalArg { value, local: dst })],
        [BinOpLocalLocal32(BinOp::IAdd, left, right), _] => [AddLocalLocalSet32(LocalTripleArg { left, right, dst })],
        [BinOpLocalLocal32(op, left, right), _] => [BinOpLocalLocalSet32(PackedOp::new(op, data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [instruction, _] if (let Some((op, src, value)) = local_const32(data, instruction)) => [{
            if src == dst && let Some(delta) = op.inc_delta(value) {
                IncLocal32(I32LocalArg { value: delta, local: dst })
            } else { local_const_set32(data, op, src, dst, value, false)? }
        }],
        [BinOpStackLocal32(op, local), _] => [BinOpStackLocalSet32(op, local, dst)],
        [LoadLocal32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalSet32(MemoryLocalArg { local2: dst, ..arg })],
        [LoadLocal8S32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalSet8S32(MemoryLocalArg { local2: dst, ..arg })],
        [LoadLocal8U32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalSet8U32(MemoryLocalArg { local2: dst, ..arg })],
        [LoadLocal16S32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalSet16S32(MemoryLocalArg { local2: dst, ..arg })],
        [LoadLocal16U32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalSet16U32(MemoryLocalArg { local2: dst, ..arg })],
    }
    local_set64(tail, data, dst: u16) {
        [I64Mul, LocalGet64(acc), I64Add, _] if (acc == dst) => [MulAccLocal64(dst)],
        [F64Mul, LocalGet64(acc), F64Add, _] if (acc == dst) => [FMulAccLocal64(dst)],
        [LocalGet64(src), _] if (src == dst) => [],
        [LocalGet64(src), _] => [LocalCopy64(src, dst)],
        [constant @ (Const64(_) | Const64Imm(_)), _] => [SetLocalConst64(PackedOp::new(dst, pool64(data, constant)?))],
        [BinOpLocalLocal64(op, left, right), _] => [BinOpLocalLocalSet64(PackedOp::new(op, data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [instruction, _] if (let Some((op, src, value)) = local_const64(data, instruction)) => [{
            if src == dst && matches!(op, BinOp::IAdd | BinOp::ISub) {
                let delta = if op == BinOp::IAdd { value } else { value.wrapping_neg() };
                IncLocal64(PackedOp::new(dst, data.push64(Operand64::<i64>::new(delta))?))
            } else { local_const_set64(data, op, src, dst, value, false)? }
        }],
    }
    local_set128(tail, data, dst: u16) {
        [LocalGet32(local), V128Load(index), _] if (let (Ok(local), Ok(dst), Ok(arg)) =
            (u8::try_from(local), u8::try_from(dst), CompactMemoryArg::try_from(data.operand128(index)))) =>
            [LoadLocalSet128(MemoryLocalArg { memory_arg_idx: data.push64(Operand64::from(arg))?, local1: local, local2: dst })],
        [LocalGet128(src), _] if (src == dst) => [],
        [LocalGet128(src), _] => [LocalCopy128(src, dst)],
        [constant @ (Const128(_) | Const128Imm(_)), _] => [SetLocalConst128(PackedOp::new(dst, pool128(data, constant)?))],
        [BinOpLocalLocal128(op, left, right), _] => [BinOpLocalLocalSet128(PackedOp::new(op, data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [instruction, _] if (let Some((op, src, value)) = local_const128(data, instruction)) =>
            [local_const_set128(data, op, src, dst, value, false)?],
    }
    local_tee32(tail, data, dst: u16) {
        [Const32(value), I32And, _] => [AndConstTee32(I32LocalArg { value, local: dst })],
        [Const32(value), I32Sub, _] => [SubConstTee32(I32LocalArg { value, local: dst })],
        [AndConst32(value), _] => [AndConstTee32(I32LocalArg { value, local: dst })],
        [BinOpStackConst32(BinOp::ISub, value), _] => [SubConstTee32(I32LocalArg { value, local: dst })],
        [LocalGet32(src), _] if (src == dst) => [LocalGet32(src)],
        [LocalGet32(src), _] => [LocalCopy32(src, dst), LocalGet32(dst)],
        [BinOpLocalLocal32(BinOp::IAdd, left, right), _] => [AddLocalLocalTee32(LocalTripleArg { left, right, dst })],
        [BinOpLocalLocal32(op, left, right), _] => [BinOpLocalLocalTee32(PackedOp::new(op, data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [instruction, _] if (let Some((op, src, value)) = local_const32(data, instruction)) =>
            [local_const_set32(data, op, src, dst, value, true)?],
        [BinOpStackLocal32(op, local), _] => [BinOpStackLocalTee32(op, local, dst)],
        [LoadLocal32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalTee32(MemoryLocalArg { local2: dst, ..arg })],
        [LoadLocal8S32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalTee8S32(MemoryLocalArg { local2: dst, ..arg })],
        [LoadLocal8U32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalTee8U32(MemoryLocalArg { local2: dst, ..arg })],
        [LoadLocal16S32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalTee16S32(MemoryLocalArg { local2: dst, ..arg })],
        [LoadLocal16U32(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [LoadLocalTee16U32(MemoryLocalArg { local2: dst, ..arg })],
    }
    local_tee64(tail, data, dst: u16) {
        [constant @ (Const64(_) | Const64Imm(_)), I64And, _] => [AndConstTee64(PackedOp::new(dst, pool64(data, constant)?))],
        [constant @ (Const64(_) | Const64Imm(_)), I64Sub, _] => [SubConstTee64(PackedOp::new(dst, pool64(data, constant)?))],
        [BinOpStackConst64(packed), _] if (packed.op == BinOp::IAnd) => [AndConstTee64(PackedOp::new(dst, packed.index))],
        [BinOpStackConst64(packed), _] if (packed.op == BinOp::ISub) => [SubConstTee64(PackedOp::new(dst, packed.index))],
        [LocalGet64(src), _] if (src == dst) => [LocalGet64(src)],
        [LocalGet64(src), _] => [LocalCopy64(src, dst), LocalGet64(dst)],
        [BinOpLocalLocal64(op, left, right), _] => [BinOpLocalLocalTee64(PackedOp::new(op, data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [instruction, _] if (let Some((op, src, value)) = local_const64(data, instruction)) => [local_const_set64(data, op, src, dst, value, true)?],
    }
    local_tee128(tail, data, dst: u16) {
        [LocalGet32(local), V128Load(index), _] if (let (Ok(local), Ok(dst), Ok(arg)) =
            (u8::try_from(local), u8::try_from(dst), CompactMemoryArg::try_from(data.operand128(index)))) =>
            [LoadLocalTee128(MemoryLocalArg { memory_arg_idx: data.push64(Operand64::from(arg))?, local1: local, local2: dst })],
        [LocalGet128(src), _] if (src == dst) => [LocalGet128(src)],
        [LocalGet128(src), _] => [LocalCopy128(src, dst), LocalGet128(dst)],
        [BinOpLocalLocal128(op, left, right), _] => [BinOpLocalLocalTee128(PackedOp::new(op, data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [instruction, _] if (let Some((op, src, value)) = local_const128(data, instruction)) => [local_const_set128(data, op, src, dst, value, true)?],
    }
    drop32(tail, data) {
        [LocalTee32(local), _] => [LocalSet32(local)],
        [BinOpStackLocalTee32(op, local, dst), _] => [BinOpStackLocalSet32(op, local, dst)],
        [AddLocalLocalTee32(arg), _] => [AddLocalLocalSet32(arg)],
        [BinOpLocalLocalTee32(packed), _] => [BinOpLocalLocalSet32(packed)],
        [BinOpLocalConstTee32(packed), _] => [BinOpLocalConstSet32(packed)],
    }
    drop64(tail, data) {
        [LocalTee64(local), _] => [LocalSet64(local)],
        [BinOpLocalLocalTee64(packed), _] => [BinOpLocalLocalSet64(packed)],
        [BinOpLocalConstTee64(packed), _] => [BinOpLocalConstSet64(packed)],
    }
    drop128(tail, data) {
        [LocalTee128(local), _] => [LocalSet128(local)],
        [BinOpLocalLocalTee128(packed), _] => [BinOpLocalLocalSet128(packed)],
        [BinOpLocalConstTee128(packed), _] => [BinOpLocalConstSet128(packed)],
    }
    load(tail, data, instruction: impl FnOnce(MemoryLocalArg) -> Instruction, index: Operand128Idx<MemoryOperand>) {
        [LocalGet32(local) | LocalGet64(local), _]
            if (let Ok(local) = u8::try_from(local)
                && let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index))) =>
            [instruction(MemoryLocalArg {
                memory_arg_idx: data.push64(Operand64::from(arg))?, local1: local, local2: 0 })],
    }
    store128(tail, data, index: Operand128Idx<MemoryOperand>) {
        [LocalGet32(addr), LocalGet128(value), _]
            if (let (Ok(addr), Ok(value)) = (u8::try_from(addr), u8::try_from(value))
                && let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index))) =>
            [StoreLocalLocal128(MemoryLocalArg {
                memory_arg_idx: data.push64(Operand64::from(arg))?, local1: addr, local2: value })],
    }
    conditional(tail, data, on_zero: bool, target: u32) {
        [BinOpLocalConstTee32(packed), _] if (data.operand64(packed.index).a() == data.operand64(packed.index).b()) => [{
            let value = data.operand64(packed.index);
            update_jump(data, target, value.c() as i32, u32::from(value.a()), packed.op, on_zero, false)?
        }],
        [BinOpGlobalConst32(packed), GlobalTee32(dst), _] if (data.operand64(packed.index).a() == dst) => [update_jump(data, target, data.operand64(packed.index).b() as i32, dst, packed.op, on_zero, true)?],
        [AddConst32(value), LocalTee32(local), LocalGet32(cond), _] if (local == cond) => [IncStackTeeLocalJump32(data.push_target128(Operand128::<LocalUpdateOperand>::new(target, value, local, on_zero))?)],
        [XorConst32(value), LocalTee32(local), LocalGet32(cond), _] if (local == cond) => [stack_update_jump(data, target, value, local, BinOp::IXor, on_zero)?],
        [ShrUConst32(value), LocalTee32(local), LocalGet32(cond), _] if (local == cond) => [stack_update_jump(data, target, value, local, BinOp::IShrU, on_zero)?],
        [BinOpStackConst32(op, value), LocalTee32(local), LocalGet32(cond), _] if (local == cond) => [stack_update_jump(data, target, value, local, op, on_zero)?],
        [AndConstTee32(arg), LocalGet32(cond), _] if (arg.local == cond) => [stack_update_jump(data, target, arg.value, arg.local, BinOp::IAnd, on_zero)?],
        [SubConstTee32(arg), LocalGet32(cond), _] if (arg.local == cond) =>
            [IncStackTeeLocalJump32(data.push_target128(Operand128::<LocalUpdateOperand>::new(
                target, arg.value.wrapping_neg(), arg.local, on_zero))?)],
        [BinOpLocalConstTee32(packed), LocalGet32(right), raw_cmp, _]
            if (let Some(cmp) = cmp_op(raw_cmp)
                && data.operand64(packed.index).a() == data.operand64(packed.index).b()) => [{
                let value = data.operand64(packed.index);
                let cmp = if on_zero { cmp.inverse() } else { cmp };
                if let Some(delta) = packed.op.inc_delta(value.c() as i32) {
                    IncLocalJumpCmpLocal32(PackedOp::new(cmp, data.push_target128(
                        Operand128::<LocalUpdateCmpOperand>::new(target, delta, value.a(), right))?))
                } else {
                    BinOpLocalConstJumpCmpLocal32(PackedOp::new((packed.op, cmp), data.push_target128(
                        Operand128::<LocalUpdateCmpOperand>::new(target, value.c() as i32, value.a(), right))?))
                }
            }],
        [LocalGet32(local), I32Eqz, _] => [local_jump(target, local, !on_zero, false)],
        [LocalGet64(local), I64Eqz, _] => [local_jump(target, local, !on_zero, true)],
        [CmpLocalLocal32(op, left, right), I32Eqz, _] => [jump_cmp_local_local(data, target, left, right, if on_zero { op } else { op.inverse() }, false)?],
        [I32Eqz, _] => [if on_zero { JumpIfNonZero32(target) } else { JumpIfZero32(target) }],
        [I64Eqz, _] => [if on_zero { JumpIfNonZero64(target) } else { JumpIfZero64(target) }],
        [CmpLocalLocal32(op, left, right), _] => [jump_cmp_local_local(data, target, left, right, if on_zero { op.inverse() } else { op }, false)?],
        [CmpLocalLocal64(op, left, right), _] => [jump_cmp_local_local(data, target, left, right, if on_zero { op.inverse() } else { op }, true)?],
        [LocalGet32(local), _] => [local_jump(target, local, on_zero, false)],
        [LocalGet64(local), _] => [local_jump(target, local, on_zero, true)],
        [LocalGet32(local), Const32(value), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_local_const32(data, target, local, value, op)?],
        [LocalGet64(local), constant, raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)
            && let Some(value) = const64(data, constant) && let Ok(value) = i32::try_from(value)) =>
            [jump_cmp_local_const64(data, target, local, value, op)?],
        [LocalGet32(left), LocalGet32(right), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_local_local(data, target, left, right, op, false)?],
        [LocalGet64(left), LocalGet64(right), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_local_local(data, target, left, right, op, true)?],
        [LocalGet32(local), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_stack_local(data, target, local, op, false)?],
        [LocalGet64(local), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_stack_local(data, target, local, op, true)?],
        [Const32(value), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_stack_const32(data, target, value, op)?],
        [constant, raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero) && let Some(value) = const64(data, constant)) => [jump_cmp_stack_const64(data, target, value, op)?],
    }
}
