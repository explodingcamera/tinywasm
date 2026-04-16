use crate::ParserOptions;
use alloc::vec::Vec;
use tinywasm_types::{CmpOp, Instruction, WasmFunctionData};

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

pub(crate) fn optimize_instructions(
    mut instructions: Vec<Instruction>,
    function_data: &mut WasmFunctionData,
    self_func_addr: u32,
    options: &ParserOptions,
) -> Vec<Instruction> {
    rewrite(&mut instructions, self_func_addr);
    if options.dce {
        dce(&mut instructions, function_data);
    }
    instructions
}

fn rewrite(instructions: &mut [Instruction], self_func_addr: u32) {
    for read in 0..instructions.len() {
        match instructions[read] {
            Instruction::LocalCopy32(a, b) if a == b => instructions[read] = Instruction::Nop,
            Instruction::LocalCopy64(a, b) if a == b => instructions[read] = Instruction::Nop,
            Instruction::LocalCopy128(a, b) if a == b => instructions[read] = Instruction::Nop,
            Instruction::Call(addr) if addr == self_func_addr => instructions[read] = Instruction::CallSelf,
            Instruction::ReturnCall(addr) if addr == self_func_addr => instructions[read] = Instruction::ReturnCallSelf,
            Instruction::I32Add => {
                if read > 1
                    && let (Instruction::LocalGet32(a), Instruction::LocalGet32(b)) =
                        (instructions[read - 2], instructions[read - 1])
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::AddLocalLocal32(a, b);
                }

                if read > 0 {
                    match instructions[read - 1] {
                        Instruction::I32Const(c) if read > 1 => {
                            if let Instruction::LocalGet32(local) = instructions[read - 2] {
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::LocalGet32(local);
                                instructions[read] = Instruction::AddConst32(c);
                            } else {
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::AddConst32(c);
                            }
                        }
                        Instruction::I32Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] = Instruction::AddConst32(c);
                        }
                        _ => {}
                    }
                }
            }
            Instruction::I64Add => {
                if read > 1
                    && let (Instruction::LocalGet64(a), Instruction::LocalGet64(b)) =
                        (instructions[read - 2], instructions[read - 1])
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::AddLocalLocal64(a, b);
                }

                if read > 0 {
                    match instructions[read - 1] {
                        Instruction::I64Const(c) if read > 1 => {
                            if let Instruction::LocalGet64(local) = instructions[read - 2] {
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::LocalGet64(local);
                                instructions[read] = Instruction::AddConst64(c);
                            } else {
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::AddConst64(c);
                            }
                        }
                        Instruction::I64Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] = Instruction::AddConst64(c);
                        }
                        _ => {}
                    }
                }
            }
            Instruction::I64Rotl => {
                if read > 1
                    && let (Instruction::I64Xor, Instruction::I64Const(c)) =
                        (instructions[read - 2], instructions[read - 1])
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::XorRotlConst64(c);
                }
            }
            Instruction::I32Store(memarg) => {
                if read > 1
                    && let (Instruction::LocalGet32(addr_local), Instruction::LocalGet32(value_local)) =
                        (instructions[read - 2], instructions[read - 1])
                    && let (Ok(addr_local), Ok(value_local)) = (u8::try_from(addr_local), u8::try_from(value_local))
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::StoreLocalLocal32(memarg, addr_local, value_local);
                }
            }
            Instruction::I64Store(memarg) => {
                if read > 1
                    && let (Instruction::LocalGet32(addr_local), Instruction::LocalGet64(value_local)) =
                        (instructions[read - 2], instructions[read - 1])
                    && let (Ok(addr_local), Ok(value_local)) = (u8::try_from(addr_local), u8::try_from(value_local))
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::StoreLocalLocal64(memarg, addr_local, value_local);
                }
            }
            Instruction::I32Load(memarg) => {
                if read > 0
                    && let Instruction::LocalGet32(addr_local) = instructions[read - 1]
                    && let Ok(addr_local) = u8::try_from(addr_local)
                {
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::LoadLocal32(memarg, addr_local);
                }
            }
            Instruction::MemoryFill(mem) => {
                if read > 1
                    && let (Instruction::I32Const(val), Instruction::I32Const(size)) =
                        (instructions[read - 2], instructions[read - 1])
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::MemoryFillImm(mem, val as u8, size);
                }
            }

            Instruction::LocalGet32(dst) => {
                if read > 0
                    && let Instruction::LocalSet32(src) = instructions[read - 1]
                    && src == dst
                {
                    instructions[read - 1] = Instruction::LocalTee32(src);
                    instructions[read] = Instruction::Nop;
                }
            }
            Instruction::LocalGet64(dst) => {
                if read > 0
                    && let Instruction::LocalSet64(src) = instructions[read - 1]
                    && src == dst
                {
                    instructions[read - 1] = Instruction::LocalTee64(src);
                    instructions[read] = Instruction::Nop;
                }
            }
            Instruction::LocalGet128(dst) => {
                if read > 0
                    && let Instruction::LocalSet128(src) = instructions[read - 1]
                    && src == dst
                {
                    instructions[read - 1] = Instruction::LocalTee128(src);
                    instructions[read] = Instruction::Nop;
                }
            }
            Instruction::LocalSet32(dst) => {
                if read > 0 {
                    match instructions[read - 1] {
                        Instruction::LocalGet32(src) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] =
                                if src == dst { Instruction::Nop } else { Instruction::LocalCopy32(src, dst) };
                        }
                        Instruction::I32Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] = Instruction::SetLocalConst32(dst, c);
                        }
                        Instruction::F32Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] =
                                Instruction::SetLocalConst32(dst, i32::from_ne_bytes(c.to_bits().to_ne_bytes()));
                        }
                        _ => {}
                    }
                }

                if read > 1 {
                    match (instructions[read - 2], instructions[read - 1]) {
                        (Instruction::LocalGet32(src), Instruction::AddConst32(c)) if src == dst => {
                            instructions[read - 2] = Instruction::Nop;
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] = Instruction::AddLocalConst32(dst, c);
                        }
                        (Instruction::LocalGet32(addr), Instruction::I32Load(memarg)) => {
                            if let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst)) {
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::LoadLocalSet32(memarg, addr, dst);
                            }
                        }
                        _ => {}
                    }
                }

                if read > 0
                    && let Instruction::LoadLocal32(memarg, addr) = instructions[read - 1]
                    && let Ok(dst) = u8::try_from(dst)
                {
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::LoadLocalSet32(memarg, addr, dst);
                }
            }
            Instruction::LocalSet64(dst) => {
                if read > 0 {
                    match instructions[read - 1] {
                        Instruction::LocalGet64(src) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] =
                                if src == dst { Instruction::Nop } else { Instruction::LocalCopy64(src, dst) };
                        }
                        Instruction::I64Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] = Instruction::SetLocalConst64(dst, c);
                        }
                        Instruction::F64Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] =
                                Instruction::SetLocalConst64(dst, i64::from_ne_bytes(c.to_bits().to_ne_bytes()));
                        }
                        _ => {}
                    }
                }

                if read > 1
                    && let (Instruction::LocalGet64(src), Instruction::AddConst64(c)) =
                        (instructions[read - 2], instructions[read - 1])
                    && src == dst
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::AddLocalConst64(dst, c);
                }
            }
            Instruction::LocalSet128(dst) => {
                if read > 0
                    && let Instruction::LocalGet128(src) = instructions[read - 1]
                {
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] =
                        if src == dst { Instruction::Nop } else { Instruction::LocalCopy128(src, dst) };
                }
            }
            Instruction::LocalTee32(dst) => {
                if read > 0
                    && let Instruction::LocalGet32(src) = instructions[read - 1]
                    && src == dst
                {
                    instructions[read] = Instruction::Nop;
                }

                if read > 1
                    && let (Instruction::LocalGet32(addr), Instruction::I32Load(memarg)) =
                        (instructions[read - 2], instructions[read - 1])
                    && let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst))
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::LoadLocalTee32(memarg, addr, dst);
                }

                if read > 0
                    && let Instruction::LoadLocal32(memarg, addr) = instructions[read - 1]
                    && let Ok(dst) = u8::try_from(dst)
                {
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::LoadLocalTee32(memarg, addr, dst);
                }
            }
            Instruction::LocalTee64(dst) if read > 0 => match instructions[read - 1] {
                Instruction::LocalGet64(src) if src == dst => {
                    instructions[read] = Instruction::Nop;
                }
                Instruction::XorRotlConst64(c) => {
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::XorRotlConstTee64(c, dst);
                }
                _ => {}
            },
            Instruction::LocalTee128(dst) => {
                if read > 0
                    && let Instruction::LocalGet128(src) = instructions[read - 1]
                    && src == dst
                {
                    instructions[read] = Instruction::Nop;
                }
            }
            Instruction::Drop32 => {
                if read > 0
                    && let Instruction::LocalTee32(local) = instructions[read - 1]
                {
                    instructions[read - 1] = Instruction::LocalSet32(local);
                    instructions[read] = Instruction::Nop;
                }
            }
            Instruction::Drop64 => {
                if read > 0
                    && let Instruction::LocalTee64(local) = instructions[read - 1]
                {
                    instructions[read - 1] = Instruction::LocalSet64(local);
                    instructions[read] = Instruction::Nop;
                }
            }
            Instruction::Drop128 => {
                if read > 0
                    && let Instruction::LocalTee128(local) = instructions[read - 1]
                {
                    instructions[read - 1] = Instruction::LocalSet128(local);
                    instructions[read] = Instruction::Nop;
                }
            }
            Instruction::JumpIfZero(ip) => {
                if read > 0 && instructions[read - 1] == Instruction::I32Eqz {
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::JumpIfNonZero(ip);
                    continue;
                }

                if read > 2 {
                    match (instructions[read - 2], instructions[read - 1]) {
                        (Instruction::I32Const(imm), cmp) => {
                            if read > 3
                                && let Instruction::LocalGet32(local) = instructions[read - 3]
                                && let Some(op) = cmp_op(cmp)
                            {
                                instructions[read - 3] = Instruction::Nop;
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::JumpCmpLocalConst32 {
                                    target_ip: ip,
                                    local,
                                    imm,
                                    op: inverse_cmp_op(op),
                                };
                            }
                        }
                        (Instruction::LocalGet32(right), cmp) => {
                            if read > 3
                                && let Instruction::LocalGet32(left) = instructions[read - 3]
                                && let Some(op) = cmp_op(cmp)
                            {
                                instructions[read - 3] = Instruction::Nop;
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::JumpCmpLocalLocal32 {
                                    target_ip: ip,
                                    left,
                                    right,
                                    op: inverse_cmp_op(op),
                                };
                            }
                        }
                        _ => {}
                    }
                }
            }
            Instruction::JumpIfNonZero(ip) => {
                if read > 0 && instructions[read - 1] == Instruction::I32Eqz {
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::JumpIfZero(ip);
                    continue;
                }

                if read > 2 {
                    match (instructions[read - 2], instructions[read - 1]) {
                        (Instruction::I32Const(imm), cmp) => {
                            if read > 3
                                && let Instruction::LocalGet32(local) = instructions[read - 3]
                                && let Some(op) = cmp_op(cmp)
                            {
                                instructions[read - 3] = Instruction::Nop;
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::JumpCmpLocalConst32 { target_ip: ip, local, imm, op };
                            }
                        }
                        (Instruction::LocalGet32(right), cmp) => {
                            if read > 3
                                && let Instruction::LocalGet32(left) = instructions[read - 3]
                                && let Some(op) = cmp_op(cmp)
                            {
                                instructions[read - 3] = Instruction::Nop;
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] =
                                    Instruction::JumpCmpLocalLocal32 { target_ip: ip, left, right, op };
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn dce(instructions: &mut Vec<Instruction>, function_data: &mut WasmFunctionData) {
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
            | Instruction::JumpCmpLocalConst32 { target_ip: ip, .. }
            | Instruction::JumpCmpLocalLocal32 { target_ip: ip, .. }
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
