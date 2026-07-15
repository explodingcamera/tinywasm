use crate::macros::optimize::*;
use crate::{ParseError, ParserOptions, Result};
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};
use tinywasm_types::{BinOp, BinOp128, CmpOp, ConstIdx, Instruction, ValueCounts, WasmFunctionData};

pub(crate) struct OptimizeResult {
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) uses_local_memory: bool,
}

struct CompactOutput {
    instructions: Vec<Instruction>,
    block_start: usize,
    tail_rewritten: bool,
}

impl Deref for CompactOutput {
    type Target = Vec<Instruction>;

    fn deref(&self) -> &Self::Target {
        &self.instructions
    }
}

impl DerefMut for CompactOutput {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.instructions
    }
}

pub(crate) fn optimize_instructions(
    instructions: Vec<Instruction>,
    function_data: &mut WasmFunctionData,
    options: &ParserOptions,
    function_results: ValueCounts,
    self_func_addr: u32,
    imported_memory_count: u32,
) -> Result<OptimizeResult> {
    let (mut instructions, old_to_new) = if options.optimize_rewrite() {
        let boundaries = target_boundaries(&instructions, function_data)?;
        let (instructions, old_to_new) = rewrite(instructions, &boundaries, function_results, self_func_addr);
        (instructions, Some(old_to_new))
    } else {
        (instructions, None)
    };
    let uses_local_memory = finalize(&mut instructions, function_data, old_to_new.as_deref(), imported_memory_count)?;
    Ok(OptimizeResult { instructions, uses_local_memory })
}

fn rewrite(
    source: Vec<Instruction>,
    boundaries: &[bool],
    function_results: ValueCounts,
    self_func_addr: u32,
) -> (Vec<Instruction>, Vec<u32>) {
    use Instruction::*;
    let mut instrs =
        CompactOutput { instructions: Vec::with_capacity(source.len()), block_start: 0, tail_rewritten: false };
    let mut old_to_new = alloc::vec![0; source.len() + 1];
    let mut after_terminator = false;
    let return_instr = match function_results {
        ValueCounts { c32: 0, c64: 0, c128: 0 } => Some(ReturnVoid),
        ValueCounts { c32: 1, c64: 0, c128: 0 } => Some(Return32),
        ValueCounts { c32: 0, c64: 1, c128: 0 } => Some(Return64),
        ValueCounts { c32: 0, c64: 0, c128: 1 } => Some(Return128),
        _ => None,
    };

    for (old_idx, instr) in source.iter().copied().enumerate() {
        if boundaries[old_idx] || after_terminator {
            instrs.block_start = instrs.len();
        }
        old_to_new[old_idx] = instrs.len() as u32;
        instrs.tail_rewritten = false;
        instrs.push(instr);
        let mut i = instrs.len() - 1;
        match instrs[i] {
            LocalCopy32(a, b) if a == b => {
                instrs.pop();
            }
            LocalCopy64(a, b) if a == b => {
                instrs.pop();
            }
            LocalCopy128(a, b) if a == b => {
                instrs.pop();
            }
            Call(addr) if addr == self_func_addr => instrs[i] = CallSelf,
            ReturnCall(addr) if addr == self_func_addr => instrs[i] = ReturnCallSelf,
            Return if let Some(return_instr) = return_instr => instrs[i] = return_instr,
            instr @ (I32Add | I32Mul | I32And | I32Or | I32Xor) => {
                let Some(op) = int_bin_op(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), Const32(c)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [Const32(c), LocalGet32(local)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [GlobalGet(global)] => BinOpStackGlobal32(op, global));
                if matches!(op, BinOp::IAdd) {
                    rewrite!(instrs, i, [Const32(c)] => AddConst32(c));
                    rewrite!(instrs, i, [I32Add] => I32Add3);
                }
            }
            instr @ (I32Sub | I32Shl | I32ShrS | I32ShrU | I32Rotl | I32Rotr) => {
                let Some(op) = int_bin_op(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), Const32(c)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [GlobalGet(global)] => BinOpStackGlobal32(op, global));
                if matches!(op, BinOp::IShrS) {
                    rewrite!(instrs, i, [BinOpLocalConst32(BinOp::IShl, local, 8), Const32(8)] => [LocalGet32(local), I32Extend8S]);
                    rewrite!(instrs, i, [BinOpLocalConst32(BinOp::IShl, local, 16), Const32(16)] => [LocalGet32(local), I32Extend16S]);
                }
            }
            instr @ (I64Add | I64Mul | I64And | I64Or | I64Xor) => {
                let Some(op) = int_bin_op(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), Const64(c)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [Const64(c), LocalGet64(local)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [GlobalGet(global)] => BinOpStackGlobal64(op, global));
                if matches!(op, BinOp::IAdd) {
                    rewrite!(instrs, i, [Const64(c)] => AddConst64(c));
                    rewrite!(instrs, i, [I64Add] => I64Add3);
                }
            }
            instr @ (I64Sub | I64Shl | I64ShrS | I64ShrU | I64Rotl | I64Rotr) => {
                let Some(op) = int_bin_op(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), Const64(c)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [GlobalGet(global)] => BinOpStackGlobal64(op, global));
                if matches!(op, BinOp::IShrS) {
                    rewrite!(instrs, i, [BinOpLocalConst64(BinOp::IShl, local, 8), Const64(8)] => [LocalGet64(local), I64Extend8S]);
                    rewrite!(instrs, i, [BinOpLocalConst64(BinOp::IShl, local, 16), Const64(16)] => [LocalGet64(local), I64Extend16S]);
                    rewrite!(instrs, i, [BinOpLocalConst64(BinOp::IShl, local, 32), Const64(32)] => [LocalGet64(local), I64Extend32S]);
                }
            }
            instr @ (F32Add | F32Mul | F32Min | F32Max) => {
                let Some(op) = float_bin_op(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), Const32(c)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [Const32(c), LocalGet32(local)] => BinOpLocalConst32(op, local, c));
            }
            instr @ (F32Sub | F32Div | F32Copysign) => {
                let Some(op) = float_bin_op(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), Const32(c)] => BinOpLocalConst32(op, local, c));
            }
            instr @ (F64Add | F64Mul | F64Min | F64Max) => {
                let Some(op) = float_bin_op(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), Const64(c)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [Const64(c), LocalGet64(local)] => BinOpLocalConst64(op, local, c));
            }
            instr @ (F64Sub | F64Div | F64Copysign) => {
                let Some(op) = float_bin_op(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), Const64(c)] => BinOpLocalConst64(op, local, c));
            }
            instr @ (V128And | V128Or | V128Xor | I64x2Add | I64x2Mul) => {
                let Some(op) = bin_op_128(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet128(a), LocalGet128(b)] => BinOpLocalLocal128(op, a, b));
                rewrite!(instrs, i, [LocalGet128(local), Const128(c)] => BinOpLocalConst128(op, local, c));
                rewrite!(instrs, i, [Const128(c), LocalGet128(local)] => BinOpLocalConst128(op, local, c));
            }
            V128AndNot => {
                rewrite!(instrs, i, [LocalGet128(a), LocalGet128(b)] => BinOpLocalLocal128(BinOp128::AndNot, a, b));
                rewrite!(instrs, i, [LocalGet128(local), Const128(c)] => BinOpLocalConst128(BinOp128::AndNot, local, c));
            }
            I32Store(memarg) | F32Store(memarg) => {
                rewrite!(instrs, i, [F32Mul, F32Add] => FMaStoreF32(memarg));
                rewrite!(instrs, i,
                    [LocalGet32(addr_local), LocalGet32(value_local)] if
                    (let (Ok(addr_local), Ok(value_local)) = (u8::try_from(addr_local), u8::try_from(value_local))) =>
                    StoreLocalLocal32(memarg, addr_local, value_local)
                );
            }
            I64Store(memarg) | F64Store(memarg) => {
                rewrite!(instrs, i, [F64Mul, F64Add] => FMaStoreF64(memarg));
                rewrite!(instrs, i,
                    [LocalGet32(addr_local), LocalGet64(value_local)] if
                    (let (Ok(addr_local), Ok(value_local)) = (u8::try_from(addr_local), u8::try_from(value_local))) =>
                    StoreLocalLocal64(memarg, addr_local, value_local)
                );
            }
            V128Store(memarg) => {
                rewrite!(instrs, i,
                    [LocalGet32(addr_local), LocalGet128(value_local)] if
                    (let (Ok(addr_local), Ok(value_local)) = (u8::try_from(addr_local), u8::try_from(value_local))) =>
                    StoreLocalLocal128(memarg, addr_local, value_local)
                );
            }
            I32Load(memarg) | F32Load(memarg) => {
                rewrite!(instrs, i,
                    [LocalGet32(addr_local)] if (let Ok(addr_local) = u8::try_from(addr_local)) =>
                    LoadLocal32(memarg, addr_local)
                );
            }
            MemoryFill(mem) => {
                rewrite!(instrs, i, [Const32(val), Const32(size)] => MemoryFillImm(mem, val as u8, size))
            }
            LocalGet32(dst) => rewrite!(instrs, i, [LocalSet32(src)] if (src == dst) => LocalTee32(src)),
            LocalGet64(dst) => rewrite!(instrs, i, [LocalSet64(src)] if (src == dst) => LocalTee64(src)),
            LocalGet128(dst) => rewrite!(instrs, i, [LocalSet128(src)] if (src == dst) => LocalTee128(src)),
            LocalSet32(dst) => {
                fold_local_binop!(
                    instrs, i, dst,
                    source = resolve_local_source_32,
                    op = scalar_bin_op,
                    const = scalar_const_32,
                    local_local = BinOpLocalLocalSet32,
                    local_const = |dst, lhs, op, imm| match (dst == lhs, op) {
                        (true, BinOp::IAdd) => Instruction::IncLocal32(dst, imm),
                        (true, BinOp::ISub) => Instruction::IncLocal32(dst, imm.wrapping_neg()),
                        _ => Instruction::BinOpLocalConstSet32(op, lhs, imm, dst),
                    }
                );
                rewrite!(instrs, i, [I32Mul, LocalGet32(acc), I32Add] if (acc == dst) => MulAccLocal32(dst));
                rewrite!(instrs, i, [F32Mul, LocalGet32(acc), F32Add] if (acc == dst) => FMulAccLocal32(dst));
                rewrite_local_set_direct!(
                    instrs,
                    i,
                    dst,
                    get = LocalGet32,
                    copy = LocalCopy32,
                    binop_local_local = BinOpLocalLocal32,
                    binop_local_local_set = BinOpLocalLocalSet32,
                    binop_local_const = BinOpLocalConst32,
                    binop_local_const_set = |dst, src, op, c| match (dst == src, op) {
                        (true, BinOp::IAdd) => IncLocal32(dst, c),
                        (true, BinOp::ISub) => IncLocal32(dst, c.wrapping_neg()),
                        _ => BinOpLocalConstSet32(op, src, c, dst),
                    },
                    const_instr = Const32,
                    set_local_const = SetLocalConst32
                );
                rewrite!(instrs, i, [LoadLocal32(memarg, addr)] if (let Ok(dst) = u8::try_from(dst)) => LoadLocalSet32(memarg, addr, dst));
                rewrite!(instrs, i,
                    [LocalGet32(addr), I32Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalSet32(memarg, addr, dst)
                );
            }
            LocalSet64(dst) => {
                fold_local_binop!(
                    instrs, i, dst,
                    source = resolve_local_source_64,
                    op = scalar_bin_op,
                    const = scalar_const_64,
                    local_local = BinOpLocalLocalSet64,
                    local_const = |dst, lhs, op, imm| match (dst == lhs, op) {
                        (true, BinOp::IAdd) => Instruction::IncLocal64(dst, imm),
                        (true, BinOp::ISub) => Instruction::IncLocal64(dst, imm.wrapping_neg()),
                        _ => Instruction::BinOpLocalConstSet64(op, lhs, imm, dst),
                    }
                );
                rewrite!(instrs, i, [I64Mul, LocalGet64(acc), I64Add] if (acc == dst) => MulAccLocal64(dst));
                rewrite!(instrs, i, [F64Mul, LocalGet64(acc), F64Add] if (acc == dst) => FMulAccLocal64(dst));
                rewrite_local_set_direct!(
                    instrs,
                    i,
                    dst,
                    get = LocalGet64,
                    copy = LocalCopy64,
                    binop_local_local = BinOpLocalLocal64,
                    binop_local_local_set = BinOpLocalLocalSet64,
                    binop_local_const = BinOpLocalConst64,
                    binop_local_const_set = |dst, src, op, c| match (dst == src, op) {
                        (true, BinOp::IAdd) => IncLocal64(dst, c),
                        (true, BinOp::ISub) => IncLocal64(dst, c.wrapping_neg()),
                        _ => BinOpLocalConstSet64(op, src, c, dst),
                    },
                    const_instr = Const64,
                    set_local_const = SetLocalConst64
                );
            }
            LocalSet128(dst) => {
                fold_local_binop!(
                    instrs, i, dst,
                    source = resolve_local_source_128,
                    op = bin_op_128,
                    const = const_128,
                    local_local = BinOpLocalLocalSet128,
                    local_const = |dst, lhs, op, imm| Instruction::BinOpLocalConstSet128(op, lhs, imm, dst)
                );
                rewrite_local_set_direct!(
                    instrs,
                    i,
                    dst,
                    get = LocalGet128,
                    copy = LocalCopy128,
                    binop_local_local = BinOpLocalLocal128,
                    binop_local_local_set = BinOpLocalLocalSet128,
                    binop_local_const = BinOpLocalConst128,
                    binop_local_const_set = |dst, src, op, c| BinOpLocalConstSet128(op, src, c, dst),
                    const_instr = Const128,
                    set_local_const = SetLocalConst128
                );
                rewrite!(instrs, i,
                    [LocalGet32(addr), V128Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalSet128(memarg, addr, dst)
                );
            }
            LocalTee32(dst) => {
                fold_local_binop!(
                    instrs, i, dst,
                    source = resolve_local_source_32,
                    op = scalar_bin_op,
                    const = scalar_const_32,
                    local_local = BinOpLocalLocalTee32,
                    local_const = |dst, lhs, op, imm| Instruction::BinOpLocalConstTee32(op, lhs, imm, dst)
                );
                rewrite_local_tee_direct!(
                    instrs,
                    i,
                    dst,
                    get = LocalGet32,
                    binop_local_local = BinOpLocalLocal32,
                    binop_local_local_tee = BinOpLocalLocalTee32,
                    binop_local_const = BinOpLocalConst32,
                    binop_local_const_tee = BinOpLocalConstTee32
                );
                rewrite!(instrs, i, [Const32(c), I32And] => AndConstTee32(c, dst));
                rewrite!(instrs, i, [Const32(c), I32Sub] => SubConstTee32(c, dst));
                rewrite!(instrs, i,
                    [LocalGet32(addr), I32Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalTee32(memarg, addr, dst)
                );
                rewrite!(instrs, i,
                    [LocalGet32(addr), F32Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalTee32(memarg, addr, dst)
                );
                rewrite!(instrs, i,
                    [LoadLocal32(memarg, addr)] if (let Ok(dst) = u8::try_from(dst)) =>
                    LoadLocalTee32(memarg, addr, dst)
                );
            }
            LocalTee64(dst) => {
                fold_local_binop!(
                    instrs, i, dst,
                    source = resolve_local_source_64,
                    op = scalar_bin_op,
                    const = scalar_const_64,
                    local_local = BinOpLocalLocalTee64,
                    local_const = |dst, lhs, op, imm| Instruction::BinOpLocalConstTee64(op, lhs, imm, dst)
                );
                rewrite_local_tee_direct!(
                    instrs,
                    i,
                    dst,
                    get = LocalGet64,
                    binop_local_local = BinOpLocalLocal64,
                    binop_local_local_tee = BinOpLocalLocalTee64,
                    binop_local_const = BinOpLocalConst64,
                    binop_local_const_tee = BinOpLocalConstTee64
                );
                rewrite!(instrs, i, [Const64(c), I64And] => AndConstTee64(c, dst));
                rewrite!(instrs, i, [Const64(c), I64Sub] => SubConstTee64(c, dst));
            }
            LocalTee128(dst) => {
                fold_local_binop!(
                    instrs, i, dst,
                    source = resolve_local_source_128,
                    op = bin_op_128,
                    const = const_128,
                    local_local = BinOpLocalLocalTee128,
                    local_const = |dst, lhs, op, imm| Instruction::BinOpLocalConstTee128(op, lhs, imm, dst)
                );
                rewrite_local_tee_direct!(
                    instrs,
                    i,
                    dst,
                    get = LocalGet128,
                    binop_local_local = BinOpLocalLocal128,
                    binop_local_local_tee = BinOpLocalLocalTee128,
                    binop_local_const = BinOpLocalConst128,
                    binop_local_const_tee = BinOpLocalConstTee128
                );
                rewrite!(instrs, i,
                    [LocalGet32(addr), V128Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalTee128(memarg, addr, dst)
                );
            }
            Drop32 => rewrite_drop_tee_direct!(
                instrs,
                i,
                tee = LocalTee32,
                set = LocalSet32,
                binop_local_local_tee = BinOpLocalLocalTee32,
                binop_local_local_set = BinOpLocalLocalSet32,
                binop_local_const_tee = BinOpLocalConstTee32,
                binop_local_const_set = BinOpLocalConstSet32
            ),
            Drop64 => rewrite_drop_tee_direct!(
                instrs,
                i,
                tee = LocalTee64,
                set = LocalSet64,
                binop_local_local_tee = BinOpLocalLocalTee64,
                binop_local_local_set = BinOpLocalLocalSet64,
                binop_local_const_tee = BinOpLocalConstTee64,
                binop_local_const_set = BinOpLocalConstSet64
            ),
            Drop128 => rewrite_drop_tee_direct!(
                instrs,
                i,
                tee = LocalTee128,
                set = LocalSet128,
                binop_local_local_tee = BinOpLocalLocalTee128,
                binop_local_local_set = BinOpLocalLocalSet128,
                binop_local_const_tee = BinOpLocalConstTee128,
                binop_local_const_set = BinOpLocalConstSet128
            ),
            Jump(ip) => {
                let target = resolve_jump_target(&source, ip);
                let exit = old_idx as u32 + 1;
                let body = target + 1;

                match source.get(target as usize).copied() {
                    Some(JumpCmpLocalLocal32 { target_ip, left, right, op })
                        if resolve_jump_target(&source, target_ip) == exit && body > target =>
                    {
                        instrs[i] = JumpCmpLocalLocal32 { target_ip: body, left, right, op: inverse_cmp_op(op) };
                    }
                    Some(JumpCmpLocalLocal64 { target_ip, left, right, op })
                        if resolve_jump_target(&source, target_ip) == exit && body > target =>
                    {
                        instrs[i] = JumpCmpLocalLocal64 { target_ip: body, left, right, op: inverse_cmp_op(op) };
                    }
                    _ => canonicalize_jump_like_with_target(&mut instrs, i, target, exit),
                }
            }
            JumpIfZero32(ip) => {
                let target = resolve_jump_target(&source, ip);
                rewrite!(instrs, i, [LocalGet32(local), I32Eqz] => {
                    replace!(instrs, i, 2 => JumpIfLocalNonZero32 { target_ip: target, local });
                    continue;
                });
                rewrite!(instrs, i, [I32Eqz] => {
                    replace!(instrs, i, 1 => JumpIfNonZero32(target));
                    continue;
                });
                rewrite!(instrs, i, [LocalGet32(local)] => JumpIfLocalZero32 { target_ip: target, local });
                rewrite!(instrs, i, [LocalGet64(local), I64Eqz] => {
                    replace!(instrs, i, 2 => JumpIfLocalNonZero64 { target_ip: target, local });
                    continue;
                });
                rewrite!(instrs, i, [I64Eqz] => {
                    replace!(instrs, i, 1 => JumpIfNonZero64(target));
                    continue;
                });
                rewrite!(instrs, i,
                    [LocalGet32(local), Const32(imm), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    match (imm, inverse_cmp_op(op)) {
                        (0, CmpOp::Eq) => JumpIfLocalZero32 { target_ip: target, local },
                        (0, CmpOp::Ne) => JumpIfLocalNonZero32 { target_ip: target, local },
                        (imm, op) => JumpCmpLocalConst32 { target_ip: target, local, imm, op },
                    }
                );
                rewrite!(instrs, i,
                    [LocalGet64(local), Const64(imm), cmp] if
                    (let Some(op) = cmp_op(cmp) && let Ok(imm) = i32::try_from(imm)) =>
                    match (imm, inverse_cmp_op(op)) {
                        (0, CmpOp::Eq) => JumpIfLocalZero64 { target_ip: target, local },
                        (0, CmpOp::Ne) => JumpIfLocalNonZero64 { target_ip: target, local },
                        (imm, op) => JumpCmpLocalConst64 { target_ip: target, local, imm, op },
                    }
                );
                rewrite!(instrs, i,
                    [LocalGet32(left), LocalGet32(right), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalLocal32 { target_ip: target, left, right, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i,
                    [LocalGet64(left), LocalGet64(right), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalLocal64 { target_ip: target, left, right, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i, [Const32(imm), cmp] if (let Some(op) = cmp_op(cmp)) => match (imm, inverse_cmp_op(op)) {
                    (0, CmpOp::Eq) => JumpIfZero32(target),
                    (0, CmpOp::Ne) => JumpIfNonZero32(target),
                    (imm, op) => JumpCmpStackConst32 { target_ip: target, imm, op },
                });
                rewrite!(instrs, i, [Const64(imm), cmp] if (let Some(op) = cmp_op(cmp)) => match (imm, inverse_cmp_op(op)) {
                    (0, CmpOp::Eq) => JumpIfZero64(target),
                    (0, CmpOp::Ne) => JumpIfNonZero64(target),
                    (imm, op) => JumpCmpStackConst64 { target_ip: target, imm, op },
                });
                canonicalize_jump_like_with_target(&mut instrs, i, target, old_idx as u32 + 1);
            }
            JumpIfNonZero32(ip) => {
                let target = resolve_jump_target(&source, ip);
                rewrite!(instrs, i, [LocalGet32(local), I32Eqz] => {
                    replace!(instrs, i, 2 => JumpIfLocalZero32 { target_ip: target, local });
                    continue;
                });
                rewrite!(instrs, i, [I32Eqz] => {
                    replace!(instrs, i, 1 => JumpIfZero32(target));
                    continue;
                });
                rewrite!(instrs, i, [LocalGet32(local)] => JumpIfLocalNonZero32 { target_ip: target, local });
                rewrite!(instrs, i, [LocalGet64(local), I64Eqz] => {
                    replace!(instrs, i, 2 => JumpIfLocalZero64 { target_ip: target, local });
                    continue;
                });
                rewrite!(instrs, i, [I64Eqz] => {
                    replace!(instrs, i, 1 => JumpIfZero64(target));
                    continue;
                });
                rewrite!(instrs, i,
                    [LocalGet32(local), Const32(imm), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    match (imm, op) {
                        (0, CmpOp::Eq) => JumpIfLocalZero32 { target_ip: target, local },
                        (0, CmpOp::Ne) => JumpIfLocalNonZero32 { target_ip: target, local },
                        (imm, op) => JumpCmpLocalConst32 { target_ip: target, local, imm, op },
                    }
                );
                rewrite!(instrs, i,
                    [LocalGet64(local), Const64(imm), cmp] if
                    (let Some(op) = cmp_op(cmp) && let Ok(imm) = i32::try_from(imm)) =>
                    match (imm, op) {
                        (0, CmpOp::Eq) => JumpIfLocalZero64 { target_ip: target, local },
                        (0, CmpOp::Ne) => JumpIfLocalNonZero64 { target_ip: target, local },
                        (imm, op) => JumpCmpLocalConst64 { target_ip: target, local, imm, op },
                    }
                );
                rewrite!(instrs, i,
                    [LocalGet32(left), LocalGet32(right), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalLocal32 { target_ip: target, left, right, op }
                );
                rewrite!(instrs, i,
                    [LocalGet64(left), LocalGet64(right), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalLocal64 { target_ip: target, left, right, op }
                );
                rewrite!(instrs, i, [Const32(imm), cmp] if (let Some(op) = cmp_op(cmp)) => match (imm, op) {
                    (0, CmpOp::Eq) => JumpIfZero32(target),
                    (0, CmpOp::Ne) => JumpIfNonZero32(target),
                    (imm, op) => JumpCmpStackConst32 { target_ip: target, imm, op },
                });
                rewrite!(instrs, i, [Const64(imm), cmp] if (let Some(op) = cmp_op(cmp)) => match (imm, op) {
                    (0, CmpOp::Eq) => JumpIfZero64(target),
                    (0, CmpOp::Ne) => JumpIfNonZero64(target),
                    (imm, op) => JumpCmpStackConst64 { target_ip: target, imm, op },
                });
                canonicalize_jump_like_with_target(&mut instrs, i, target, old_idx as u32 + 1);
            }
            JumpIfZero64(ip) => {
                let target = resolve_jump_target(&source, ip);
                rewrite!(instrs, i, [LocalGet64(local)] => JumpIfLocalZero64 { target_ip: target, local });
                canonicalize_jump_like_with_target(&mut instrs, i, target, old_idx as u32 + 1);
            }
            JumpIfNonZero64(ip) => {
                let target = resolve_jump_target(&source, ip);
                rewrite!(instrs, i, [LocalGet64(local)] => JumpIfLocalNonZero64 { target_ip: target, local });
                canonicalize_jump_like_with_target(&mut instrs, i, target, old_idx as u32 + 1);
            }
            JumpCmpStackConst32 { target_ip, imm: 0, op } => {
                match op {
                    CmpOp::Eq => instrs[i] = JumpIfZero32(target_ip),
                    CmpOp::Ne => instrs[i] = JumpIfNonZero32(target_ip),
                    _ => {}
                }
                canonicalize_jump_like(&source, &mut instrs, i, old_idx as u32 + 1);
            }
            JumpCmpStackConst64 { target_ip, imm: 0, op } => {
                match op {
                    CmpOp::Eq => instrs[i] = JumpIfZero64(target_ip),
                    CmpOp::Ne => instrs[i] = JumpIfNonZero64(target_ip),
                    _ => {}
                }
                canonicalize_jump_like(&source, &mut instrs, i, old_idx as u32 + 1);
            }
            JumpCmpLocalConst32 { target_ip, local, imm: 0, op } => {
                match op {
                    CmpOp::Eq => instrs[i] = JumpIfLocalZero32 { target_ip, local },
                    CmpOp::Ne => instrs[i] = JumpIfLocalNonZero32 { target_ip, local },
                    _ => {}
                }
                canonicalize_jump_like(&source, &mut instrs, i, old_idx as u32 + 1);
            }
            JumpCmpLocalConst64 { target_ip, local, imm: 0, op } => {
                match op {
                    CmpOp::Eq => instrs[i] = JumpIfLocalZero64 { target_ip, local },
                    CmpOp::Ne => instrs[i] = JumpIfLocalNonZero64 { target_ip, local },
                    _ => {}
                }
                canonicalize_jump_like(&source, &mut instrs, i, old_idx as u32 + 1);
            }
            JumpCmpStackConst32 { .. }
            | JumpCmpStackConst64 { .. }
            | JumpCmpLocalConst32 { .. }
            | JumpCmpLocalConst64 { .. }
            | JumpCmpLocalLocal32 { .. }
            | JumpCmpLocalLocal64 { .. }
            | JumpIfLocalZero32 { .. }
            | JumpIfLocalNonZero32 { .. }
            | JumpIfLocalZero64 { .. }
            | JumpIfLocalNonZero64 { .. } => {
                canonicalize_jump_like(&source, &mut instrs, i, old_idx as u32 + 1);
            }
            _ => {}
        }

        after_terminator = is_unconditional_terminator(instr);
    }

    old_to_new[source.len()] = instrs.len() as u32;
    (instrs.instructions, old_to_new)
}

fn cmp_op(instr: Instruction) -> Option<CmpOp> {
    Some(match instr {
        Instruction::I32Eq | Instruction::I64Eq => CmpOp::Eq,
        Instruction::I32Ne | Instruction::I64Ne => CmpOp::Ne,
        Instruction::I32LtS | Instruction::I64LtS => CmpOp::LtS,
        Instruction::I32LtU | Instruction::I64LtU => CmpOp::LtU,
        Instruction::I32GtS | Instruction::I64GtS => CmpOp::GtS,
        Instruction::I32GtU | Instruction::I64GtU => CmpOp::GtU,
        Instruction::I32LeS | Instruction::I64LeS => CmpOp::LeS,
        Instruction::I32LeU | Instruction::I64LeU => CmpOp::LeU,
        Instruction::I32GeS | Instruction::I64GeS => CmpOp::GeS,
        Instruction::I32GeU | Instruction::I64GeU => CmpOp::GeU,
        _ => return None,
    })
}

fn int_bin_op(instr: Instruction) -> Option<BinOp> {
    Some(match instr {
        Instruction::I32Add | Instruction::I64Add => BinOp::IAdd,
        Instruction::I32Sub | Instruction::I64Sub => BinOp::ISub,
        Instruction::I32Mul | Instruction::I64Mul => BinOp::IMul,
        Instruction::I32And | Instruction::I64And => BinOp::IAnd,
        Instruction::I32Or | Instruction::I64Or => BinOp::IOr,
        Instruction::I32Xor | Instruction::I64Xor => BinOp::IXor,
        Instruction::I32Shl | Instruction::I64Shl => BinOp::IShl,
        Instruction::I32ShrS | Instruction::I64ShrS => BinOp::IShrS,
        Instruction::I32ShrU | Instruction::I64ShrU => BinOp::IShrU,
        Instruction::I32Rotl | Instruction::I64Rotl => BinOp::IRotl,
        Instruction::I32Rotr | Instruction::I64Rotr => BinOp::IRotr,
        _ => return None,
    })
}

fn float_bin_op(instr: Instruction) -> Option<BinOp> {
    Some(match instr {
        Instruction::F32Add | Instruction::F64Add => BinOp::FAdd,
        Instruction::F32Sub | Instruction::F64Sub => BinOp::FSub,
        Instruction::F32Mul | Instruction::F64Mul => BinOp::FMul,
        Instruction::F32Div | Instruction::F64Div => BinOp::FDiv,
        Instruction::F32Min | Instruction::F64Min => BinOp::FMin,
        Instruction::F32Max | Instruction::F64Max => BinOp::FMax,
        Instruction::F32Copysign | Instruction::F64Copysign => BinOp::FCopysign,
        _ => return None,
    })
}

fn scalar_bin_op(instr: Instruction) -> Option<BinOp> {
    int_bin_op(instr).or_else(|| float_bin_op(instr))
}

fn scalar_const_32(instr: Instruction, op_instr: Instruction) -> Option<i32> {
    match instr {
        Instruction::Const32(c) if scalar_bin_op(op_instr).is_some() => Some(c),
        _ => None,
    }
}

fn scalar_const_64(instr: Instruction, op_instr: Instruction) -> Option<i64> {
    match instr {
        Instruction::Const64(c) if scalar_bin_op(op_instr).is_some() => Some(c),
        _ => None,
    }
}

fn const_128(instr: Instruction, op_instr: Instruction) -> Option<ConstIdx> {
    match instr {
        Instruction::Const128(c) if bin_op_128(op_instr).is_some() => Some(c),
        _ => None,
    }
}

define_local_source_resolver!(
    resolve_local_source_32,
    get = LocalGet32,
    tee = LocalTee32,
    set = LocalSet32,
    binop_local_local_tee = BinOpLocalLocalTee32,
    binop_local_local_set = BinOpLocalLocalSet32,
    binop_local_const_tee = BinOpLocalConstTee32,
    binop_local_const_set = BinOpLocalConstSet32,
    load_local_tee = LoadLocalTee32,
    load_local_set = LoadLocalSet32
);

define_local_source_resolver!(
    resolve_local_source_64,
    get = LocalGet64,
    tee = LocalTee64,
    set = LocalSet64,
    binop_local_local_tee = BinOpLocalLocalTee64,
    binop_local_local_set = BinOpLocalLocalSet64,
    binop_local_const_tee = BinOpLocalConstTee64,
    binop_local_const_set = BinOpLocalConstSet64
);

define_local_source_resolver!(
    resolve_local_source_128,
    get = LocalGet128,
    tee = LocalTee128,
    set = LocalSet128,
    binop_local_local_tee = BinOpLocalLocalTee128,
    binop_local_local_set = BinOpLocalLocalSet128,
    binop_local_const_tee = BinOpLocalConstTee128,
    binop_local_const_set = BinOpLocalConstSet128,
    load_local_tee = LoadLocalTee128,
    load_local_set = LoadLocalSet128
);

fn bin_op_128(instr: Instruction) -> Option<BinOp128> {
    Some(match instr {
        Instruction::V128And => BinOp128::And,
        Instruction::V128AndNot => BinOp128::AndNot,
        Instruction::V128Or => BinOp128::Or,
        Instruction::V128Xor => BinOp128::Xor,
        Instruction::I64x2Add => BinOp128::I64x2Add,
        Instruction::I64x2Mul => BinOp128::I64x2Mul,
        _ => return None,
    })
}

fn inverse_cmp_op(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Eq => CmpOp::Ne,
        CmpOp::Ne => CmpOp::Eq,
        CmpOp::LtS => CmpOp::GeS,
        CmpOp::LtU => CmpOp::GeU,
        CmpOp::GtS => CmpOp::LeS,
        CmpOp::GtU => CmpOp::LeU,
        CmpOp::LeS => CmpOp::GtS,
        CmpOp::LeU => CmpOp::GtU,
        CmpOp::GeS => CmpOp::LtS,
        CmpOp::GeU => CmpOp::LtU,
    }
}

fn resolve_jump_target(instrs: &[Instruction], target: u32) -> u32 {
    let mut idx = target as usize;
    let mut steps = 0usize;

    while let Some(Instruction::Jump(next)) = instrs.get(idx)
        && steps < instrs.len()
    {
        idx = *next as usize;
        steps += 1;
    }

    idx as u32
}

fn instruction_target_mut(instr: &mut Instruction) -> Option<&mut u32> {
    Some(match instr {
        Instruction::Jump(ip)
        | Instruction::JumpIfZero32(ip)
        | Instruction::JumpIfNonZero32(ip)
        | Instruction::JumpIfZero64(ip)
        | Instruction::JumpIfNonZero64(ip)
        | Instruction::JumpCmpStackConst32 { target_ip: ip, .. }
        | Instruction::JumpCmpStackConst64 { target_ip: ip, .. }
        | Instruction::JumpIfLocalZero32 { target_ip: ip, .. }
        | Instruction::JumpIfLocalNonZero32 { target_ip: ip, .. }
        | Instruction::JumpIfLocalZero64 { target_ip: ip, .. }
        | Instruction::JumpIfLocalNonZero64 { target_ip: ip, .. }
        | Instruction::JumpCmpLocalConst32 { target_ip: ip, .. }
        | Instruction::JumpCmpLocalConst64 { target_ip: ip, .. }
        | Instruction::JumpCmpLocalLocal32 { target_ip: ip, .. }
        | Instruction::JumpCmpLocalLocal64 { target_ip: ip, .. } => ip,
        Instruction::BranchTable(ip, _, _) => ip,
        _ => return None,
    })
}

fn instruction_target(instr: &Instruction) -> Option<u32> {
    let mut instr = *instr;
    instruction_target_mut(&mut instr).copied()
}

fn canonicalize_jump_like(source: &[Instruction], instrs: &mut Vec<Instruction>, idx: usize, fallthrough: u32) {
    let Some(target) = instruction_target(&instrs[idx]) else {
        return;
    };

    canonicalize_jump_like_with_target(instrs, idx, resolve_jump_target(source, target), fallthrough);
}

fn canonicalize_jump_like_with_target(instrs: &mut Vec<Instruction>, idx: usize, target: u32, fallthrough: u32) {
    if matches!(instrs[idx], Instruction::Jump(_)) && target == fallthrough {
        instrs.truncate(idx);
    } else if let Some(ip) = instruction_target_mut(&mut instrs[idx]) {
        *ip = target;
    }
}

fn is_unconditional_terminator(instr: Instruction) -> bool {
    matches!(
        instr,
        Instruction::Unreachable
            | Instruction::Jump(_)
            | Instruction::BranchTable(..)
            | Instruction::Return
            | Instruction::ReturnVoid
            | Instruction::Return32
            | Instruction::Return64
            | Instruction::Return128
            | Instruction::ReturnCall(_)
            | Instruction::ReturnCallSelf
            | Instruction::ReturnCallIndirect(..)
    )
}

fn target_boundaries(instructions: &[Instruction], function_data: &WasmFunctionData) -> Result<Vec<bool>> {
    let mut boundaries = alloc::vec![false; instructions.len() + 1];
    for instr in instructions {
        if let Some(target) = instruction_target(instr) {
            let boundary = boundaries
                .get_mut(target as usize)
                .ok_or_else(|| ParseError::Other(alloc::format!("instruction target out of bounds: {target}")))?;
            *boundary = true;
        }
        if let Instruction::BranchTable(_, start, count) = *instr {
            let end =
                start.checked_add(count).ok_or_else(|| ParseError::Other("branch table range overflow".into()))?;
            let targets = function_data
                .branch_table_targets
                .get(start as usize..end as usize)
                .ok_or_else(|| ParseError::Other("branch table range out of bounds".into()))?;
            for &target in targets {
                let boundary = boundaries
                    .get_mut(target as usize)
                    .ok_or_else(|| ParseError::Other(alloc::format!("branch table target out of bounds: {target}")))?;
                *boundary = true;
            }
        }
    }
    Ok(boundaries)
}

/// Remaps rewritten targets, then validates targets and ranges while detecting local memory use.
fn finalize(
    instructions: &mut [Instruction],
    function_data: &mut WasmFunctionData,
    old_to_new: Option<&[u32]>,
    imported_memory_count: u32,
) -> Result<bool> {
    let len = instructions.len() as u32;
    for target in &mut function_data.branch_table_targets {
        if let Some(old_to_new) = old_to_new {
            *target = *old_to_new
                .get(*target as usize)
                .ok_or_else(|| ParseError::Other(alloc::format!("instruction target out of bounds: {target}")))?;
        }
        if *target >= len {
            return Err(ParseError::Other(alloc::format!("branch table target out of bounds: {target}")));
        }
    }

    let mut uses_local_memory = false;
    for instr in instructions {
        if let Some(target) = instruction_target_mut(instr) {
            if let Some(old_to_new) = old_to_new {
                *target = *old_to_new
                    .get(*target as usize)
                    .ok_or_else(|| ParseError::Other(alloc::format!("instruction target out of bounds: {target}")))?;
            }
            if *target >= len {
                return Err(ParseError::Other(alloc::format!("instruction target out of bounds: {target}")));
            }
        }
        if let Instruction::BranchTable(_, start, count) = *instr {
            let end =
                start.checked_add(count).ok_or_else(|| ParseError::Other("branch table range overflow".into()))?;
            function_data
                .branch_table_targets
                .get(start as usize..end as usize)
                .ok_or_else(|| ParseError::Other("branch table range out of bounds".into()))?;
        }
        uses_local_memory |= instr.memory_addr().is_some_and(|mem| mem >= imported_memory_count);
    }
    Ok(uses_local_memory)
}
