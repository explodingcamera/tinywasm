//! Ordered rule families selected during streaming lowering.
//! Scalar arithmetic uses ordinary accumulators and bounded BinOp operand forms.
//! Local-only forms avoid transient accumulator loads and operand-stack slots.

mod encoding;
use crate::{Result, emitter::PendingTail, visit::FunctionDataBuilder};
use encoding::*;
use tinywasm_types::{Instruction, Instruction::*, *};

/// Applies binary rules, then tries an earlier local or constant.
pub(crate) fn binary32(tail: &mut PendingTail, data: &mut FunctionDataBuilder, op: BinOp) -> Result<()> {
    let original = tail.suffix::<1>();
    binary32_rules(tail, data, op)?;
    if matches!(op, BinOp::IAdd | BinOp::IMul | BinOp::IAnd | BinOp::IOr | BinOp::IXor)
        && tail.suffix::<1>() == original
        && tail.move_read_to_rhs(ValueLane::S32, data)
    {
        binary32_rules(tail, data, op)?;
    }
    Ok(())
}

/// Applies binary rules, then tries an earlier local or constant.
pub(crate) fn binary64(tail: &mut PendingTail, data: &mut FunctionDataBuilder, op: BinOp) -> Result<()> {
    let original = tail.suffix::<1>();
    binary64_rules(tail, data, op)?;
    if matches!(op, BinOp::IAdd | BinOp::IMul | BinOp::IAnd | BinOp::IOr | BinOp::IXor)
        && tail.suffix::<1>() == original
        && tail.move_read_to_rhs(ValueLane::S64, data)
    {
        binary64_rules(tail, data, op)?;
    }
    Ok(())
}

/// Normalizes boolean negations before selecting the strongest conditional rule.
pub(crate) fn conditional(
    tail: &mut PendingTail,
    data: &mut FunctionDataBuilder,
    mut on_zero: bool,
    target: u32,
) -> Result<()> {
    while let Some([I32Eqz, _]) = tail.suffix::<2>() {
        on_zero = !on_zero;
        tail.replace::<2, 1>([if on_zero { JumpIfZero32(target) } else { JumpIfNonZero32(target) }]);
    }
    conditional_rules(tail, data, on_zero, target)
}

// Patterns include the incoming instruction. First match wins, including identity replacements.
macro_rules! families {
    ($($name:ident($tail:ident, $data:ident $(, $arg:ident: $ty:ty)*) {
        $([$($pattern:pat),+ $(,)?] $(if ($($guard:tt)+))? => $replacement:expr),+ $(,)?
    })*) => {$(
        #[doc = concat!("Applies the ordered `", stringify!($name), "` rules to the pending suffix.")]
        #[inline]
        pub(crate) fn $name($tail: &mut PendingTail, $data: &mut FunctionDataBuilder $(, $arg: $ty)*) -> Result<()> {
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
    binary32_rules(tail, data, op: BinOp) {
        [Const32(left), Const32(right), _]
            if (let Some(value) = fold32(op, left as u32, right as u32)) => [Const32(value as i32)],
        [I32Add, _] if (op == BinOp::IAdd) => [I32Add3],
        [I32Xor, Const32(value), _] if (matches!(op, BinOp::IRotl | BinOp::IRotr)) =>
            [I32XorRotlConst((if op == BinOp::IRotr { value.wrapping_neg() } else { value }) as u8 & 31)],
        [BinOpLocal32(BinOp::IXor, local), Const32(value), _] if (matches!(op, BinOp::IRotl | BinOp::IRotr)) =>
            [I32XorLocalRotlConst(local, (if op == BinOp::IRotr { value.wrapping_neg() } else { value }) as u8 & 31)],
        [BinOpLocalLocal32(BinOp::IXor, left, right), Const32(value), _] if (matches!(op, BinOp::IRotl | BinOp::IRotr)) =>
            [I32XorLocalLocalRotlConst(left, right, (if op == BinOp::IRotr { value.wrapping_neg() } else { value }) as u8 & 31)],
        [expr @ (BinOpLocalConstCompact32(_) | BinOpLocalConst32(_)), I32ShlConst(shift), _]
            if (op == BinOp::IAdd && let Some((BinOp::IAnd, local, mask)) = local_const32(data, expr)) =>
            [I32AddLocalAndShl(PackedOp::new((shift & 31) as u8,
                data.push64(Operand64::<(u16, u32)>::new(local, mask))?))],
        [load @ (I32LoadLocal(_) | I32LoadLocalCompact(_) | I32Load8SLocal(_) | I32Load8ULocal(_) | I32Load16SLocal(_) | I32Load16ULocal(_)), LocalGet32(right), _]
            if (let Some((kind, arg)) = local_load32(data, load)?) => [LoadLocalBinOp32(PackedOp::new((kind, op),
                data.push64(Operand64::<(u8, u16, u32)>::new(arg.local1, right, arg.memory_arg_idx.index() as u32))?))],
        [load @ (I32LoadLocal(_) | I32LoadLocalCompact(_) | I32Load8SLocal(_) | I32Load8ULocal(_) | I32Load16SLocal(_) | I32Load16ULocal(_)), _]
            if (let Some((kind, arg)) = local_load32(data, load)?) => [BinOpLoadLocalRhs32(PackedOp::new((kind, op),
                data.push64(Operand64::<(u8, u16, u32)>::new(arg.local1, 0, arg.memory_arg_idx.index() as u32))?))],
        [expr @ (BinOpLocalConstCompact32(_) | BinOpLocalConst32(_)), _]
            if (op == BinOp::IAdd && let Some((BinOp::IShl, local, shift)) = local_const32(data, expr)) =>
            [I32AddLocalShl(local, (shift & 31) as u8)],
        [I32AddLocalConst(arg), _] => [BinOpLocalConstRhs32(PackedOp::new((op, BinOp::IAdd),
            data.push64(Operand64::<(u16, u32)>::new(arg.local, arg.value as u32))?))],
        [BinOpLocalConstCompact32(arg), _] => [BinOpLocalConstRhs32(PackedOp::new((op, arg.op),
            data.push64(Operand64::<(u16, u32)>::new(u16::from(arg.local), arg.value))?))],
        [BinOpLocalConst32(arg), _] => [BinOpLocalConstRhs32(PackedOp::new((op, arg.op), arg.index))],
        [BinOpLocalLocal32(inner, left, right), _] => [BinOpLocalLocalRhs32(op, inner, left, right)],
        [LocalGet32(left), LocalGet32(right), _] => [BinOpLocalLocal32(op, left, right)],
        [LocalGet32(local), Const32(value), _] if (op == BinOp::IAdd) =>
            [I32AddLocalConst(I32LocalArg { local, value })],
        [Const32(value), LocalGet32(local), _] if (op == BinOp::IAdd) =>
            [I32AddLocalConst(I32LocalArg { local, value })],
        [LocalGet32(local), Const32(value), _] if (let Ok(local) = u8::try_from(local)) =>
            [BinOpLocalConstCompact32(BinOpLocalCompact32 { op, local, value: value as u32 })],
        [Const32(value), LocalGet32(local), _] if (matches!(op, BinOp::IAdd | BinOp::IMul | BinOp::IAnd | BinOp::IOr | BinOp::IXor)
            && let Ok(local) = u8::try_from(local)) =>
            [BinOpLocalConstCompact32(BinOpLocalCompact32 { op, local, value: value as u32 })],
        [LocalGet32(local), Const32(value), _] => [BinOpLocalConst32(PackedOp::new(op,
            data.push64(Operand64::<(u16, u32)>::new(local, value as u32))?))],
        [Const32(value), LocalGet32(local), _] if (matches!(op, BinOp::IAdd | BinOp::IMul | BinOp::IAnd | BinOp::IOr | BinOp::IXor)) =>
            [BinOpLocalConst32(PackedOp::new(op, data.push64(Operand64::<(u16, u32)>::new(local, value as u32))?))],
        [LocalGet32(local), _] if (op == BinOp::IAdd) => [I32AddLocal(local)],
        [LocalGet32(local), _] => [BinOpLocal32(op, local)],
        [Const32(value), _] if (op == BinOp::IAdd) => [I32AddConst(value)],
        [Const32(value), _] if (op == BinOp::ISub) => [I32AddConst(value.wrapping_neg())],
        [Const32(value), _] if (op == BinOp::IAnd) => [I32AndConst(value)],
        [Const32(value), _] if (op == BinOp::IXor) => [I32XorConst(value)],
        [Const32(value), _] if (op == BinOp::IShl) => [I32ShlConst(value)],
        [Const32(value), _] if (op == BinOp::IShrU) => [I32ShrUConst(value)],
        [Const32(value), _] => [BinOpConst32(op, value)],
    }
    binary64_rules(tail, data, op: BinOp) {
        [left @ (Const64(_) | Const64Imm(_)), right @ (Const64(_) | Const64Imm(_)), _]
            if (let (Some(left), Some(right)) = (const64(data, left), const64(data, right))
                && let Some(value) = fold64(op, left as u64, right as u64)) => [{
                let value = value as i64;
                match i32::try_from(value) {
                    Ok(value) => Const64Imm(value),
                    Err(_) => Const64(data.push64(Operand64::<i64>::new(value))?),
                }
            }],
        [I64Add, _] if (op == BinOp::IAdd) => [I64Add3],
        [I64LoadLocal(arg), _] if (op == BinOp::FAdd) => [F64AddLoadLocal(arg)],
        [I64LoadLocal(arg), _] if (op == BinOp::FSub) => [F64SubLoadLocal(arg)],
        [I64Xor, constant, _] if (matches!(op, BinOp::IRotl | BinOp::IRotr) && let Some(value) = const64(data, constant)) =>
            [I64XorRotlConst((if op == BinOp::IRotr { value.wrapping_neg() } else { value }) as u8 & 63)],
        [BinOpLocal64(BinOp::IXor, local), constant, _] if (matches!(op, BinOp::IRotl | BinOp::IRotr) && let Some(value) = const64(data, constant)) =>
            [I64XorLocalRotlConst(local, (if op == BinOp::IRotr { value.wrapping_neg() } else { value }) as u8 & 63)],
        [BinOpLocalLocal64(BinOp::IXor, left, right), constant, _] if (matches!(op, BinOp::IRotl | BinOp::IRotr) && let Some(value) = const64(data, constant)) =>
            [I64XorLocalLocalRotlConst(left, right, (if op == BinOp::IRotr { value.wrapping_neg() } else { value }) as u8 & 63)],
        [BinOpLocalConst64(mask), BinOpConst64(shift), _]
            if (op == BinOp::IAdd && mask.op == BinOp::IAnd && shift.op == BinOp::IShl) =>
            [I64AddLocalAndShl(PackedOp::new((data.operand64(shift.index).value() & 63) as u8, mask.index))],
        [BinOpLocalConst64(arg), _] => [BinOpLocalConstRhs64(PackedOp::new((op, arg.op), arg.index))],
        [BinOpLocalLocal64(inner, left, right), _] => [BinOpLocalLocalRhs64(op, inner, left, right)],
        [LocalGet64(left), LocalGet64(right), _] => [BinOpLocalLocal64(op, left, right)],
        [LocalGet64(local), constant, _] if (let Some(value) = const64(data, constant)) =>
            [BinOpLocalConst64(PackedOp::new(op, data.push128(Operand128::<(u16, u64)>::new(local, value as u64))?))],
        [constant, LocalGet64(local), _] if (matches!(op, BinOp::IAdd | BinOp::IMul | BinOp::IAnd | BinOp::IOr | BinOp::IXor)
            && let Some(value) = const64(data, constant)) =>
            [BinOpLocalConst64(PackedOp::new(op, data.push128(Operand128::<(u16, u64)>::new(local, value as u64))?))],
        [LocalGet64(local), _] => [BinOpLocal64(op, local)],
        [constant @ (Const64(_) | Const64Imm(_)), _] if (op == BinOp::IAdd) => [I64AddConst(pool64(data, constant)?)],
        [constant @ (Const64(_) | Const64Imm(_)), _]
            if (op == BinOp::ISub && let Some(value) = const64(data, constant)) =>
            [I64AddConst(data.push64(Operand64::<i64>::new(value.wrapping_neg()))?)],
        [constant @ (Const64(_) | Const64Imm(_)), _] => [BinOpConst64(PackedOp::new(op, pool64(data, constant)?))],
    }
    vector_or(tail, data) {
        [LocalTee128(saved), Const32(left), I64x2Shl, LocalGet128(local), Const32(right), I64x2ShrU, V128Or]
            if (saved == local && (left.wrapping_add(right) & 63) == 0) =>
            [LocalTee128(saved), I64x2RotlConst((left & 63) as u8)],
    }
    compare(tail, data, op: CmpOp) {
        [Const32(left), Const32(right), _] =>
            [Const32(i32::from(compare32(op, left as u32, right as u32)))],
        [left @ (Const64(_) | Const64Imm(_)), right @ (Const64(_) | Const64Imm(_)), _]
            if (let (Some(left), Some(right)) = (const64(data, left), const64(data, right))) =>
            [Const32(i32::from(compare64(op, left as u64, right as u64)))],
        [LocalGet32(a), LocalGet32(b), _] => [CmpLocalLocal32(op, a, b)],
        [LocalGet64(a), LocalGet64(b), _] => [CmpLocalLocal64(op, a, b)],
    }
    eqz32(tail, data) {
        [Const32(value), _] => [Const32(i32::from(value == 0))],
    }
    eqz64(tail, data) {
        [constant @ (Const64(_) | Const64Imm(_)), _] if (let Some(value) = const64(data, constant)) =>
            [Const32(i32::from(value == 0))],
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
    store32(tail, data, memory: MemoryArg) {
        [F32Mul, F32Add, _] => [F32MulAddStore(memory)],
        [I32LoadLocalSet(old), LocalGet32(address) | LocalGet64(address), LocalGet32(value), _]
            if (address == u16::from(old.local1) && (memory.address64 || old.local2 != old.local1)
                && compact_memory(data, memory) == Some(data.operand64(old.memory_arg_idx))) =>
            [UpdateMemoryLocal32(PackedOp::new(MemoryUpdateOp::Replace,
                data.push128(Operand128::<MemoryUpdateOperand>::new(data.operand64(old.memory_arg_idx), old.local1, value, u16::from(old.local2)))?))],
        [LocalGet32(address) | LocalGet64(address), LoadLocalBinOp32(packed), _]
            if (packed.op.0 == LoadKind32::U32 && let arg = data.operand64(packed.index)
                && address == u16::from(arg.a()) && let index = Operand64Idx::<CompactMemoryOperand>::new(arg.c())
                && compact_memory(data, memory) == Some(data.operand64(index))) =>
            [UpdateMemoryLocal32(PackedOp::new(MemoryUpdateOp::BinOp(packed.op.1), data.push128(
                Operand128::<MemoryUpdateOperand>::new(data.operand64(index), arg.a(), arg.b(), 0))?))],
        [Select32, _] => [SelectStore32(memory)],
        [LocalGet32(addr) | LocalGet64(addr), load @ (I32LoadLocal(_) | I32LoadLocalCompact(_)), I32AddConst(1), _]
            if (let Some((_, arg)) = local_load32(data, load)?
                && compact_memory(data, memory) == Some(data.operand64(arg.memory_arg_idx))
                && addr == u16::from(arg.local1)) => [IncMemoryLocal32(arg)],
        [LocalGet32(addr) | LocalGet64(addr), LocalGet32(value), _]
            if (let Some(store) = scalar_store_locals(data, memory, addr, value, ValueLane::S32)?) => [store],
        [LocalGet32(address) | LocalGet64(address), value, _]
            if (let Some((set, result)) = stored_result(data, value, address, memory, ValueLane::S32)
                && let Some(store) = scalar_store_locals(data, memory, address, result, ValueLane::S32)?) =>
            [set, store],
    }
    store64(tail, data, memory: MemoryArg) {
        // The load and tee touch only the value lane after consuming the source address.
        // Delay rereading the store address so it survives in the other accumulator.
        [address @ LocalGet32(_), source @ I32AddLocalConst(_), load @ (I64Load(arg) | F64Load(arg)), tee @ LocalTee64(_), store]
            if (!memory.address64 && !arg.address64) => [source, load, tee, address, store],
        [F64Mul, F64Add, _] => [F64MulAddStore(memory)],
        [Select64, _] => [SelectStore64(memory.memory_arg_idx)],
        [LocalGet32(addr) | LocalGet64(addr), I64LoadLocal(arg), I64AddConst(one), _]
            if (compact_memory(data, memory) == Some(data.operand64(arg.memory_arg_idx))
                && addr == u16::from(arg.local1)
                && data.operand64(one).value() == 1) => [IncMemoryLocal64(arg)],
        [LocalGet32(addr) | LocalGet64(addr), LocalGet64(value), _]
            if (let Some(store) = scalar_store_locals(data, memory, addr, value, ValueLane::S64)?) => [store],
        [LocalGet32(address) | LocalGet64(address), value, _]
            if (let Some((set, result)) = stored_result(data, value, address, memory, ValueLane::S64)
                && let Some(store) = scalar_store_locals(data, memory, address, result, ValueLane::S64)?) =>
            [set, store],
    }
    store_local(tail, data, make: fn(MemoryLocalArg) -> Instruction, width64: bool, memory: MemoryArg) {
        [BinOpLocalLocal32(BinOp::IAdd, dst, index), I32Load8ULocalAddCompact(0, src, src_index), I32Store8(_)]
            if (index == u16::from(src_index) && let Ok(dst) = u8::try_from(dst)
                && inline_memory_offset(data, memory) == Some(0)) =>
            [CopyByteLocalAdd(dst, src, src_index)],
        [LocalGet32(addr) | LocalGet64(addr), value @ (LocalGet32(_) | LocalGet64(_)), _]
            if (let local = match value { LocalGet32(local) if !width64 => Some(local), LocalGet64(local) if width64 => Some(local), _ => None }
                && let Some(value) = local
                && let Some(arg) = memory_locals(data, memory, addr, value)?) => [make(arg)],
        [LocalGet32(address) | LocalGet64(address), value, _]
            if (let Some((set, arg)) = stored_local_result(data, value, address, memory,
                if width64 { ValueLane::S64 } else { ValueLane::S32 })?) => [set, make(arg)],
    }
    memory_fill(tail, data, memory: u32) {
        [Const32(value), Const32(size), _] => [MemoryFillConst(data.push128(Operand128::<MemoryFillOperand>::new(memory, value as u8, size))?)],
    }
    global_get32(tail, data, dst: u32) { [GlobalSet32(src), _] if (src == dst) => [GlobalTee32(src)] }
    global_get64(tail, data, dst: u32) { [GlobalSet64(src), _] if (src == dst) => [GlobalTee64(src)] }
    global_get128(tail, data, dst: u32) { [GlobalSet128(src), _] if (src == dst) => [GlobalTee128(src)] }
    local_get32(tail, data, dst: u16) {
        [BinOpLocalSet32(op, local, target), _] if (target == dst) => [BinOpLocalTee32(op, local, dst)],
        [BinOpLocalLocalSet32(packed), _] if (data.operand64(packed.index).c() == dst) => [BinOpLocalLocalTee32(packed)],
        [BinOpLocalConstSet32(packed), _] if (data.operand64(packed.index).b() == dst) => [BinOpLocalConstTee32(packed)],
        [LocalSet32(src), _] if (src == dst) => [LocalTee32(src)],
    }
    local_get64(tail, data, dst: u16) {
        [BinOpLocalSet64(op, local, target), _] if (target == dst) => [BinOpLocalTee64(op, local, dst)],
        [BinOpLocalLocalSet64(packed), _] if (data.operand64(packed.index).c() == dst) => [BinOpLocalLocalTee64(packed)],
        [BinOpLocalConstSet64(packed), _] if (data.operand128(packed.index).b() == dst) => [BinOpLocalConstTee64(packed)],
        [LocalSet64(src), _] if (src == dst) => [LocalTee64(src)],
    }
    local_get128(tail, data, dst: u16) { [LocalSet128(src), _] if (src == dst) => [LocalTee128(src)] }
    local_set32(tail, data, dst: u16) {
        [I32Mul, I32AddLocal(acc), _] if (acc == dst) => [I32MulAddLocal(dst)],
        [F32Mul, BinOpLocal32(BinOp::FAdd, acc), _] if (acc == dst) => [F32MulAddLocal(dst)],
        [I32AddLocalConst(arg), _] if (arg.local == dst) => [I32AddLocalSetConst(arg)],
        [I32AddLocalConst(arg), _] => [BinOpLocalConstSet32(PackedOp::new(BinOp::IAdd,
            data.push64(Operand64::<(u16, u16, u32)>::new(arg.local, dst, arg.value as u32))?))],
        [LocalCopy32(first, target), LocalGet32(second), _]
            if (second != dst && let (Ok(first), Ok(target), Ok(second), Ok(dst)) =
                (u8::try_from(first), u8::try_from(target), u8::try_from(second), u8::try_from(dst))) =>
            [LocalCopyPair32(first, target, second, dst)],
        [BinOpLocalConstCompact32(arg), _] if (u16::from(arg.local) == dst && let Some(value) = arg.op.inc_delta(arg.value as i32)) =>
            [I32AddLocalSetConst(I32LocalArg { value, local: dst })],
        [BinOpLocalConstCompact32(arg), _] => [BinOpLocalConstSet32(PackedOp::new(arg.op,
            data.push64(Operand64::<(u16, u16, u32)>::new(u16::from(arg.local), dst, arg.value))?))],
        [BinOpLocalLocal32(op, left, right), _] => [BinOpLocalLocalSet32(PackedOp::new(op,
            data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [BinOpLocalConst32(packed), _] if (let arg = data.operand64(packed.index)
            && arg.a() == dst && let Some(value) = packed.op.inc_delta(arg.b() as i32)) => [I32AddLocalSetConst(I32LocalArg { value, local: dst })],
        [BinOpLocalConst32(packed), _] => [{ let arg = data.operand64(packed.index);
            BinOpLocalConstSet32(PackedOp::new(packed.op, data.push64(Operand64::<(u16, u16, u32)>::new(arg.a(), dst, arg.b()))?)) }],
        [BinOpLocal32(op, local), _] => [BinOpLocalSet32(op, local, dst)],
        [I32AddLocal(local), _] => [BinOpLocalSet32(BinOp::IAdd, local, dst)],
        [LocalGet32(src), I32AddConst(value), _] if (src == dst) => [I32AddLocalSetConst(I32LocalArg { value, local: dst })],
        [LocalGet32(src), _] if (src == dst) => [],
        [LocalGet32(src), _] => [LocalCopy32(src, dst)],
        [Const32(value), _] => [SetLocalConst32(I32LocalArg { value, local: dst })],
        [load @ (I32LoadLocal(_) | I32LoadLocalCompact(_)), _]
            if (let Ok(dst) = u8::try_from(dst) && let Some((_, arg)) = local_load32(data, load)?) =>
            [I32LoadLocalSet(MemoryLocalArg { local2: dst, ..arg })],
        [I32Load8SLocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [I32Load8SLocalSet(MemoryLocalArg { local2: dst, ..arg })],
        [I32Load8ULocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [I32Load8ULocalSet(MemoryLocalArg { local2: dst, ..arg })],
        [I32Load16SLocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [I32Load16SLocalSet(MemoryLocalArg { local2: dst, ..arg })],
        [I32Load16ULocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [I32Load16ULocalSet(MemoryLocalArg { local2: dst, ..arg })],
    }
    local_set64(tail, data, dst: u16) {
        [I64LoadLocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) =>
            [I64LoadLocalSet(MemoryLocalArg { local2: dst, ..arg })],
        [I64Mul, BinOpLocal64(BinOp::IAdd, acc), _] if (acc == dst) => [I64MulAddLocal(dst)],
        [F64Mul, BinOpLocal64(BinOp::FAdd, acc), _] if (acc == dst) => [F64MulAddLocal(dst)],
        [LocalCopy64(first, target), LocalGet64(second), _]
            if (second != dst && let (Ok(first), Ok(target), Ok(second), Ok(dst)) =
                (u8::try_from(first), u8::try_from(target), u8::try_from(second), u8::try_from(dst))) =>
            [LocalCopyPair64(first, target, second, dst)],
        [BinOpLocalLocal64(op, left, right), _] => [BinOpLocalLocalSet64(PackedOp::new(op,
            data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [BinOpLocalConst64(packed), _] if (let arg = data.operand128(packed.index)
            && arg.a() == dst && matches!(packed.op, BinOp::IAdd | BinOp::ISub)) => [{
                let value = if packed.op == BinOp::IAdd { arg.b() as i64 } else { (arg.b() as i64).wrapping_neg() };
                I64AddLocalSetConst(PackedOp::new(dst, data.push64(Operand64::<i64>::new(value))?))
            }],
        [BinOpLocalConst64(packed), _] => [{ let arg = data.operand128(packed.index);
            BinOpLocalConstSet64(PackedOp::new(packed.op, data.push128(Operand128::<(u16, u16, u64)>::new(arg.a(), dst, arg.b()))?)) }],
        [BinOpLocal64(op, local), _] => [BinOpLocalSet64(op, local, dst)],
        [LocalGet64(src), I64AddConst(value), _] if (src == dst) => [I64AddLocalSetConst(PackedOp::new(dst, value))],
        [LocalGet64(src), _] if (src == dst) => [],
        [LocalGet64(src), _] => [LocalCopy64(src, dst)],
        [constant @ (Const64(_) | Const64Imm(_)), _] => [SetLocalConst64(PackedOp::new(dst, pool64(data, constant)?))],
    }
    local_set128(tail, data, dst: u16) {
        [LocalGet32(local), V128Load(memory), _]
            if (!memory.address64 && let Some(arg) = memory_locals(data, memory, local, dst)?) => [V128LoadLocalSet(arg)],
        [LocalGet128(src), _] if (src == dst) => [],
        [LocalGet128(src), _] => [LocalCopy128(src, dst)],
        [constant @ (Const128(_) | Const128Imm(_)), _] => [SetLocalConst128(PackedOp::new(dst, pool128(data, constant)?))],
    }
    local_tee32(tail, data, dst: u16) {
        [I32AddConst(value), _] => [I32AddConstTee(I32LocalArg { value, local: dst })],
        [I32AddLocalConst(arg), _] => [BinOpLocalConstTee32(PackedOp::new(BinOp::IAdd,
            data.push64(Operand64::<(u16, u16, u32)>::new(arg.local, dst, arg.value as u32))?))],
        [BinOpLocalConstCompact32(arg), _] => [BinOpLocalConstTee32(PackedOp::new(arg.op,
            data.push64(Operand64::<(u16, u16, u32)>::new(u16::from(arg.local), dst, arg.value))?))],
        [BinOpLocalLocal32(op, left, right), _] => [BinOpLocalLocalTee32(PackedOp::new(op,
            data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [BinOpLocalConst32(packed), _] => [{ let arg = data.operand64(packed.index);
            BinOpLocalConstTee32(PackedOp::new(packed.op, data.push64(Operand64::<(u16, u16, u32)>::new(arg.a(), dst, arg.b()))?)) }],
        [BinOpLocal32(op, local), _] => [BinOpLocalTee32(op, local, dst)],
        [I32AddLocal(local), _] => [BinOpLocalTee32(BinOp::IAdd, local, dst)],
        [BinOpConst32(BinOp::ISub, value), _] => [I32AddConstTee(I32LocalArg { value: value.wrapping_neg(), local: dst })],
        [Const32(value), I32And, _] => [I32AndConstTee(I32LocalArg { value, local: dst })],
        [Const32(value), I32Sub, _] => [I32AddConstTee(I32LocalArg { value: value.wrapping_neg(), local: dst })],
        [I32AndConst(value), _] => [I32AndConstTee(I32LocalArg { value, local: dst })],
        [LocalGet32(src), _] if (src == dst) => [LocalGet32(src)],
        [LocalGet32(src), _] => [LocalCopy32(src, dst), LocalGet32(dst)],
        [load @ (I32LoadLocal(_) | I32LoadLocalCompact(_)), _]
            if (let Ok(dst) = u8::try_from(dst) && let Some((_, arg)) = local_load32(data, load)?) =>
            [I32LoadLocalTee(MemoryLocalArg { local2: dst, ..arg })],
        [I32Load8SLocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [I32Load8SLocalTee(MemoryLocalArg { local2: dst, ..arg })],
        [I32Load8ULocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [I32Load8ULocalTee(MemoryLocalArg { local2: dst, ..arg })],
        [I32Load16SLocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [I32Load16SLocalTee(MemoryLocalArg { local2: dst, ..arg })],
        [I32Load16ULocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) => [I32Load16ULocalTee(MemoryLocalArg { local2: dst, ..arg })],
    }
    local_tee64(tail, data, dst: u16) {
        [I64LoadLocal(arg), _] if (let Ok(dst) = u8::try_from(dst)) =>
            [I64LoadLocalTee(MemoryLocalArg { local2: dst, ..arg })],
        [I64AddConst(index), _] => [I64AddConstTee(PackedOp::new(dst, index))],
        [BinOpLocalLocal64(op, left, right), _] => [BinOpLocalLocalTee64(PackedOp::new(op,
            data.push64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?))],
        [BinOpLocalConst64(packed), _] => [{ let arg = data.operand128(packed.index);
            BinOpLocalConstTee64(PackedOp::new(packed.op, data.push128(Operand128::<(u16, u16, u64)>::new(arg.a(), dst, arg.b()))?)) }],
        [BinOpLocal64(op, local), _] => [BinOpLocalTee64(op, local, dst)],
        [BinOpConst64(packed), _] if (packed.op == BinOp::IAnd) => [I64AndConstTee(PackedOp::new(dst, packed.index))],
        [BinOpConst64(packed), _] if (packed.op == BinOp::ISub) => [I64AddConstTee(PackedOp::new(dst,
            data.push64(Operand64::<i64>::new(data.operand64(packed.index).value().wrapping_neg()))?))],
        [constant @ (Const64(_) | Const64Imm(_)), I64And, _] => [I64AndConstTee(PackedOp::new(dst, pool64(data, constant)?))],
        [constant @ (Const64(_) | Const64Imm(_)), I64Sub, _] if (let Some(value) = const64(data, constant)) =>
            [I64AddConstTee(PackedOp::new(dst, data.push64(Operand64::<i64>::new(value.wrapping_neg()))?))],
        [LocalGet64(src), _] if (src == dst) => [LocalGet64(src)],
        [LocalGet64(src), _] => [LocalCopy64(src, dst), LocalGet64(dst)],
    }
    local_tee128(tail, data, dst: u16) {
        [LocalGet32(local), V128Load(memory), _]
            if (!memory.address64 && let Some(arg) = memory_locals(data, memory, local, dst)?) => [V128LoadLocalTee(arg)],
        [LocalGet128(src), _] if (src == dst) => [LocalGet128(src)],
        [LocalGet128(src), _] => [LocalCopy128(src, dst), LocalGet128(dst)],
    }
    drop32(tail, data) {
        [value, _] if (matches!(value.value_effect(), InstructionValueEffect::Read(_))
            && matches!(value.effects(), InstructionEffects::Operands { output: Some(ValueLane::S32), .. })) => [],
        [value, _] if (matches!(value.effects(), InstructionEffects::Operands { output: Some(ValueLane::S32), .. })
            && let Some(set) = value.without_result()) => [set],
    }
    drop64(tail, data) {
        [value, _] if (matches!(value.value_effect(), InstructionValueEffect::Read(_))
            && matches!(value.effects(), InstructionEffects::Operands { output: Some(ValueLane::S64), .. })) => [],
        [value, _] if (matches!(value.effects(), InstructionEffects::Operands { output: Some(ValueLane::S64), .. })
            && let Some(set) = value.without_result()) => [set],
    }
    drop128(tail, data) {
        [value, _] if (matches!(value.value_effect(), InstructionValueEffect::Read(_))
            && matches!(value.effects(), InstructionEffects::Operands { output: Some(ValueLane::S128), .. })) => [],
        [value, _] if (matches!(value.effects(), InstructionEffects::Operands { output: Some(ValueLane::S128), .. })
            && let Some(set) = value.without_result()) => [set],
    }
    load(tail, data, instruction: impl FnOnce(MemoryLocalArg) -> Instruction,
        add_instruction: impl FnOnce(MemoryLocalArg) -> Instruction, memory: MemoryArg) {
        [BinOpLocalLocal32(BinOp::IAdd, left, right), I32Load8U(_)]
            if (let (Ok(left), Ok(right)) = (u8::try_from(left), u8::try_from(right))
                && let Some(offset) = inline_memory_offset(data, memory)) =>
            [I32Load8ULocalAddCompact(offset, left, right)],
        [address @ (BinOpLocalLocal32(BinOp::IAdd, left, right) | BinOpLocalLocal64(BinOp::IAdd, left, right)), _]
            if (matches!(address.effects(), InstructionEffects::Operands { output: Some(lane), .. }
                    if lane == if memory.address64 { ValueLane::S64 } else { ValueLane::S32 })
                && let Some(arg) = memory_locals(data, memory, left, right)?) =>
            [add_instruction(arg)],
        [LocalGet32(local), I32Load(_) | F32Load(_)]
            if (let Some(offset) = inline_memory_offset(data, memory)) =>
            [I32LoadLocalCompact(MemoryLocalCompactArg { local, offset })],
        [LocalGet32(local) | LocalGet64(local), _]
            if (let Some(arg) = memory_locals(data, memory, local, 0)?) => [instruction(arg)],
    }
    store128(tail, data, memory: MemoryArg) {
        [LocalGet32(addr) | LocalGet64(addr), LocalGet128(value), _]
            if (let Some(arg) = memory_locals(data, memory, addr, value)?) => [V128StoreLocalLocal(arg)],
        [LocalGet32(address) | LocalGet64(address), value, _]
            if (let Some((set, arg)) = stored_local_result(data, value, address, memory, ValueLane::S128)?) =>
            [set, V128StoreLocalLocal(arg)],
    }
    conditional_rules(tail, data, on_zero: bool, target: u32) {
        [constant, raw_cmp, _]
            if (let Some(jump) = float_const_jump(data, target, constant, raw_cmp, on_zero)?) => [jump],
        [I32AddConst(value), _] =>
            [jump_cmp_acc_const32(data, target, value.wrapping_neg(), if on_zero { CmpOp::Eq } else { CmpOp::Ne })?],
        [I32XorConst(value), _] =>
            [jump_cmp_acc_const32(data, target, value, if on_zero { CmpOp::Eq } else { CmpOp::Ne })?],
        [I64AddConst(index), I64Eqz, _] =>
            [jump_cmp_acc_const64(data, target, data.operand64(index).value().wrapping_neg(),
                if on_zero { CmpOp::Ne } else { CmpOp::Eq })?],
        [BinOpConst64(packed), I64Eqz, _] if (packed.op == BinOp::IXor) =>
            [jump_cmp_acc_const64(data, target, data.operand64(packed.index).value(),
                if on_zero { CmpOp::Ne } else { CmpOp::Eq })?],
        [expr @ (I32AddLocalConst(_) | BinOpLocalConstCompact32(_) | BinOpLocalConst32(_)), Const32(right), raw_cmp, _]
            if (let Some(cmp) = branch_cmp(raw_cmp, on_zero)
                && let Some((op, local, value)) = local_const32(data, expr)) =>
            [BinOpLocalConstJumpCmpConst32(PackedOp::new((op, cmp), data.push_target128(
                Operand128::<LocalExprCmpOperand>::new(target, local, value, right))?))],
        [expr @ (I32AddLocalConst(_) | BinOpLocalConstCompact32(_) | BinOpLocalConst32(_)), _]
            if (let Some((op, local, value)) = local_const32(data, expr)) =>
            [BinOpLocalConstTestJump32(PackedOp::new(op, data.push_target128(
                Operand128::<LocalUpdateOperand>::new(target, value as i32, local, on_zero))?))],
        [BinOpLocalLocal32(op, left, right), _] =>
            [BinOpLocalLocalTestJump32(PackedOp::new((op, on_zero), data.push_target64(
                Operand64::<(u32, u16, u16)>::new(target, left, right))?))],
        [LocalGet32(right), BinOpLocalConstTee32(packed), raw_cmp @ (I32Eq | I32Ne), _]
            if (let arg = data.operand64(packed.index) && arg.a() == arg.b() && right != arg.b()
                && let Some(value) = packed.op.inc_delta(arg.c() as i32) && let Some(op) = branch_cmp(raw_cmp, on_zero)) =>
            [I32AddLocalSetConstJumpCmpLocal(PackedOp::new(op,
                data.push_target128(Operand128::<LocalUpdateCmpOperand>::new(target, value, arg.b(), right))?))],
        [BinOpLocalConstTee32(packed), _] if (let arg = data.operand64(packed.index) && arg.a() == arg.b()) =>
            [update_jump(data, target, arg.c() as i32, u32::from(arg.b()), packed.op, on_zero, false)?],
        [I32LoadLocalTee(arg), _] => [I32LoadLocalTeeJump(PackedOp::new(on_zero,
            data.push_target128(Operand128::<MemoryLocalJumpOperand>::new(target, arg))?))],
        [LocalGet32(right), LocalGet32(src), I32AddConst(value), LocalTee32(dst), raw_cmp @ (I32Eq | I32Ne), _]
            if (src == dst && right != dst && let Some(op) = branch_cmp(raw_cmp, on_zero)) =>
            [I32AddLocalSetConstJumpCmpLocal(PackedOp::new(op,
                data.push_target128(Operand128::<LocalUpdateCmpOperand>::new(target, value, dst, right))?))],
        [LocalGet32(src), I32AddConst(value), LocalTee32(dst), _] if (src == dst) =>
            [update_jump(data, target, value, u32::from(dst), BinOp::IAdd, on_zero, false)?],
        [GlobalGet32(src), I32AddConst(value), GlobalTee32(dst), _] if (src == dst) =>
            [update_jump(data, target, value, dst, BinOp::IAdd, on_zero, true)?],
        [I32AddConst(value), LocalTee32(local), LocalGet32(cond), _] if (local == cond) => [I32AddStackConstTeeLocalJump(data.push_target128(Operand128::<LocalUpdateOperand>::new(target, value, local, on_zero))?)],
        [I32XorConst(value), LocalTee32(local), LocalGet32(cond), _] if (local == cond) => [stack_update_jump(data, target, value, local, BinOp::IXor, on_zero)?],
        [I32ShrUConst(value), LocalTee32(local), LocalGet32(cond), _] if (local == cond) => [stack_update_jump(data, target, value, local, BinOp::IShrU, on_zero)?],
        [I32AndConstTee(arg), LocalGet32(cond), _] if (arg.local == cond) => [stack_update_jump(data, target, arg.value, arg.local, BinOp::IAnd, on_zero)?],
        [I32AddConstTee(arg), LocalGet32(cond), _] if (arg.local == cond) =>
            [I32AddStackConstTeeLocalJump(data.push_target128(Operand128::<LocalUpdateOperand>::new(
                target, arg.value, arg.local, on_zero))?)],
        [BinOpLocalConstTee32(packed), LocalGet32(right), raw_cmp, _]
            if (let arg = data.operand64(packed.index) && arg.a() == arg.b()
                && let Some(cmp) = branch_cmp(raw_cmp, on_zero)) =>
            [BinOpLocalConstSetJumpCmpLocal32(PackedOp::new((packed.op, cmp),
                data.push_target128(Operand128::<LocalUpdateCmpOperand>::new(target, arg.c() as i32, arg.b(), right))?))],
        [BinOpLocalConstTee64(packed), LocalGet64(right), raw_cmp, _]
            if (let arg = data.operand128(packed.index) && arg.a() == arg.b()
                && let Some(cmp) = branch_cmp(raw_cmp, on_zero)) =>
            [BinOpLocalConstSetJumpCmpLocal64(PackedOp::new((packed.op, cmp),
                data.push_target128(Operand128::<LocalUpdateCmpOperand64>::new(target, arg.c() as i64, arg.b(), right))?))],
        [BinOpLocalLocalTee32(packed), LocalGet32(right), raw_cmp, _]
            if (let Some(cmp) = branch_cmp(raw_cmp, on_zero) && let arg = data.operand64(packed.index)) =>
            [BinOpLocalLocalSetJumpCmpLocal32(PackedOp::new((packed.op, cmp),
                data.push_target128(Operand128::<LocalBinOpCmpOperand>::new(target, arg.a(), arg.b(), arg.c(), right))?))],
        [BinOpLocalLocalTee64(packed), LocalGet64(right), raw_cmp, _]
            if (let Some(cmp) = branch_cmp(raw_cmp, on_zero) && let arg = data.operand64(packed.index)) =>
            [BinOpLocalLocalSetJumpCmpLocal64(PackedOp::new((packed.op, cmp),
                data.push_target128(Operand128::<LocalBinOpCmpOperand>::new(target, arg.a(), arg.b(), arg.c(), right))?))],
        [LocalGet64(local), I64Eqz, _] => [local_jump(target, local, !on_zero, true)],
        [I64Eqz, _] => [if on_zero { JumpIfNonZero64(target) } else { JumpIfZero64(target) }],
        [CmpLocalLocal32(op, left, right), _] => [jump_cmp_local_local(data, target, left, right, if on_zero { op.inverse() } else { op }, false)?],
        [CmpLocalLocal64(op, left, right), _] => [jump_cmp_local_local(data, target, left, right, if on_zero { op.inverse() } else { op }, true)?],
        [LocalGet32(local), _] => [local_jump(target, local, on_zero, false)],
        [LocalGet64(local), _] => [local_jump(target, local, on_zero, true)],
        [LocalGet32(local), Const32(value), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_local_const32(data, target, local, value, op)?],
        [LocalGet64(local), constant, raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)
            && let Some(value) = const64(data, constant)) =>
            [jump_cmp_local_const64(data, target, local, value, op)?],
        [LocalGet32(left), LocalGet32(right), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_local_local(data, target, left, right, op, false)?],
        [LocalGet64(left), LocalGet64(right), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_local_local(data, target, left, right, op, true)?],
        [LocalGet32(local), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_acc_local(data, target, local, op, false)?],
        [LocalGet64(local), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_acc_local(data, target, local, op, true)?],
        [Const32(value), raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero)) => [jump_cmp_acc_const32(data, target, value, op)?],
        [constant, raw_cmp, _] if (let Some(op) = branch_cmp(raw_cmp, on_zero) && let Some(value) = const64(data, constant)) => [jump_cmp_acc_const64(data, target, value, op)?],
    }
}
