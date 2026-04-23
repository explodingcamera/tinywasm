use crate::macros::optimize::*;
use alloc::vec::Vec;
use tinywasm_types::{BinOp, BinOp128, CmpOp, Instruction, WasmFunctionData};

pub(crate) struct OptimizeResult {
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) uses_local_memory: bool,
}

pub(crate) fn optimize_instructions(
    mut instructions: Vec<Instruction>,
    function_data: &mut WasmFunctionData,
    self_func_addr: u32,
    imported_memory_count: u32,
    track_local_memory_usage: bool,
) -> OptimizeResult {
    let uses_local_memory = rewrite(&mut instructions, self_func_addr, imported_memory_count, track_local_memory_usage);
    remove_nop(&mut instructions, function_data);
    OptimizeResult { instructions, uses_local_memory }
}

fn rewrite(
    instrs: &mut [Instruction],
    self_func_addr: u32,
    imported_memory_count: u32,
    track_local_memory_usage: bool,
) -> bool {
    use Instruction::*;
    let mut uses_local_memory = false;

    for i in 0..instrs.len() {
        match instrs[i] {
            LocalCopy32(a, b) if a == b => instrs[i] = Nop,
            LocalCopy64(a, b) if a == b => instrs[i] = Nop,
            LocalCopy128(a, b) if a == b => instrs[i] = Nop,
            Call(addr) if addr == self_func_addr => instrs[i] = CallSelf,
            ReturnCall(addr) if addr == self_func_addr => instrs[i] = ReturnCallSelf,
            instr @ (I32Add | I32Mul | I32And | I32Or | I32Xor) => {
                let Some(op) = int_bin_op_32(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), I32Const(c)] => BinOpLocalConst32(op, local, c));
                rewrite!(instrs, i, [I32Const(c), LocalGet32(local)] => BinOpLocalConst32(op, local, c));
                if matches!(op, BinOp::IAdd) {
                    rewrite!(instrs, i, [I32Const(c)] => AddConst32(c));
                }
            }
            instr @ (I32Sub | I32Shl | I32ShrS | I32ShrU | I32Rotl | I32Rotr) => {
                let Some(op) = int_bin_op_32(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), I32Const(c)] => BinOpLocalConst32(op, local, c));
            }
            instr @ (I64Add | I64Mul | I64And | I64Or | I64Xor) => {
                let Some(op) = int_bin_op_64(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), I64Const(c)] => BinOpLocalConst64(op, local, c));
                rewrite!(instrs, i, [I64Const(c), LocalGet64(local)] => BinOpLocalConst64(op, local, c));
                if matches!(op, BinOp::IAdd) {
                    rewrite!(instrs, i, [I64Const(c)] => AddConst64(c));
                }
            }
            instr @ (I64Sub | I64Shl | I64ShrS | I64ShrU | I64Rotl | I64Rotr) => {
                let Some(op) = int_bin_op_64(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), I64Const(c)] => BinOpLocalConst64(op, local, c));
            }
            instr @ (F32Add | F32Mul | F32Min | F32Max) => {
                let Some(op) = float_bin_op_32(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), F32Const(c)] => BinOpLocalConst32(op, local, f32_const_bits(c)));
                rewrite!(instrs, i, [F32Const(c), LocalGet32(local)] => BinOpLocalConst32(op, local, f32_const_bits(c)));
            }
            instr @ (F32Sub | F32Div | F32Copysign) => {
                let Some(op) = float_bin_op_32(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(instrs, i, [LocalGet32(local), F32Const(c)] => BinOpLocalConst32(op, local, f32_const_bits(c)));
            }
            instr @ (F64Add | F64Mul | F64Min | F64Max) => {
                let Some(op) = float_bin_op_64(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), F64Const(c)] => BinOpLocalConst64(op, local, f64_const_bits(c)));
                rewrite!(instrs, i, [F64Const(c), LocalGet64(local)] => BinOpLocalConst64(op, local, f64_const_bits(c)));
            }
            instr @ (F64Sub | F64Div | F64Copysign) => {
                let Some(op) = float_bin_op_64(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(instrs, i, [LocalGet64(local), F64Const(c)] => BinOpLocalConst64(op, local, f64_const_bits(c)));
            }
            instr @ (V128And | V128Or | V128Xor | I64x2Add | I64x2Mul) => {
                let Some(op) = bin_op_128(instr) else { unreachable!() };
                rewrite!(instrs, i, [LocalGet128(a), LocalGet128(b)] => BinOpLocalLocal128(op, a, b));
                rewrite!(instrs, i, [LocalGet128(local), V128Const(c)] => BinOpLocalConst128(op, local, c));
                rewrite!(instrs, i, [V128Const(c), LocalGet128(local)] => BinOpLocalConst128(op, local, c));
            }
            V128AndNot => {
                rewrite!(instrs, i, [LocalGet128(a), LocalGet128(b)] => BinOpLocalLocal128(BinOp128::AndNot, a, b));
                rewrite!(instrs, i, [LocalGet128(local), V128Const(c)] => BinOpLocalConst128(BinOp128::AndNot, local, c));
            }
            I32Store(memarg) => {
                rewrite!(instrs, i,
                    [LocalGet32(addr_local), LocalGet32(value_local)] if
                    (let (Ok(addr_local), Ok(value_local)) = (u8::try_from(addr_local), u8::try_from(value_local))) =>
                    StoreLocalLocal32(memarg, addr_local, value_local)
                );
            }
            I64Store(memarg) => {
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
            I32Load(memarg) => {
                rewrite!(instrs, i,
                    [LocalGet32(addr_local)] if (let Ok(addr_local) = u8::try_from(addr_local)) =>
                    LoadLocal32(memarg, addr_local)
                );
            }
            MemoryFill(mem) => {
                rewrite!(instrs, i, [I32Const(val), I32Const(size)] => MemoryFillImm(mem, val as u8, size))
            }
            LocalGet32(dst) => rewrite!(instrs, i, [LocalSet32(src)] if (src == dst) => [LocalTee32(src), Nop]),
            LocalGet64(dst) => rewrite!(instrs, i, [LocalSet64(src)] if (src == dst) => [LocalTee64(src), Nop]),
            LocalGet128(dst) => rewrite!(instrs, i, [LocalSet128(src)] if (src == dst) => [LocalTee128(src), Nop]),
            LocalSet32(dst) => {
                if let Some([(lhs_idx, lhs_src), (rhs_idx, rhs_src), (op_idx, raw_op)]) = previous_non_nop_3(instrs, i)
                    && let Some((lhs_instr, lhs)) = stack_source_local_32(lhs_src)
                    && let Some(op) = scalar_bin_op_32(raw_op)
                {
                    if let Some((rhs_instr, rhs)) = stack_source_local_32(rhs_src) {
                        instrs[lhs_idx] = lhs_instr;
                        instrs[rhs_idx] = rhs_instr;
                        instrs[op_idx] = Nop;
                        instrs[i] = BinOpLocalLocalSet32(op, lhs, rhs, dst);
                    } else if let Some(imm) = scalar_const_32(rhs_src, raw_op) {
                        instrs[lhs_idx] = lhs_instr;
                        instrs[rhs_idx] = Nop;
                        instrs[op_idx] = Nop;
                        instrs[i] = match (dst == lhs, op) {
                            (true, BinOp::IAdd) => IncLocal32(dst, imm),
                            (true, BinOp::ISub) => IncLocal32(dst, imm.wrapping_neg()),
                            _ => BinOpLocalConstSet32(op, lhs, imm, dst),
                        };
                    }
                }
                rewrite!(instrs, i, [LocalGet32(src)] => if src == dst { Nop } else { LocalCopy32(src, dst) });
                rewrite!(instrs, i, [I32Const(c)] => SetLocalConst32(dst, c));
                rewrite!(instrs, i, [F32Const(c)] => SetLocalConst32(dst, i32::from_ne_bytes(c.to_bits().to_ne_bytes())));
                rewrite!(instrs, i, [BinOpLocalLocal32(op, a, b)] => BinOpLocalLocalSet32(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConst32(op, src, c)] => match (dst == src, op) {
                    (true, BinOp::IAdd) => IncLocal32(dst, c),
                    (true, BinOp::ISub) => IncLocal32(dst, c.wrapping_neg()),
                    _ => BinOpLocalConstSet32(op, src, c, dst),
                });
                rewrite!(instrs, i, [LoadLocal32(memarg, addr)] if (let Ok(dst) = u8::try_from(dst)) => LoadLocalSet32(memarg, addr, dst));
                rewrite!(instrs, i,
                    [LocalGet32(addr), I32Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalSet32(memarg, addr, dst)
                );
            }
            LocalSet64(dst) => {
                rewrite!(instrs, i, [LocalGet64(src)] => if src == dst { Nop } else { LocalCopy64(src, dst) });
                rewrite!(instrs, i,
                    [LocalTee64(src), I64Const(c), instr] if (let Some(op) = int_bin_op_64(instr)) =>
                    [LocalSet64(src), Nop, Nop, match (dst == src, op) {
                        (true, BinOp::IAdd) => IncLocal64(dst, c),
                        (true, BinOp::ISub) => IncLocal64(dst, c.wrapping_neg()),
                        _ => BinOpLocalConstSet64(op, src, c, dst),
                    }]
                );
                rewrite!(instrs, i,
                    [LocalTee64(src), F64Const(c), instr] if (let Some(op) = float_bin_op_64(instr)) =>
                    [LocalSet64(src), Nop, Nop, BinOpLocalConstSet64(op, src, f64_const_bits(c), dst)]
                );
                rewrite!(instrs, i, [I64Const(c)] => SetLocalConst64(dst, c));
                rewrite!(instrs, i, [F64Const(c)] => SetLocalConst64(dst, i64::from_ne_bytes(c.to_bits().to_ne_bytes())));
                rewrite!(instrs, i, [BinOpLocalLocal64(op, a, b)] => BinOpLocalLocalSet64(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConst64(op, src, c)] => match (dst == src, op) {
                    (true, BinOp::IAdd) => IncLocal64(dst, c),
                    (true, BinOp::ISub) => IncLocal64(dst, c.wrapping_neg()),
                    _ => BinOpLocalConstSet64(op, src, c, dst),
                });
            }
            LocalSet128(dst) => {
                rewrite!(instrs, i, [LocalGet128(src)] => if src == dst { Nop } else { LocalCopy128(src, dst) });
                rewrite!(instrs, i, [BinOpLocalLocal128(op, a, b)] => BinOpLocalLocalSet128(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConst128(op, src, c)] => BinOpLocalConstSet128(op, src, c, dst));
                rewrite!(instrs, i,
                    [LocalGet32(addr), V128Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalSet128(memarg, addr, dst)
                );
            }
            LocalTee32(dst) => {
                if let Some([(lhs_idx, lhs_src), (rhs_idx, rhs_src), (op_idx, raw_op)]) = previous_non_nop_3(instrs, i)
                    && let Some((lhs_instr, lhs)) = stack_source_local_32(lhs_src)
                    && let Some(op) = scalar_bin_op_32(raw_op)
                {
                    if let Some((rhs_instr, rhs)) = stack_source_local_32(rhs_src) {
                        instrs[lhs_idx] = lhs_instr;
                        instrs[rhs_idx] = rhs_instr;
                        instrs[op_idx] = Nop;
                        instrs[i] = BinOpLocalLocalTee32(op, lhs, rhs, dst);
                    } else if let Some(imm) = scalar_const_32(rhs_src, raw_op) {
                        instrs[lhs_idx] = lhs_instr;
                        instrs[rhs_idx] = Nop;
                        instrs[op_idx] = Nop;
                        instrs[i] = BinOpLocalConstTee32(op, lhs, imm, dst);
                    }
                }
                rewrite!(instrs, i, [LocalGet32(src)] if (src == dst) => [LocalGet32(src), Nop]);
                rewrite!(instrs, i, [BinOpLocalLocal32(op, a, b)] => BinOpLocalLocalTee32(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConst32(op, src, c)] => BinOpLocalConstTee32(op, src, c, dst));
                rewrite!(instrs, i, [I32Const(c), I32And] => AndConstTee32(c, dst));
                rewrite!(instrs, i, [I32Const(c), I32Sub] => SubConstTee32(c, dst));
                rewrite!(instrs, i,
                    [LocalGet32(addr), I32Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalTee32(memarg, addr, dst)
                );
                rewrite!(instrs, i,
                    [LoadLocal32(memarg, addr)] if (let Ok(dst) = u8::try_from(dst)) =>
                    LoadLocalTee32(memarg, addr, dst)
                );
            }
            LocalTee64(dst) => {
                rewrite!(instrs, i, [LocalGet64(src)] if (src == dst) => [LocalGet64(src), Nop]);
                rewrite!(instrs, i, [BinOpLocalLocal64(op, a, b)] => BinOpLocalLocalTee64(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConst64(op, src, c)] => BinOpLocalConstTee64(op, src, c, dst));
                rewrite!(instrs, i, [I64Const(c), I64And] => AndConstTee64(c, dst));
                rewrite!(instrs, i, [I64Const(c), I64Sub] => SubConstTee64(c, dst));
            }
            LocalTee128(dst) => {
                rewrite!(instrs, i, [LocalGet128(src)] if (src == dst) => [LocalGet128(src), Nop]);
                rewrite!(instrs, i, [BinOpLocalLocal128(op, a, b)] => BinOpLocalLocalTee128(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConst128(op, src, c)] => BinOpLocalConstTee128(op, src, c, dst));
                rewrite!(instrs, i,
                    [LocalGet32(addr), V128Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalTee128(memarg, addr, dst)
                );
            }
            Drop32 => {
                rewrite!(instrs, i, [LocalTee32(local)] => [LocalSet32(local), Nop]);
                rewrite!(instrs, i, [BinOpLocalLocalTee32(op, a, b, dst)] => BinOpLocalLocalSet32(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConstTee32(op, src, c, dst)] => BinOpLocalConstSet32(op, src, c, dst));
            }
            Drop64 => {
                rewrite!(instrs, i, [LocalTee64(local)] => [LocalSet64(local), Nop]);
                rewrite!(instrs, i, [BinOpLocalLocalTee64(op, a, b, dst)] => BinOpLocalLocalSet64(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConstTee64(op, src, c, dst)] => BinOpLocalConstSet64(op, src, c, dst));
            }
            Drop128 => {
                rewrite!(instrs, i, [LocalTee128(local)] => [LocalSet128(local), Nop]);
                rewrite!(instrs, i, [BinOpLocalLocalTee128(op, a, b, dst)] => BinOpLocalLocalSet128(op, a, b, dst));
                rewrite!(instrs, i, [BinOpLocalConstTee128(op, src, c, dst)] => BinOpLocalConstSet128(op, src, c, dst));
            }
            JumpIfZero(ip) => {
                rewrite!(instrs, i, [I32Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfNonZero32(ip)]);
                    continue;
                });
                rewrite!(instrs, i, [I64Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfNonZero64(ip)]);
                    continue;
                });
                rewrite!(instrs, i,
                    [LocalGet32(local), I32Const(imm), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalConst32 { target_ip: ip, local, imm, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i,
                    [LocalGet64(local), I64Const(imm), cmp] if
                    (let Some(op) = cmp_op_64(cmp) && let Ok(imm) = i32::try_from(imm)) =>
                    JumpCmpLocalConst64 { target_ip: ip, local, imm, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i,
                    [LocalGet32(left), LocalGet32(right), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalLocal32 { target_ip: ip, left, right, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i,
                    [LocalGet64(left), LocalGet64(right), cmp] if (let Some(op) = cmp_op_64(cmp)) =>
                    JumpCmpLocalLocal64 { target_ip: ip, left, right, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i, [I32Const(imm), cmp] if (let Some(op) = cmp_op(cmp)) => match (imm, inverse_cmp_op(op)) {
                    (0, CmpOp::Eq) => JumpIfZero32(ip),
                    (0, CmpOp::Ne) => JumpIfNonZero32(ip),
                    (imm, op) => JumpCmpStackConst32 { target_ip: ip, imm, op },
                });
                rewrite!(instrs, i, [I64Const(imm), cmp] if (let Some(op) = cmp_op_64(cmp)) => match (imm, inverse_cmp_op(op)) {
                    (0, CmpOp::Eq) => JumpIfZero64(ip),
                    (0, CmpOp::Ne) => JumpIfNonZero64(ip),
                    (imm, op) => JumpCmpStackConst64 { target_ip: ip, imm, op },
                });
            }
            JumpIfNonZero(ip) => {
                rewrite!(instrs, i, [I32Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfZero32(ip)]);
                    continue;
                });
                rewrite!(instrs, i, [I64Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfZero64(ip)]);
                    continue;
                });
                rewrite!(instrs, i,
                    [LocalGet32(local), I32Const(imm), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalConst32 { target_ip: ip, local, imm, op }
                );
                rewrite!(instrs, i,
                    [LocalGet64(local), I64Const(imm), cmp] if
                    (let Some(op) = cmp_op_64(cmp) && let Ok(imm) = i32::try_from(imm)) =>
                    JumpCmpLocalConst64 { target_ip: ip, local, imm, op }
                );
                rewrite!(instrs, i,
                    [LocalGet32(left), LocalGet32(right), cmp] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalLocal32 { target_ip: ip, left, right, op }
                );
                rewrite!(instrs, i,
                    [LocalGet64(left), LocalGet64(right), cmp] if (let Some(op) = cmp_op_64(cmp)) =>
                    JumpCmpLocalLocal64 { target_ip: ip, left, right, op }
                );
                rewrite!(instrs, i, [I32Const(imm), cmp] if (let Some(op) = cmp_op(cmp)) => match (imm, op) {
                    (0, CmpOp::Eq) => JumpIfZero32(ip),
                    (0, CmpOp::Ne) => JumpIfNonZero32(ip),
                    (imm, op) => JumpCmpStackConst32 { target_ip: ip, imm, op },
                });
                rewrite!(instrs, i, [I64Const(imm), cmp] if (let Some(op) = cmp_op_64(cmp)) => match (imm, op) {
                    (0, CmpOp::Eq) => JumpIfZero64(ip),
                    (0, CmpOp::Ne) => JumpIfNonZero64(ip),
                    (imm, op) => JumpCmpStackConst64 { target_ip: ip, imm, op },
                });
            }
            JumpCmpStackConst32 { target_ip, imm: 0, op } => match op {
                CmpOp::Eq => instrs[i] = JumpIfZero32(target_ip),
                CmpOp::Ne => instrs[i] = JumpIfNonZero32(target_ip),
                _ => {}
            },
            JumpCmpStackConst64 { target_ip, imm: 0, op } => match op {
                CmpOp::Eq => instrs[i] = JumpIfZero64(target_ip),
                CmpOp::Ne => instrs[i] = JumpIfNonZero64(target_ip),
                _ => {}
            },
            _ => {}
        }

        if track_local_memory_usage {
            uses_local_memory |= instrs[i].memory_addr().is_some_and(|mem| mem >= imported_memory_count);
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

fn stack_source_local_32(instr: Instruction) -> Option<(Instruction, u16)> {
    Some(match instr {
        Instruction::LocalGet32(local) => (Instruction::Nop, local),
        Instruction::LocalTee32(local) => (Instruction::LocalSet32(local), local),
        Instruction::BinOpLocalLocalTee32(op, a, b, local) => {
            (Instruction::BinOpLocalLocalSet32(op, a, b, local), local)
        }
        Instruction::BinOpLocalConstTee32(op, src, c, local) => {
            (Instruction::BinOpLocalConstSet32(op, src, c, local), local)
        }
        Instruction::LoadLocalTee32(memarg, addr, local) => {
            (Instruction::LoadLocalSet32(memarg, addr, local), local.into())
        }
        _ => return None,
    })
}

fn scalar_const_32(instr: Instruction, op_instr: Instruction) -> Option<i32> {
    match instr {
        Instruction::I32Const(c) if int_bin_op_32(op_instr).is_some() => Some(c),
        Instruction::F32Const(c) if float_bin_op_32(op_instr).is_some() => Some(f32_const_bits(c)),
        _ => None,
    }
}

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

fn f32_const_bits(value: f32) -> i32 {
    i32::from_ne_bytes(value.to_bits().to_ne_bytes())
}

fn f64_const_bits(value: f64) -> i64 {
    i64::from_ne_bytes(value.to_bits().to_ne_bytes())
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

fn previous_non_nop_3(instrs: &[Instruction], read: usize) -> Option<[(usize, Instruction); 3]> {
    let mut out = [(0usize, Instruction::Nop); 3];
    let mut filled = 0usize;

    for idx in (0..read).rev() {
        let instr = instrs[idx];
        if matches!(instr, Instruction::MergeBarrier) {
            return None;
        }
        if matches!(instr, Instruction::Nop) {
            continue;
        }

        out[2 - filled] = (idx, instr);
        filled += 1;
        if filled == 3 {
            return Some(out);
        }
    }

    None
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
            | Instruction::JumpIfZero(ip)
            | Instruction::JumpIfNonZero(ip)
            | Instruction::JumpIfZero32(ip)
            | Instruction::JumpIfNonZero32(ip)
            | Instruction::JumpIfZero64(ip)
            | Instruction::JumpIfNonZero64(ip)
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
