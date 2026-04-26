use crate::ParserOptions;
use crate::macros::optimize::*;
use alloc::vec::Vec;
use tinywasm_types::{BinOp, BinOp128, CmpOp, ConstIdx, Instruction, ValueCounts, WasmFunctionData};

pub(crate) struct OptimizeResult {
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) uses_local_memory: bool,
}

pub(crate) fn optimize_instructions(
    mut instructions: Vec<Instruction>,
    function_data: &mut WasmFunctionData,
    options: &ParserOptions,
    function_results: ValueCounts,
    self_func_addr: u32,
    imported_memory_count: u32,
) -> OptimizeResult {
    let uses_local_memory = if options.optimize_rewrite() {
        rewrite(&mut instructions, function_results, self_func_addr, imported_memory_count)
    } else {
        instructions.iter().any(|instr| instr.memory_addr().is_some_and(|mem| mem >= imported_memory_count))
    };

    if options.optimize_remove_nop() {
        remove_nop(&mut instructions, function_data);
    }
    OptimizeResult { instructions, uses_local_memory }
}

fn rewrite(
    instrs: &mut [Instruction],
    function_results: ValueCounts,
    self_func_addr: u32,
    imported_memory_count: u32,
) -> bool {
    use Instruction::*;
    let mut uses_local_memory = false;
    let return_instr = match function_results {
        ValueCounts { c32: 0, c64: 0, c128: 0 } => Some(ReturnVoid),
        ValueCounts { c32: 1, c64: 0, c128: 0 } => Some(Return32),
        ValueCounts { c32: 0, c64: 1, c128: 0 } => Some(Return64),
        ValueCounts { c32: 0, c64: 0, c128: 1 } => Some(Return128),
        _ => None,
    };

    for i in 0..instrs.len() {
        match instrs[i] {
            LocalCopy32(a, b) if a == b => instrs[i] = Nop,
            LocalCopy64(a, b) if a == b => instrs[i] = Nop,
            LocalCopy128(a, b) if a == b => instrs[i] = Nop,
            Call(addr) if addr == self_func_addr => instrs[i] = CallSelf,
            ReturnCall(addr) if addr == self_func_addr => instrs[i] = ReturnCallSelf,
            Return if let Some(return_instr) = return_instr => instrs[i] = return_instr,
            instr @ (I32Add | I32Mul | I32And | I32Or | I32Xor) => {
                let Some(op) = int_bin_op_32(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), Const32(c)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [Const32(c), LocalGet32(local)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [GlobalGet(global)] => [Nop, BinOpStackGlobal32(op, global)]);
                if matches!(op, BinOp::IAdd) {
                    rewrite!(instrs, i, [Const32(c)] => AddConst32(c));
                    rewrite!(instrs, i, [I32Add] => [Nop, I32Add3]);
                }
            }
            instr @ (I32Sub | I32Shl | I32ShrS | I32ShrU | I32Rotl | I32Rotr) => {
                let Some(op) = int_bin_op_32(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), Const32(c)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [GlobalGet(global)] => [Nop, BinOpStackGlobal32(op, global)]);
                if matches!(op, BinOp::IShrS) {
                    rewrite!(instrs, i, [BinOpLocalConst32(BinOp::IShl, local, 8), Const32(8)] => [Nop, LocalGet32(local), I32Extend8S]);
                    rewrite!(instrs, i, [BinOpLocalConst32(BinOp::IShl, local, 16), Const32(16)] => [Nop, LocalGet32(local), I32Extend16S]);
                }
            }
            instr @ (I64Add | I64Mul | I64And | I64Or | I64Xor) => {
                let Some(op) = int_bin_op_64(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), Const64(c)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [Const64(c), LocalGet64(local)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [GlobalGet(global)] => [Nop, BinOpStackGlobal64(op, global)]);
                if matches!(op, BinOp::IAdd) {
                    rewrite!(instrs, i, [Const64(c)] => AddConst64(c));
                    rewrite!(instrs, i, [I64Add] => [Nop, I64Add3]);
                }
            }
            instr @ (I64Sub | I64Shl | I64ShrS | I64ShrU | I64Rotl | I64Rotr) => {
                let Some(op) = int_bin_op_64(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), Const64(c)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [GlobalGet(global)] => [Nop, BinOpStackGlobal64(op, global)]);
                if matches!(op, BinOp::IShrS) {
                    rewrite!(instrs, i, [BinOpLocalConst64(BinOp::IShl, local, 8), Const64(8)] => [Nop, LocalGet64(local), I64Extend8S]);
                    rewrite!(instrs, i, [BinOpLocalConst64(BinOp::IShl, local, 16), Const64(16)] => [Nop, LocalGet64(local), I64Extend16S]);
                    rewrite!(instrs, i, [BinOpLocalConst64(BinOp::IShl, local, 32), Const64(32)] => [Nop, LocalGet64(local), I64Extend32S]);
                }
            }
            instr @ (F32Add | F32Mul | F32Min | F32Max) => {
                let Some(op) = float_bin_op_32(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), Const32(c)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [Const32(c), LocalGet32(local)] => BinOpLocalConst32(op, local, c));
            }
            instr @ (F32Sub | F32Div | F32Copysign) => {
                let Some(op) = float_bin_op_32(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), Const32(c)] => BinOpLocalConst32(op, local, c));
            }
            instr @ (F64Add | F64Mul | F64Min | F64Max) => {
                let Some(op) = float_bin_op_64(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), Const64(c)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [Const64(c), LocalGet64(local)] => BinOpLocalConst64(op, local, c));
            }
            instr @ (F64Sub | F64Div | F64Copysign) => {
                let Some(op) = float_bin_op_64(instr) else { unreachable!() };
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
                rewrite!(instrs, i, [F32Mul, F32Add] => [Nop, Nop, FMaStoreF32(memarg)]);
                rewrite!(instrs, i,
                    [LocalGet32(addr_local), LocalGet32(value_local)] if
                    (let (Ok(addr_local), Ok(value_local)) = (u8::try_from(addr_local), u8::try_from(value_local))) =>
                    StoreLocalLocal32(memarg, addr_local, value_local)
                );
            }
            I64Store(memarg) | F64Store(memarg) => {
                rewrite!(instrs, i, [F64Mul, F64Add] => [Nop, Nop, FMaStoreF64(memarg)]);
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
            LocalGet32(dst) => rewrite!(instrs, i, [LocalSet32(src)] if (src == dst) => [LocalTee32(src), Nop]),
            LocalGet64(dst) => rewrite!(instrs, i, [LocalSet64(src)] if (src == dst) => [LocalTee64(src), Nop]),
            LocalGet128(dst) => rewrite!(instrs, i, [LocalSet128(src)] if (src == dst) => [LocalTee128(src), Nop]),
            LocalSet32(dst) => {
                fold_local_binop!(
                    instrs, i, dst,
                    source = resolve_local_source_32,
                    op = scalar_bin_op_32,
                    const = scalar_const_32,
                    local_local = BinOpLocalLocalSet32,
                    local_const = |dst, lhs, op, imm| match (dst == lhs, op) {
                        (true, BinOp::IAdd) => Instruction::IncLocal32(dst, imm),
                        (true, BinOp::ISub) => Instruction::IncLocal32(dst, imm.wrapping_neg()),
                        _ => Instruction::BinOpLocalConstSet32(op, lhs, imm, dst),
                    }
                );
                rewrite!(instrs, i, [I32Mul, LocalGet32(acc), I32Add] if (acc == dst) => [Nop, Nop, Nop, MulAccLocal32(dst)]);
                rewrite!(instrs, i, [F32Mul, LocalGet32(acc), F32Add] if (acc == dst) => [Nop, Nop, Nop, FMulAccLocal32(dst)]);
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
                    op = scalar_bin_op_64,
                    const = scalar_const_64,
                    local_local = BinOpLocalLocalSet64,
                    local_const = |dst, lhs, op, imm| match (dst == lhs, op) {
                        (true, BinOp::IAdd) => Instruction::IncLocal64(dst, imm),
                        (true, BinOp::ISub) => Instruction::IncLocal64(dst, imm.wrapping_neg()),
                        _ => Instruction::BinOpLocalConstSet64(op, lhs, imm, dst),
                    }
                );
                rewrite!(instrs, i, [I64Mul, LocalGet64(acc), I64Add] if (acc == dst) => [Nop, Nop, Nop, MulAccLocal64(dst)]);
                rewrite!(instrs, i, [F64Mul, LocalGet64(acc), F64Add] if (acc == dst) => [Nop, Nop, Nop, FMulAccLocal64(dst)]);
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
                    op = scalar_bin_op_32,
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
                    op = scalar_bin_op_64,
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
                let target = resolve_jump_target(instrs, ip);
                let exit = next_non_nop(instrs, i + 1) as u32;
                let body = next_non_nop(instrs, target as usize + 1) as u32;

                match instrs[target as usize] {
                    JumpCmpLocalLocal32 { target_ip, left, right, op }
                        if resolve_jump_target(instrs, target_ip) == exit && body > target =>
                    {
                        instrs[i] = JumpCmpLocalLocal32 { target_ip: body, left, right, op: inverse_cmp_op(op) };
                    }
                    JumpCmpLocalLocal64 { target_ip, left, right, op }
                        if resolve_jump_target(instrs, target_ip) == exit && body > target =>
                    {
                        instrs[i] = JumpCmpLocalLocal64 { target_ip: body, left, right, op: inverse_cmp_op(op) };
                    }
                    _ => canonicalize_jump_like_with_target(instrs, i, target),
                }
            }
            JumpIfZero32(ip) => {
                let target = resolve_jump_target(instrs, ip);
                rewrite!(instrs, i, [LocalGet32(local), I32Eqz] => {
                    replace!(instrs, i, 2 => [Nop, Nop, JumpIfLocalNonZero32 { target_ip: target, local }]);
                    continue;
                });
                rewrite!(instrs, i, [I32Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfNonZero32(target)]);
                    continue;
                });
                rewrite!(instrs, i, [LocalGet32(local)] => JumpIfLocalZero32 { target_ip: target, local });
                rewrite!(instrs, i, [LocalGet64(local), I64Eqz] => {
                    replace!(instrs, i, 2 => [Nop, Nop, JumpIfLocalNonZero64 { target_ip: target, local }]);
                    continue;
                });
                rewrite!(instrs, i, [I64Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfNonZero64(target)]);
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
                    (let Some(op) = cmp_op_64(cmp) && let Ok(imm) = i32::try_from(imm)) =>
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
                    [LocalGet64(left), LocalGet64(right), cmp] if (let Some(op) = cmp_op_64(cmp)) =>
                    JumpCmpLocalLocal64 { target_ip: target, left, right, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i, [Const32(imm), cmp] if (let Some(op) = cmp_op(cmp)) => match (imm, inverse_cmp_op(op)) {
                    (0, CmpOp::Eq) => JumpIfZero32(target),
                    (0, CmpOp::Ne) => JumpIfNonZero32(target),
                    (imm, op) => JumpCmpStackConst32 { target_ip: target, imm, op },
                });
                rewrite!(instrs, i, [Const64(imm), cmp] if (let Some(op) = cmp_op_64(cmp)) => match (imm, inverse_cmp_op(op)) {
                    (0, CmpOp::Eq) => JumpIfZero64(target),
                    (0, CmpOp::Ne) => JumpIfNonZero64(target),
                    (imm, op) => JumpCmpStackConst64 { target_ip: target, imm, op },
                });
                canonicalize_jump_like_with_target(instrs, i, target);
            }
            JumpIfNonZero32(ip) => {
                let target = resolve_jump_target(instrs, ip);
                rewrite!(instrs, i, [LocalGet32(local), I32Eqz] => {
                    replace!(instrs, i, 2 => [Nop, Nop, JumpIfLocalZero32 { target_ip: target, local }]);
                    continue;
                });
                rewrite!(instrs, i, [I32Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfZero32(target)]);
                    continue;
                });
                rewrite!(instrs, i, [LocalGet32(local)] => JumpIfLocalNonZero32 { target_ip: target, local });
                rewrite!(instrs, i, [LocalGet64(local), I64Eqz] => {
                    replace!(instrs, i, 2 => [Nop, Nop, JumpIfLocalZero64 { target_ip: target, local }]);
                    continue;
                });
                rewrite!(instrs, i, [I64Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfZero64(target)]);
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
                    (let Some(op) = cmp_op_64(cmp) && let Ok(imm) = i32::try_from(imm)) =>
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
                    [LocalGet64(left), LocalGet64(right), cmp] if (let Some(op) = cmp_op_64(cmp)) =>
                    JumpCmpLocalLocal64 { target_ip: target, left, right, op }
                );
                rewrite!(instrs, i, [Const32(imm), cmp] if (let Some(op) = cmp_op(cmp)) => match (imm, op) {
                    (0, CmpOp::Eq) => JumpIfZero32(target),
                    (0, CmpOp::Ne) => JumpIfNonZero32(target),
                    (imm, op) => JumpCmpStackConst32 { target_ip: target, imm, op },
                });
                rewrite!(instrs, i, [Const64(imm), cmp] if (let Some(op) = cmp_op_64(cmp)) => match (imm, op) {
                    (0, CmpOp::Eq) => JumpIfZero64(target),
                    (0, CmpOp::Ne) => JumpIfNonZero64(target),
                    (imm, op) => JumpCmpStackConst64 { target_ip: target, imm, op },
                });
                canonicalize_jump_like_with_target(instrs, i, target);
            }
            JumpIfZero64(ip) => {
                let target = resolve_jump_target(instrs, ip);
                rewrite!(instrs, i, [LocalGet64(local)] => JumpIfLocalZero64 { target_ip: target, local });
                canonicalize_jump_like_with_target(instrs, i, target);
            }
            JumpIfNonZero64(ip) => {
                let target = resolve_jump_target(instrs, ip);
                rewrite!(instrs, i, [LocalGet64(local)] => JumpIfLocalNonZero64 { target_ip: target, local });
                canonicalize_jump_like_with_target(instrs, i, target);
            }
            JumpCmpStackConst32 { target_ip, imm: 0, op } => {
                match op {
                    CmpOp::Eq => instrs[i] = JumpIfZero32(target_ip),
                    CmpOp::Ne => instrs[i] = JumpIfNonZero32(target_ip),
                    _ => {}
                }
                canonicalize_jump_like(instrs, i);
            }
            JumpCmpStackConst64 { target_ip, imm: 0, op } => {
                match op {
                    CmpOp::Eq => instrs[i] = JumpIfZero64(target_ip),
                    CmpOp::Ne => instrs[i] = JumpIfNonZero64(target_ip),
                    _ => {}
                }
                canonicalize_jump_like(instrs, i);
            }
            JumpCmpLocalConst32 { target_ip, local, imm: 0, op } => {
                match op {
                    CmpOp::Eq => instrs[i] = JumpIfLocalZero32 { target_ip, local },
                    CmpOp::Ne => instrs[i] = JumpIfLocalNonZero32 { target_ip, local },
                    _ => {}
                }
                canonicalize_jump_like(instrs, i);
            }
            JumpCmpLocalConst64 { target_ip, local, imm: 0, op } => {
                match op {
                    CmpOp::Eq => instrs[i] = JumpIfLocalZero64 { target_ip, local },
                    CmpOp::Ne => instrs[i] = JumpIfLocalNonZero64 { target_ip, local },
                    _ => {}
                }
                canonicalize_jump_like(instrs, i);
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
                canonicalize_jump_like(instrs, i);
            }
            _ => {}
        }

        if !uses_local_memory {
            uses_local_memory = instrs[i].memory_addr().is_some_and(|mem| mem >= imported_memory_count);
        }
    }

    uses_local_memory
}

fn cmp_op(instr: Instruction) -> Option<CmpOp> {
    Some(match instr {
        Instruction::I32Eq => CmpOp::Eq,
        Instruction::I32Ne => CmpOp::Ne,
        Instruction::I32LtS => CmpOp::LtS,
        Instruction::I32LtU => CmpOp::LtU,
        Instruction::I32GtS => CmpOp::GtS,
        Instruction::I32GtU => CmpOp::GtU,
        Instruction::I32LeS => CmpOp::LeS,
        Instruction::I32LeU => CmpOp::LeU,
        Instruction::I32GeS => CmpOp::GeS,
        Instruction::I32GeU => CmpOp::GeU,
        _ => return None,
    })
}

fn int_bin_op_32(instr: Instruction) -> Option<BinOp> {
    Some(match instr {
        Instruction::I32Add => BinOp::IAdd,
        Instruction::I32Sub => BinOp::ISub,
        Instruction::I32Mul => BinOp::IMul,
        Instruction::I32And => BinOp::IAnd,
        Instruction::I32Or => BinOp::IOr,
        Instruction::I32Xor => BinOp::IXor,
        Instruction::I32Shl => BinOp::IShl,
        Instruction::I32ShrS => BinOp::IShrS,
        Instruction::I32ShrU => BinOp::IShrU,
        Instruction::I32Rotl => BinOp::IRotl,
        Instruction::I32Rotr => BinOp::IRotr,
        _ => return None,
    })
}

fn int_bin_op_64(instr: Instruction) -> Option<BinOp> {
    Some(match instr {
        Instruction::I64Add => BinOp::IAdd,
        Instruction::I64Sub => BinOp::ISub,
        Instruction::I64Mul => BinOp::IMul,
        Instruction::I64And => BinOp::IAnd,
        Instruction::I64Or => BinOp::IOr,
        Instruction::I64Xor => BinOp::IXor,
        Instruction::I64Shl => BinOp::IShl,
        Instruction::I64ShrS => BinOp::IShrS,
        Instruction::I64ShrU => BinOp::IShrU,
        Instruction::I64Rotl => BinOp::IRotl,
        Instruction::I64Rotr => BinOp::IRotr,
        _ => return None,
    })
}

fn float_bin_op_32(instr: Instruction) -> Option<BinOp> {
    Some(match instr {
        Instruction::F32Add => BinOp::FAdd,
        Instruction::F32Sub => BinOp::FSub,
        Instruction::F32Mul => BinOp::FMul,
        Instruction::F32Div => BinOp::FDiv,
        Instruction::F32Min => BinOp::FMin,
        Instruction::F32Max => BinOp::FMax,
        Instruction::F32Copysign => BinOp::FCopysign,
        _ => return None,
    })
}

fn float_bin_op_64(instr: Instruction) -> Option<BinOp> {
    Some(match instr {
        Instruction::F64Add => BinOp::FAdd,
        Instruction::F64Sub => BinOp::FSub,
        Instruction::F64Mul => BinOp::FMul,
        Instruction::F64Div => BinOp::FDiv,
        Instruction::F64Min => BinOp::FMin,
        Instruction::F64Max => BinOp::FMax,
        Instruction::F64Copysign => BinOp::FCopysign,
        _ => return None,
    })
}

fn scalar_bin_op_32(instr: Instruction) -> Option<BinOp> {
    int_bin_op_32(instr).or_else(|| float_bin_op_32(instr))
}

fn scalar_bin_op_64(instr: Instruction) -> Option<BinOp> {
    int_bin_op_64(instr).or_else(|| float_bin_op_64(instr))
}

fn scalar_const_32(instr: Instruction, op_instr: Instruction) -> Option<i32> {
    match instr {
        Instruction::Const32(c) if int_bin_op_32(op_instr).is_some() || float_bin_op_32(op_instr).is_some() => Some(c),
        _ => None,
    }
}

fn scalar_const_64(instr: Instruction, op_instr: Instruction) -> Option<i64> {
    match instr {
        Instruction::Const64(c) if int_bin_op_64(op_instr).is_some() || float_bin_op_64(op_instr).is_some() => Some(c),
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

fn cmp_op_64(instr: Instruction) -> Option<CmpOp> {
    Some(match instr {
        Instruction::I64Eq => CmpOp::Eq,
        Instruction::I64Ne => CmpOp::Ne,
        Instruction::I64LtS => CmpOp::LtS,
        Instruction::I64LtU => CmpOp::LtU,
        Instruction::I64GtS => CmpOp::GtS,
        Instruction::I64GtU => CmpOp::GtU,
        Instruction::I64LeS => CmpOp::LeS,
        Instruction::I64LeU => CmpOp::LeU,
        Instruction::I64GeS => CmpOp::GeS,
        Instruction::I64GeU => CmpOp::GeU,
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

const PREVIOUS_NON_NOP_BACKTRACK_LIMIT: usize = 32;

fn previous_non_nop<const N: usize>(instrs: &[Instruction], read: usize) -> Option<[(usize, Instruction); N]> {
    let mut out = [(0usize, Instruction::Nop); N];
    let mut filled = 0usize;
    let start = read.saturating_sub(PREVIOUS_NON_NOP_BACKTRACK_LIMIT);

    for idx in (start..read).rev() {
        let instr = instrs[idx];
        if matches!(instr, Instruction::MergeBarrier) {
            return None;
        }
        if matches!(instr, Instruction::Nop) {
            continue;
        }

        out[N - 1 - filled] = (idx, instr);
        filled += 1;
        if filled == N {
            return Some(out);
        }
    }

    None
}

fn next_non_nop(instrs: &[Instruction], mut idx: usize) -> usize {
    while idx < instrs.len() && matches!(instrs[idx], Instruction::Nop | Instruction::MergeBarrier) {
        idx += 1;
    }
    idx
}

fn resolve_jump_target(instrs: &[Instruction], target: u32) -> u32 {
    let mut idx = next_non_nop(instrs, target as usize);
    let mut steps = 0usize;

    while idx < instrs.len() && steps < instrs.len() {
        match instrs[idx] {
            Instruction::Jump(next) => {
                idx = next_non_nop(instrs, next as usize);
                steps += 1;
            }
            _ => break,
        }
    }

    idx as u32
}

fn jump_target(instr: Instruction) -> Option<u32> {
    Some(match instr {
        Instruction::Jump(ip)
        | Instruction::JumpIfZero32(ip)
        | Instruction::JumpIfNonZero32(ip)
        | Instruction::JumpIfZero64(ip)
        | Instruction::JumpIfNonZero64(ip) => ip,
        Instruction::JumpCmpStackConst32 { target_ip, .. }
        | Instruction::JumpCmpStackConst64 { target_ip, .. }
        | Instruction::JumpIfLocalZero32 { target_ip, .. }
        | Instruction::JumpIfLocalNonZero32 { target_ip, .. }
        | Instruction::JumpIfLocalZero64 { target_ip, .. }
        | Instruction::JumpIfLocalNonZero64 { target_ip, .. }
        | Instruction::JumpCmpLocalConst32 { target_ip, .. }
        | Instruction::JumpCmpLocalConst64 { target_ip, .. }
        | Instruction::JumpCmpLocalLocal32 { target_ip, .. }
        | Instruction::JumpCmpLocalLocal64 { target_ip, .. } => target_ip,
        _ => return None,
    })
}

fn set_jump_target(instr: &mut Instruction, target: u32) {
    match instr {
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
        | Instruction::JumpCmpLocalLocal64 { target_ip: ip, .. } => *ip = target,
        _ => {}
    }
}

fn canonicalize_jump_like(instrs: &mut [Instruction], idx: usize) {
    let Some(target) = jump_target(instrs[idx]) else {
        return;
    };

    canonicalize_jump_like_with_target(instrs, idx, resolve_jump_target(instrs, target));
}

fn canonicalize_jump_like_with_target(instrs: &mut [Instruction], idx: usize, target: u32) {
    if matches!(instrs[idx], Instruction::Jump(_)) && target == next_non_nop(instrs, idx + 1) as u32 {
        instrs[idx] = Instruction::Nop;
    } else {
        set_jump_target(&mut instrs[idx], target);
    }
}

fn remove_nop(instructions: &mut Vec<Instruction>, function_data: &mut WasmFunctionData) {
    let old_len = instructions.len();
    if old_len == 0 {
        return;
    }

    let mut removed_before = Vec::with_capacity(old_len + 1);
    removed_before.push(0u32);
    instructions.iter().for_each(|instr| {
        let removed = removed_before.last().copied().unwrap_or(0)
            + u32::from(matches!(instr, Instruction::Nop | Instruction::MergeBarrier));
        removed_before.push(removed);
    });

    let removed_total = removed_before[old_len];
    if removed_total == 0 {
        return;
    }

    let compacted_len = old_len as u32 - removed_total;

    function_data.branch_table_targets.iter_mut().for_each(|ip| {
        let old_target = *ip as usize;
        if old_target <= old_len {
            *ip -= removed_before[old_target];
            debug_assert!(*ip < compacted_len, "remapped jump target points past end of function");
        }
    });

    instructions.retain_mut(|instr| {
        let ip = match instr {
            Instruction::Jump(ip)
            | Instruction::JumpIfZero32(ip)
            | Instruction::JumpIfNonZero32(ip)
            | Instruction::JumpIfZero64(ip)
            | Instruction::JumpIfNonZero64(ip)
            | Instruction::JumpIfLocalZero32 { target_ip: ip, .. }
            | Instruction::JumpIfLocalNonZero32 { target_ip: ip, .. }
            | Instruction::JumpIfLocalZero64 { target_ip: ip, .. }
            | Instruction::JumpIfLocalNonZero64 { target_ip: ip, .. }
            | Instruction::JumpCmpStackConst32 { target_ip: ip, .. }
            | Instruction::JumpCmpStackConst64 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalConst32 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalConst64 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalLocal32 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalLocal64 { target_ip: ip, .. }
            | Instruction::BranchTable(ip, _, _) => ip,
            _ => return !matches!(instr, Instruction::Nop | Instruction::MergeBarrier),
        };

        let old_target = *ip as usize;
        if old_target > old_len {
            return !matches!(instr, Instruction::Nop | Instruction::MergeBarrier);
        }

        *ip -= removed_before[old_target];
        debug_assert!(*ip < compacted_len, "remapped jump target points past end of function");
        !matches!(instr, Instruction::Nop | Instruction::MergeBarrier)
    });
}
