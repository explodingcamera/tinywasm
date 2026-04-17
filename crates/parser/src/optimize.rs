use crate::macros::optimize::*;
use alloc::vec::Vec;
use tinywasm_types::{CmpOp, Instruction, WasmFunctionData};

pub(crate) fn optimize_instructions(
    mut instructions: Vec<Instruction>,
    function_data: &mut WasmFunctionData,
    self_func_addr: u32,
) -> Vec<Instruction> {
    rewrite(&mut instructions, self_func_addr);
    remove_nop(&mut instructions, function_data);
    instructions
}

fn rewrite(instrs: &mut [Instruction], self_func_addr: u32) {
    use Instruction::*;
    for i in 0..instrs.len() {
        match instrs[i] {
            LocalCopy32(a, b) if a == b => instrs[i] = Nop,
            LocalCopy64(a, b) if a == b => instrs[i] = Nop,
            LocalCopy128(a, b) if a == b => instrs[i] = Nop,
            Call(addr) if addr == self_func_addr => instrs[i] = CallSelf,
            ReturnCall(addr) if addr == self_func_addr => instrs[i] = ReturnCallSelf,
            I32Add => {
                rewrite!(instrs, i, [I32Const(c)] => AddConst32(c));
                rewrite!(instrs, i, [LocalGet32(a), LocalGet32(b)] => AddLocalLocal32(a, b));
                rewrite!(instrs, i, [LocalGet32(local), I32Const(c)] => [ Nop, LocalGet32(local), AddConst32(c)]);
            }
            I64Add => {
                rewrite!(instrs, i, [I64Const(c)] => AddConst64(c));
                rewrite!(instrs, i, [LocalGet64(a), LocalGet64(b)] => AddLocalLocal64(a, b));
                rewrite!(instrs, i, [LocalGet64(local), I64Const(c)] => [ Nop, LocalGet64(local), AddConst64(c)]);
            }
            I64Rotl => rewrite!(instrs, i, [I64Xor, I64Const(c)] => XorRotlConst64(c)),
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
                rewrite!(instrs, i, [LocalGet32(src)] => if src == dst { Nop } else { LocalCopy32(src, dst) });
                rewrite!(instrs, i, [I32Const(c)] => SetLocalConst32(dst, c));
                rewrite!(instrs, i, [F32Const(c)] => SetLocalConst32(dst, i32::from_ne_bytes(c.to_bits().to_ne_bytes())));
                rewrite!(instrs, i, [LocalGet32(src), AddConst32(c)] if (src == dst) => AddLocalConst32(dst, c));
                rewrite!(instrs, i, [LoadLocal32(memarg, addr)] if (let Ok(dst) = u8::try_from(dst)) => LoadLocalSet32(memarg, addr, dst));
                rewrite!(instrs, i,
                    [LocalGet32(addr), I32Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalSet32(memarg, addr, dst)
                );
            }
            LocalSet64(dst) => {
                rewrite!(instrs, i, [LocalGet64(src)] => if src == dst { Nop } else { LocalCopy64(src, dst) });
                rewrite!(instrs, i, [I64Const(c)] => SetLocalConst64(dst, c));
                rewrite!(instrs, i, [F64Const(c)] => SetLocalConst64(dst, i64::from_ne_bytes(c.to_bits().to_ne_bytes())));
                rewrite!(instrs, i,
                    [LocalGet64(src), AddConst64(c)] if (src == dst) =>
                    AddLocalConst64(dst, c)
                );
            }
            LocalSet128(dst) => {
                rewrite!(instrs, i, [LocalGet128(src)] => if src == dst { Nop } else { LocalCopy128(src, dst) });
                rewrite!(instrs, i,
                    [LocalGet32(addr), V128Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalSet128(memarg, addr, dst)
                );
            }
            LocalTee32(dst) => {
                rewrite!(instrs, i, [LocalGet32(src)] if (src == dst) => [LocalGet32(src), Nop]);
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
                rewrite!(instrs, i, [I64Const(c), I64And] => AndConstTee64(c, dst));
                rewrite!(instrs, i, [I64Const(c), I64Sub] => SubConstTee64(c, dst));
                rewrite!(instrs, i, [XorRotlConst64(c)] => XorRotlConstTee64(c, dst));
            }
            LocalTee128(dst) => {
                rewrite!(instrs, i, [LocalGet128(src)] if (src == dst) => [LocalGet128(src), Nop]);
                rewrite!(instrs, i,
                    [LocalGet32(addr), V128Load(memarg)] if
                    (let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))) =>
                    LoadLocalTee128(memarg, addr, dst)
                );
            }
            Drop32 => rewrite!(instrs, i, [LocalTee32(local)] => [LocalSet32(local), Nop]),
            Drop64 => rewrite!(instrs, i, [LocalTee64(local)] => [LocalSet64(local), Nop]),
            Drop128 => rewrite!(instrs, i, [LocalTee128(local)] => [LocalSet128(local), Nop]),
            JumpIfZero(ip) => {
                rewrite!(instrs, i, [I32Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfNonZero(ip)]);
                    continue;
                });
                rewrite!(instrs, i, [cmp, I32Const(imm)] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpStackConst32 { target_ip: ip, imm, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i, [cmp, I64Const(imm)] if (let Some(op) = cmp_op_64(cmp)) =>
                    JumpCmpStackConst64 { target_ip: ip, imm, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i,
                    [LocalGet32(local), cmp, I32Const(imm)] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalConst32 { target_ip: ip, local, imm, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i,
                    [LocalGet64(local), cmp, I64Const(imm)] if
                    (let Some(op) = cmp_op_64(cmp) && let Ok(imm) = i32::try_from(imm)) =>
                    JumpCmpLocalConst64 { target_ip: ip, local, imm, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i,
                    [LocalGet32(left), cmp, LocalGet32(right)] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalLocal32 { target_ip: ip, left, right, op: inverse_cmp_op(op) }
                );
                rewrite!(instrs, i,
                    [LocalGet64(left), cmp, LocalGet64(right)] if (let Some(op) = cmp_op_64(cmp)) =>
                    JumpCmpLocalLocal64 { target_ip: ip, left, right, op: inverse_cmp_op(op) }
                );
            }
            JumpIfNonZero(ip) => {
                rewrite!(instrs, i, [I32Eqz] => {
                    replace!(instrs, i, 1 => [Nop, JumpIfZero(ip)]);
                    continue;
                });
                rewrite!(instrs, i, [cmp, I32Const(imm)] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpStackConst32 { target_ip: ip, imm, op }
                );
                rewrite!(instrs, i, [cmp, I64Const(imm)] if (let Some(op) = cmp_op_64(cmp)) =>
                    JumpCmpStackConst64 { target_ip: ip, imm, op }
                );
                rewrite!(instrs, i,
                    [LocalGet32(local), cmp, I32Const(imm)] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalConst32 { target_ip: ip, local, imm, op }
                );
                rewrite!(instrs, i,
                    [LocalGet64(local), cmp, I64Const(imm)] if
                    (let Some(op) = cmp_op_64(cmp) && let Ok(imm) = i32::try_from(imm)) =>
                    JumpCmpLocalConst64 { target_ip: ip, local, imm, op }
                );
                rewrite!(instrs, i,
                    [LocalGet32(left), cmp, LocalGet32(right)] if (let Some(op) = cmp_op(cmp)) =>
                    JumpCmpLocalLocal32 { target_ip: ip, left, right, op }
                );
                rewrite!(instrs, i,
                    [LocalGet64(left), cmp, LocalGet64(right)] if (let Some(op) = cmp_op_64(cmp)) =>
                    JumpCmpLocalLocal64 { target_ip: ip, left, right, op }
                );
            }
            _ => {}
        }
    }
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

fn remove_nop(instructions: &mut Vec<Instruction>, function_data: &mut WasmFunctionData) {
    let old_len = instructions.len();
    if old_len == 0 {
        return;
    }

    let mut removed_before = Vec::with_capacity(old_len + 1);
    removed_before.push(0u32);
    instructions.iter().for_each(|instr| {
        let removed = removed_before.last().copied().unwrap_or(0) + u32::from(matches!(instr, Instruction::Nop));
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
            | Instruction::JumpCmpStackConst32 { target_ip: ip, .. }
            | Instruction::JumpCmpStackConst64 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalConst32 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalConst64 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalLocal32 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalLocal64 { target_ip: ip, .. }
            | Instruction::BranchTable(ip, _, _) => ip,
            _ => return !matches!(instr, Instruction::Nop),
        };

        let old_target = *ip as usize;
        if old_target > old_len {
            return !matches!(instr, Instruction::Nop);
        }

        *ip -= removed_before[old_target];
        debug_assert!(*ip < compacted_len, "remapped jump target points past end of function");
        !matches!(instr, Instruction::Nop)
    });
}
