use crate::ParserOptions;
use alloc::vec::Vec;
use tinywasm_types::Instruction;

pub(crate) fn optimize_instructions(
    mut instructions: Vec<Instruction>,
    self_func_addr: u32,
    options: &ParserOptions,
) -> Vec<Instruction> {
    rewrite(&mut instructions, self_func_addr);
    if options.dce {
        dce(&mut instructions);
    }
    instructions
}

fn rewrite(instructions: &mut [Instruction], self_func_addr: u32) {
    for read in 0..instructions.len() {
        match instructions[read] {
            Instruction::LocalCopy32(a, b) if a == b => instructions[read] = Instruction::Nop,
            Instruction::LocalCopy64(a, b) if a == b => instructions[read] = Instruction::Nop,
            Instruction::LocalCopy128(a, b) if a == b => instructions[read] = Instruction::Nop,
            Instruction::LocalCopyRef(a, b) if a == b => instructions[read] = Instruction::Nop,
            Instruction::Call(addr) if addr == self_func_addr => instructions[read] = Instruction::CallSelf,
            Instruction::ReturnCall(addr) if addr == self_func_addr => instructions[read] = Instruction::ReturnCallSelf,
            Instruction::I32Add => {
                if read > 1
                    && let (Instruction::LocalGet32(a), Instruction::LocalGet32(b)) =
                        (instructions[read - 2], instructions[read - 1])
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::I32AddLocals(a, b);
                }

                if read > 0 {
                    match instructions[read - 1] {
                        Instruction::I32Const(c) if read > 1 => {
                            if let Instruction::LocalGet32(local) = instructions[read - 2] {
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::LocalGet32(local);
                                instructions[read] = Instruction::I32AddConst(c);
                            } else {
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::I32AddConst(c);
                            }
                        }
                        Instruction::I32Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] = Instruction::I32AddConst(c);
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
                    instructions[read] = Instruction::I64AddLocals(a, b);
                }

                if read > 0 {
                    match instructions[read - 1] {
                        Instruction::I64Const(c) if read > 1 => {
                            if let Instruction::LocalGet64(local) = instructions[read - 2] {
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::LocalGet64(local);
                                instructions[read] = Instruction::I64AddConst(c);
                            } else {
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::I64AddConst(c);
                            }
                        }
                        Instruction::I64Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] = Instruction::I64AddConst(c);
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
                    instructions[read] = Instruction::I64XorRotlConst(c);
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
                    instructions[read] = Instruction::I32StoreLocalLocal(memarg, addr_local, value_local);
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
                    instructions[read] = Instruction::I64StoreLocalLocal(memarg, addr_local, value_local);
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
            Instruction::LocalGet64(dst)
                if read > 0
                    && let Instruction::LocalSet64(src) = instructions[read - 1]
                    && src == dst =>
            {
                instructions[read - 1] = Instruction::LocalTee64(src);
                instructions[read] = Instruction::Nop;
            }
            Instruction::LocalGet128(dst)
                if read > 0
                    && let Instruction::LocalSet128(src) = instructions[read - 1]
                    && src == dst =>
            {
                instructions[read - 1] = Instruction::LocalTee128(src);
                instructions[read] = Instruction::Nop;
            }
            Instruction::LocalGetRef(dst)
                if read > 0
                    && let Instruction::LocalSetRef(src) = instructions[read - 1]
                    && src == dst =>
            {
                instructions[read - 1] = Instruction::LocalTeeRef(src);
                instructions[read] = Instruction::Nop;
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
                            instructions[read] = Instruction::LocalSetConst32(dst, c);
                        }
                        Instruction::F32Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] =
                                Instruction::LocalSetConst32(dst, i32::from_ne_bytes(c.to_bits().to_ne_bytes()));
                        }
                        _ => {}
                    }
                }

                if read > 1 {
                    match (instructions[read - 2], instructions[read - 1]) {
                        (Instruction::LocalGet32(src), Instruction::I32AddConst(c)) if src == dst => {
                            instructions[read - 2] = Instruction::Nop;
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] = Instruction::LocalAddConst32(dst, c);
                        }
                        (Instruction::LocalGet32(addr), Instruction::I32Load(memarg)) => {
                            if let (Ok(addr), Ok(dst)) = (u8::try_from(addr), u8::try_from(dst)) {
                                instructions[read - 2] = Instruction::Nop;
                                instructions[read - 1] = Instruction::Nop;
                                instructions[read] = Instruction::I32LoadLocalSet(memarg, addr, dst);
                            }
                        }
                        _ => {}
                    }
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
                            instructions[read] = Instruction::LocalSetConst64(dst, c);
                        }
                        Instruction::F64Const(c) => {
                            instructions[read - 1] = Instruction::Nop;
                            instructions[read] =
                                Instruction::LocalSetConst64(dst, i64::from_ne_bytes(c.to_bits().to_ne_bytes()));
                        }
                        _ => {}
                    }
                }

                if read > 1
                    && let (Instruction::LocalGet64(src), Instruction::I64AddConst(c)) =
                        (instructions[read - 2], instructions[read - 1])
                    && src == dst
                {
                    instructions[read - 2] = Instruction::Nop;
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::LocalAddConst64(dst, c);
                }
            }
            Instruction::LocalSet128(dst)
                if read > 0
                    && let Instruction::LocalGet128(src) = instructions[read - 1] =>
            {
                instructions[read - 1] = Instruction::Nop;
                instructions[read] = if src == dst { Instruction::Nop } else { Instruction::LocalCopy128(src, dst) };
            }
            Instruction::LocalSetRef(dst)
                if read > 0
                    && let Instruction::LocalGetRef(src) = instructions[read - 1] =>
            {
                instructions[read - 1] = Instruction::Nop;
                instructions[read] = if src == dst { Instruction::Nop } else { Instruction::LocalCopyRef(src, dst) };
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
                    instructions[read] = Instruction::I32LoadLocalTee(memarg, addr, dst);
                }
            }
            Instruction::LocalTee64(dst) if read > 0 => match instructions[read - 1] {
                Instruction::LocalGet64(src) if src == dst => {
                    instructions[read] = Instruction::Nop;
                }
                Instruction::I64XorRotlConst(c) => {
                    instructions[read - 1] = Instruction::Nop;
                    instructions[read] = Instruction::I64XorRotlConstTee(c, dst);
                }
                _ => {}
            },
            Instruction::LocalTee128(dst)
                if read > 0
                    && let Instruction::LocalGet128(src) = instructions[read - 1]
                    && src == dst =>
            {
                instructions[read] = Instruction::Nop;
            }
            Instruction::LocalTeeRef(dst)
                if read > 0
                    && let Instruction::LocalGetRef(src) = instructions[read - 1]
                    && src == dst =>
            {
                instructions[read] = Instruction::Nop;
            }

            Instruction::Drop32
                if read > 0
                    && let Instruction::LocalTee32(local) = instructions[read - 1] =>
            {
                instructions[read - 1] = Instruction::LocalSet32(local);
                instructions[read] = Instruction::Nop;
            }
            Instruction::Drop64
                if read > 0
                    && let Instruction::LocalTee64(local) = instructions[read - 1] =>
            {
                instructions[read - 1] = Instruction::LocalSet64(local);
                instructions[read] = Instruction::Nop;
            }
            Instruction::Drop128
                if read > 0
                    && let Instruction::LocalTee128(local) = instructions[read - 1] =>
            {
                instructions[read - 1] = Instruction::LocalSet128(local);
                instructions[read] = Instruction::Nop;
            }
            Instruction::DropRef
                if read > 0
                    && let Instruction::LocalTeeRef(local) = instructions[read - 1] =>
            {
                instructions[read - 1] = Instruction::LocalSetRef(local);
                instructions[read] = Instruction::Nop;
            }
            _ => {}
        }
    }
}

fn dce(instructions: &mut Vec<Instruction>) {
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
    instructions.retain_mut(|instr| {
        let ip = match instr {
            Instruction::Jump(ip)
            | Instruction::JumpIfZero(ip)
            | Instruction::JumpIfNonZero(ip)
            | Instruction::BranchTableTarget(ip)
            | Instruction::BranchTable(ip, _) => ip,
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
