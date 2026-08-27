use super::targets::{finalize, resolve_jump_target, set_rewrite_target, target_boundaries};
use crate::macros::optimize::{replace, rewrite};
use crate::visit::FunctionDataBuilder as WasmFunctionData;
use crate::{ParserOptions, Result};
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};
use tinywasm_types::{
    BinOp, BinOp128, CmpOp, CompactMemoryArg, CompactMemoryOperand, GlobalUpdateOperand, I32LocalArg, Instruction,
    LocalTripleArg, LocalUpdateCmpOperand, LocalUpdateOperand, MemoryFillOperand, MemoryLocalArg, MemoryOperand,
    Operand64, Operand64Idx, Operand128, Operand128Idx, PackedOp, TargetLocalArg, ValueCounts,
};

pub(crate) struct OptimizeResult {
    pub(crate) instructions: Vec<Instruction>,
}

struct CompactOutput {
    instructions: Vec<Instruction>,
    block_start: usize,
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
    mut instructions: Vec<Instruction>,
    function_data: &mut WasmFunctionData,
    options: &ParserOptions,
    function_results: ValueCounts,
    self_func_addr: u32,
) -> Result<OptimizeResult> {
    let boundaries = target_boundaries(&instructions, function_data)?;
    let (rewritten, old_to_new) = if options.optimize_rewrite() {
        let (instructions, map) = rewrite(instructions, function_data, &boundaries, function_results, self_func_addr)?;
        (instructions, Some(map))
    } else {
        (instructions, None)
    };
    instructions = rewritten;
    finalize(&mut instructions, function_data, old_to_new.as_deref())?;
    Ok(OptimizeResult { instructions })
}

fn rewrite(
    source: Vec<Instruction>,
    data: &mut WasmFunctionData,
    boundaries: &[bool],
    function_results: ValueCounts,
    self_func_addr: u32,
) -> Result<(Vec<Instruction>, Vec<u32>)> {
    #![allow(unused_assignments)]
    use Instruction::*;

    let return_instruction = match function_results {
        ValueCounts { c32: 0, c64: 0, c128: 0 } => Some(ReturnVoid),
        ValueCounts { c32: 1, c64: 0, c128: 0 } => Some(Return32),
        ValueCounts { c32: 0, c64: 1, c128: 0 } => Some(Return64),
        ValueCounts { c32: 0, c64: 0, c128: 1 } => Some(Return128),
        _ => None,
    };
    let mut output = CompactOutput { instructions: Vec::with_capacity(source.len()), block_start: 0 };
    let mut old_to_new = alloc::vec![0; source.len() + 1];
    let mut after_terminator = false;

    for (old_index, instruction) in source.iter().copied().enumerate() {
        if boundaries[old_index] || after_terminator {
            output.block_start = output.len();
        }
        after_terminator = is_unconditional_terminator(instruction);
        old_to_new[old_index] = output.len() as u32;
        output.push(instruction);
        let mut read = output.len() - 1;

        match output[read] {
            LocalCopy32(a, b) | LocalCopy64(a, b) | LocalCopy128(a, b) if a == b => {
                output.pop();
            }
            Call(address) if address == self_func_addr => output[read] = CallSelf,
            ReturnCall(address) if address == self_func_addr => output[read] = ReturnCallSelf,
            Return if let Some(specialized) = return_instruction => output[read] = specialized,
            raw @ (I32Add | I32Mul | I32And | I32Or | I32Xor) => {
                let op = int_bin_op(raw).unwrap();
                rewrite!(output, read, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(output, read, [GlobalGet32(global)] => BinOpStackGlobal32(op, global));
                rewrite!(output, read, [LocalGet32(local)] => BinOpStackLocal32(op, local));
                if rewrite_scalar_const32(&mut output, &mut read, data, op, true)? {
                    continue;
                }
                if op == BinOp::IAdd {
                    rewrite!(output, read, [Const32(value)] => AddConst32(value));
                    rewrite!(output, read, [I32Add] => I32Add3);
                    if read > output.block_start
                        && let BinOpStackLocal32(BinOp::IAdd, local) = output[read - 1]
                    {
                        output.truncate(read - 1);
                        output.extend([LocalGet32(local), I32Add3]);
                        read = output.len() - 1;
                    }
                }
            }
            raw @ (I32Sub | I32Shl | I32ShrS | I32ShrU | I32Rotl | I32Rotr) => {
                let op = int_bin_op(raw).unwrap();
                rewrite!(output, read, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(output, read, [GlobalGet32(global)] => BinOpStackGlobal32(op, global));
                rewrite!(output, read, [LocalGet32(local)] => BinOpStackLocal32(op, local));
                if rewrite_scalar_const32(&mut output, &mut read, data, op, false)? {
                    continue;
                }
                rewrite_sign_extend32(&mut output, &mut read, data, op);
            }
            raw @ (I64Add | I64Mul | I64And | I64Or | I64Xor) => {
                let op = int_bin_op(raw).unwrap();
                rewrite!(output, read, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(output, read, [GlobalGet64(global)] => BinOpStackGlobal64(op, global));
                if rewrite_scalar_const64(&mut output, &mut read, data, op, true)? {
                    continue;
                }
                if op == BinOp::IAdd {
                    rewrite!(output, read, [Const64(index)] => AddConst64(index));
                    rewrite!(output, read, [I64Add] => I64Add3);
                }
            }
            raw @ (I64Sub | I64Shl | I64ShrS | I64ShrU | I64Rotl | I64Rotr) => {
                let op = int_bin_op(raw).unwrap();
                rewrite!(output, read, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite!(output, read, [GlobalGet64(global)] => BinOpStackGlobal64(op, global));
                if rewrite_scalar_const64(&mut output, &mut read, data, op, false)? {
                    continue;
                }
                rewrite_sign_extend64(&mut output, &mut read, data, op);
            }
            raw if cmp_op(raw).is_some() => {
                let op = cmp_op(raw).unwrap();
                rewrite!(output, read, [LocalGet32(a), LocalGet32(b)] => CmpLocalLocal32(op, a, b));
                rewrite!(output, read, [LocalGet64(a), LocalGet64(b)] => CmpLocalLocal64(op, a, b));
            }
            raw @ (F32Add | F32Mul | F32Min | F32Max) => {
                let op = float_bin_op(raw).unwrap();
                rewrite!(output, read, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(output, read, [LocalGet32(local)] => BinOpStackLocal32(op, local));
                rewrite_scalar_const32(&mut output, &mut read, data, op, true)?;
            }
            raw @ (F32Sub | F32Div | F32Copysign) => {
                let op = float_bin_op(raw).unwrap();
                rewrite!(output, read, [LocalGet32(a), LocalGet32(b)] => BinOpLocalLocal32(op, a, b));
                rewrite!(output, read, [LocalGet32(local)] => BinOpStackLocal32(op, local));
                rewrite_scalar_const32(&mut output, &mut read, data, op, false)?;
            }
            raw @ (F64Add | F64Mul | F64Min | F64Max) => {
                let op = float_bin_op(raw).unwrap();
                rewrite!(output, read, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite_scalar_const64(&mut output, &mut read, data, op, true)?;
            }
            raw @ (F64Sub | F64Div | F64Copysign) => {
                let op = float_bin_op(raw).unwrap();
                rewrite!(output, read, [LocalGet64(a), LocalGet64(b)] => BinOpLocalLocal64(op, a, b));
                rewrite_scalar_const64(&mut output, &mut read, data, op, false)?;
            }
            raw @ (V128And | V128Or | V128Xor | I64x2Add | I64x2Mul | V128AndNot) => {
                let op = bin_op_128(raw).unwrap();
                rewrite!(output, read, [LocalGet128(local)] =>
                    BinOpStackLocal128(op, local)
                );
                rewrite_vector_binop(&mut output, &mut read, data, op, raw != V128AndNot)?;
            }
            I32Store(index) | F32Store(index) => rewrite_store32(&mut output, &mut read, data, index)?,
            I64Store(index) | F64Store(index) => rewrite_store64(&mut output, &mut read, data, index)?,
            V128Store(index) => {
                if let Some(memory_arg_idx) = compact_memory_arg(data, index)? {
                    rewrite!(output, read,
                        [LocalGet32(addr), LocalGet128(value)] if
                        (let (Ok(addr), Ok(value)) = (u8::try_from(addr), u8::try_from(value))) =>
                        StoreLocalLocal128(MemoryLocalArg { memory_arg_idx, local1: addr, local2: value })
                    );
                }
            }
            I32Load(index) | F32Load(index) => {
                if let Some(memory_arg_idx) = compact_memory_arg(data, index)? {
                    rewrite!(output, read, [LocalGet32(local) | LocalGet64(local)] if
                        (let Ok(local) = u8::try_from(local)) =>
                        LoadLocal32(MemoryLocalArg { memory_arg_idx, local1: local, local2: 0 })
                    );
                }
            }
            I64Load(index) | F64Load(index) => {
                if let Some(memory_arg_idx) = compact_memory_arg(data, index)? {
                    rewrite!(output, read, [LocalGet32(local) | LocalGet64(local)] if
                        (let Ok(local) = u8::try_from(local)) =>
                        LoadLocal64(MemoryLocalArg { memory_arg_idx, local1: local, local2: 0 })
                    );
                }
            }
            raw @ (I32Load8S(index) | I32Load8U(index) | I32Load16S(index) | I32Load16U(index)) => {
                if let Some(memory_arg_idx) = compact_memory_arg(data, index)? {
                    rewrite!(output, read, [LocalGet32(local) | LocalGet64(local)] if
                        (let Ok(local) = u8::try_from(local)) => match raw {
                            I32Load8S(_) => LoadLocal8S32(MemoryLocalArg { memory_arg_idx, local1: local, local2: 0 }),
                            I32Load8U(_) => LoadLocal8U32(MemoryLocalArg { memory_arg_idx, local1: local, local2: 0 }),
                            I32Load16S(_) => LoadLocal16S32(MemoryLocalArg { memory_arg_idx, local1: local, local2: 0 }),
                            I32Load16U(_) => LoadLocal16U32(MemoryLocalArg { memory_arg_idx, local1: local, local2: 0 }),
                            _ => unreachable!(),
                        }
                    );
                }
            }
            MemoryFill(memory) => rewrite!(output, read, [Const32(value), Const32(size)] => {
                let index = data.push_operand128(Operand128::<MemoryFillOperand>::new(memory, value as u8, size))?;
                replace!(output, read, 2 => MemoryFillConst(index));
            }),
            GlobalGet32(dst) => rewrite!(output, read, [GlobalSet32(src)] if (src == dst) => GlobalTee32(src)),
            GlobalGet64(dst) => rewrite!(output, read, [GlobalSet64(src)] if (src == dst) => GlobalTee64(src)),
            GlobalGet128(dst) => rewrite!(output, read, [GlobalSet128(src)] if (src == dst) => GlobalTee128(src)),
            LocalGet32(dst) => rewrite!(output, read, [LocalSet32(src)] if (src == dst) => LocalTee32(src)),
            LocalGet64(dst) => rewrite!(output, read, [LocalSet64(src)] if (src == dst) => LocalTee64(src)),
            LocalGet128(dst) => rewrite!(output, read, [LocalSet128(src)] if (src == dst) => LocalTee128(src)),
            LocalSet32(dst) => rewrite_local_set32(&mut output, &mut read, data, dst)?,
            LocalSet64(dst) => rewrite_local_set64(&mut output, &mut read, data, dst)?,
            LocalSet128(dst) => rewrite_local_set128(&mut output, &mut read, data, dst)?,
            LocalTee32(dst) => rewrite_local_tee32(&mut output, &mut read, data, dst)?,
            LocalTee64(dst) => rewrite_local_tee64(&mut output, &mut read, data, dst)?,
            LocalTee128(dst) => rewrite_local_tee128(&mut output, &mut read, data, dst)?,
            Drop32 => {
                rewrite!(output, read, [LocalTee32(local)] => LocalSet32(local));
                rewrite!(output, read, [BinOpStackLocalTee32(op, local, dst)] => BinOpStackLocalSet32(op, local, dst));
                rewrite!(output, read, [AddLocalLocalTee32(arg)] => AddLocalLocalSet32(arg));
                rewrite!(output, read, [BinOpLocalLocalTee32(packed)] => BinOpLocalLocalSet32(packed));
                rewrite!(output, read, [BinOpLocalConstTee32(packed)] => BinOpLocalConstSet32(packed));
            }
            Drop64 => {
                rewrite!(output, read, [LocalTee64(local)] => LocalSet64(local));
                rewrite!(output, read, [BinOpLocalLocalTee64(packed)] => BinOpLocalLocalSet64(packed));
                rewrite!(output, read, [BinOpLocalConstTee64(packed)] => BinOpLocalConstSet64(packed));
            }
            Drop128 => {
                rewrite!(output, read, [LocalTee128(local)] => LocalSet128(local));
                rewrite!(output, read, [BinOpLocalLocalTee128(packed)] => BinOpLocalLocalSet128(packed));
                rewrite!(output, read, [BinOpLocalConstTee128(packed)] => BinOpLocalConstSet128(packed));
            }
            Jump(target) => rewrite_jump(&source, &mut output, read, data, old_index as u32, target)?,
            JumpIfZero32(target) => rewrite_conditional(&source, &mut output, &mut read, data, target, true)?,
            JumpIfNonZero32(target) => rewrite_conditional(&source, &mut output, &mut read, data, target, false)?,
            JumpIfZero64(target) => {
                rewrite!(output, read, [LocalGet64(local)] =>
                    JumpIfLocalZero64(TargetLocalArg { target_ip: target, local })
                );
            }
            JumpIfNonZero64(target) => {
                rewrite!(output, read, [LocalGet64(local)] =>
                    JumpIfLocalNonZero64(TargetLocalArg { target_ip: target, local })
                );
            }
            JumpCmpStackConst32(_)
            | JumpCmpStackConst64(_)
            | JumpCmpLocalConst32(_)
            | JumpCmpLocalConst64(_)
            | JumpCmpLocalLocal32(_)
            | JumpCmpLocalLocal64(_)
            | JumpIfLocalZero32(_)
            | JumpIfLocalNonZero32(_)
            | JumpIfLocalZero64(_)
            | JumpIfLocalNonZero64(_) => {}
            _ => {}
        }
    }
    for instruction in &mut output.instructions {
        match *instruction {
            Instruction::Const64(index) => {
                let value = data.operand64(index).value();
                if value == i64::from(value as i32) {
                    *instruction = Instruction::Const64Imm(value as i32);
                }
            }
            Instruction::Const128(index) => {
                if let Ok(value) = u32::try_from(u128::from_le_bytes(data.operand128(index).value())) {
                    *instruction = Instruction::Const128Imm(value);
                }
            }
            _ => {}
        }
    }
    let end = old_to_new.len() - 1;
    old_to_new[end] = output.len() as u32;
    Ok((output.instructions, old_to_new))
}

fn local_const32(data: &WasmFunctionData, instruction: Instruction) -> Option<(BinOp, u16, i32)> {
    match instruction {
        Instruction::AddLocalConst32(arg) => Some((BinOp::IAdd, arg.local, arg.value)),
        Instruction::SubLocalConst32(arg) => Some((BinOp::ISub, arg.local, arg.value)),
        Instruction::MulLocalConst32(arg) => Some((BinOp::IMul, arg.local, arg.value)),
        Instruction::BinOpLocalConst32(packed) => {
            let value = data.operand64(packed.index);
            Some((packed.op, value.a(), value.b() as i32))
        }
        _ => None,
    }
}

fn local_const64(data: &WasmFunctionData, instruction: Instruction) -> Option<(BinOp, u16, i64)> {
    let Instruction::BinOpLocalConst64(packed) = instruction else { return None };
    let value = data.operand128(packed.index);
    Some((packed.op, value.a(), value.b() as i64))
}

fn local_const128(
    data: &WasmFunctionData,
    instruction: Instruction,
) -> Option<(BinOp128, u16, Operand128Idx<[u8; 16]>)> {
    let Instruction::BinOpLocalConst128(packed) = instruction else { return None };
    let value = data.operand64(packed.index);
    Some((packed.op, value.a(), value.b()))
}

fn compact_memory_arg(
    data: &mut WasmFunctionData,
    index: Operand128Idx<MemoryOperand>,
) -> Result<Option<Operand64Idx<CompactMemoryOperand>>> {
    let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index)) else { return Ok(None) };
    Ok(Some(data.push_operand64(Operand64::from(arg))?))
}

fn rewrite_scalar_const32(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    op: BinOp,
    commutative: bool,
) -> Result<bool> {
    if *read < output.block_start + 2 {
        return Ok(false);
    }
    let previous = &output[*read - 2..*read];
    let replacement = match previous {
        [Instruction::LocalGet32(local), Instruction::Const32(value)] => {
            let arg = I32LocalArg { value: *value, local: *local };
            match op {
                BinOp::IAdd => Instruction::AddLocalConst32(arg),
                BinOp::ISub => Instruction::SubLocalConst32(arg),
                BinOp::IMul => Instruction::MulLocalConst32(arg),
                _ => Instruction::BinOpLocalConst32(PackedOp::new(
                    op,
                    data.push_operand64(Operand64::<(u16, u32)>::new(*local, *value as u32))?,
                )),
            }
        }
        [Instruction::GlobalGet32(global), Instruction::Const32(value)] => Instruction::BinOpGlobalConst32(
            PackedOp::new(op, data.push_operand64(Operand64::<(u32, u32)>::new(*global, *value as u32))?),
        ),
        [Instruction::Const32(value), Instruction::LocalGet32(local)] if commutative => {
            let arg = I32LocalArg { value: *value, local: *local };
            match op {
                BinOp::IAdd => Instruction::AddLocalConst32(arg),
                BinOp::IMul => Instruction::MulLocalConst32(arg),
                _ => Instruction::BinOpLocalConst32(PackedOp::new(
                    op,
                    data.push_operand64(Operand64::<(u16, u32)>::new(*local, *value as u32))?,
                )),
            }
        }
        [Instruction::Const32(value), Instruction::GlobalGet32(global)] if commutative => {
            Instruction::BinOpGlobalConst32(PackedOp::new(
                op,
                data.push_operand64(Operand64::<(u32, u32)>::new(*global, *value as u32))?,
            ))
        }
        _ => return Ok(false),
    };
    replace!(output, *read, 2 => replacement);
    Ok(true)
}

fn rewrite_scalar_const64(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    op: BinOp,
    commutative: bool,
) -> Result<bool> {
    if *read < output.block_start + 2 {
        return Ok(false);
    }
    let previous = [output[*read - 2], output[*read - 1]];
    let replacement = match previous {
        [Instruction::LocalGet64(local), Instruction::Const64(index)] => Instruction::BinOpLocalConst64(PackedOp::new(
            op,
            data.push_operand128(Operand128::<(u16, u64)>::new(local, data.operand64(index).value() as u64))?,
        )),
        [Instruction::GlobalGet64(global), Instruction::Const64(index)] => {
            Instruction::BinOpGlobalConst64(PackedOp::new(
                op,
                data.push_operand128(Operand128::<(u32, u64)>::new(global, data.operand64(index).value() as u64))?,
            ))
        }
        [Instruction::Const64(index), Instruction::LocalGet64(local)] if commutative => {
            Instruction::BinOpLocalConst64(PackedOp::new(
                op,
                data.push_operand128(Operand128::<(u16, u64)>::new(local, data.operand64(index).value() as u64))?,
            ))
        }
        [Instruction::Const64(index), Instruction::GlobalGet64(global)] if commutative => {
            Instruction::BinOpGlobalConst64(PackedOp::new(
                op,
                data.push_operand128(Operand128::<(u32, u64)>::new(global, data.operand64(index).value() as u64))?,
            ))
        }
        _ => return Ok(false),
    };
    replace!(output, *read, 2 => replacement);
    Ok(true)
}

fn rewrite_sign_extend32(output: &mut CompactOutput, read: &mut usize, data: &WasmFunctionData, op: BinOp) -> bool {
    if op != BinOp::IShrS || *read < output.block_start + 2 {
        return false;
    }
    if let Some((BinOp::IShl, local, shift)) = local_const32(data, output[*read - 2])
        && let Instruction::Const32(right) = output[*read - 1]
        && shift == right
        && matches!(shift, 16 | 24)
    {
        output.truncate(*read - 2);
        output.extend([
            Instruction::LocalGet32(local),
            if shift == 24 { Instruction::I32Extend8S } else { Instruction::I32Extend16S },
        ]);
        *read = output.len() - 1;
        return true;
    }
    false
}

fn rewrite_sign_extend64(output: &mut CompactOutput, read: &mut usize, data: &WasmFunctionData, op: BinOp) -> bool {
    if op != BinOp::IShrS || *read < output.block_start + 2 {
        return false;
    }
    if let Some((BinOp::IShl, local, shift)) = local_const64(data, output[*read - 2])
        && let Instruction::Const64(index) = output[*read - 1]
        && shift == data.operand64(index).value()
    {
        let instruction = match shift {
            56 => Instruction::I64Extend8S,
            48 => Instruction::I64Extend16S,
            32 => Instruction::I64Extend32S,
            _ => return false,
        };
        output.truncate(*read - 2);
        output.extend([Instruction::LocalGet64(local), instruction]);
        *read = output.len() - 1;
        return true;
    }
    false
}

fn rewrite_vector_binop(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    op: BinOp128,
    commutative: bool,
) -> Result<()> {
    if *read >= output.block_start + 2 {
        let previous = [output[*read - 2], output[*read - 1]];
        let replacement = match previous {
            [Instruction::LocalGet128(a), Instruction::LocalGet128(b)] => Instruction::BinOpLocalLocal128(op, a, b),
            [Instruction::LocalGet128(local), Instruction::Const128(value)] => Instruction::BinOpLocalConst128(
                PackedOp::new(op, data.push_operand64(Operand64::<(u16, Operand128Idx<[u8; 16]>)>::new(local, value))?),
            ),
            [Instruction::GlobalGet128(global), Instruction::Const128(value)] => {
                Instruction::BinOpGlobalConst128(PackedOp::new(
                    op,
                    data.push_operand64(Operand64::<(u32, Operand128Idx<[u8; 16]>)>::new(global, value))?,
                ))
            }
            [Instruction::Const128(value), Instruction::LocalGet128(local)] if commutative => {
                Instruction::BinOpLocalConst128(PackedOp::new(
                    op,
                    data.push_operand64(Operand64::<(u16, Operand128Idx<[u8; 16]>)>::new(local, value))?,
                ))
            }
            [Instruction::Const128(value), Instruction::GlobalGet128(global)] if commutative => {
                Instruction::BinOpGlobalConst128(PackedOp::new(
                    op,
                    data.push_operand64(Operand64::<(u32, Operand128Idx<[u8; 16]>)>::new(global, value))?,
                ))
            }
            _ => return Ok(()),
        };
        replace!(output, *read, 2 => replacement);
    }
    Ok(())
}

fn rewrite_store32(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    index: Operand128Idx<MemoryOperand>,
) -> Result<()> {
    let compact_arg = CompactMemoryArg::try_from(data.operand128(index)).ok();
    if *read > output.block_start && output[*read - 1] == Instruction::Select32 {
        replace!(output, *read, 1 => Instruction::SelectStore32(index));
        return Ok(());
    }
    if *read >= output.block_start + 3 {
        let previous = [output[*read - 3], output[*read - 2], output[*read - 1]];
        if let [
            Instruction::LocalGet32(addr) | Instruction::LocalGet64(addr),
            Instruction::LoadLocal32(arg),
            Instruction::AddConst32(1),
        ] = previous
            && { compact_arg.map(Operand64::from) == Some(data.operand64(arg.memory_arg_idx)) }
            && addr == u16::from(arg.local1)
        {
            replace!(output, *read, 3 => Instruction::IncMemoryLocal32(arg));
            return Ok(());
        }
    }
    if *read >= output.block_start + 2 {
        let previous = [output[*read - 2], output[*read - 1]];
        match previous {
            [Instruction::F32Mul, Instruction::F32Add]
                if let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index)) =>
            {
                replace!(output, *read, 2 => Instruction::FMaStoreF32(arg));
            }
            [Instruction::BinOpStackLocal32(BinOp::FMul, local), Instruction::F32Add]
                if let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index)) =>
            {
                replace!(output, *read, 2 => [Instruction::LocalGet32(local), Instruction::FMaStoreF32(arg)]);
            }
            [Instruction::LocalGet32(addr), Instruction::LocalGet32(value)]
                if let (Ok(addr), Ok(value), Some(memory_arg)) =
                    (u8::try_from(addr), u8::try_from(value), compact_arg) =>
            {
                let memory_arg_idx = data.push_operand64(Operand64::from(memory_arg))?;
                replace!(output, *read, 2 => Instruction::StoreLocalLocal32(MemoryLocalArg { memory_arg_idx, local1: addr, local2: value }));
            }
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_store64(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    index: Operand128Idx<MemoryOperand>,
) -> Result<()> {
    let compact_arg = CompactMemoryArg::try_from(data.operand128(index)).ok();
    if *read > output.block_start && output[*read - 1] == Instruction::Select64 {
        replace!(output, *read, 1 => Instruction::SelectStore64(index));
        return Ok(());
    }
    if *read >= output.block_start + 3 {
        let previous = [output[*read - 3], output[*read - 2], output[*read - 1]];
        if let [
            Instruction::LocalGet32(addr) | Instruction::LocalGet64(addr),
            Instruction::LoadLocal64(arg),
            Instruction::Const64(one),
        ] = previous
            && { compact_arg.map(Operand64::from) == Some(data.operand64(arg.memory_arg_idx)) }
            && addr == u16::from(arg.local1)
            && data.operand64(one).value() == 1
        {
            replace!(output, *read, 3 => Instruction::IncMemoryLocal64(arg));
            return Ok(());
        }
    }
    if *read >= output.block_start + 2 {
        match [output[*read - 2], output[*read - 1]] {
            [Instruction::F64Mul, Instruction::F64Add]
                if let Ok(arg) = CompactMemoryArg::try_from(data.operand128(index)) =>
            {
                replace!(output, *read, 2 => Instruction::FMaStoreF64(arg));
            }
            [Instruction::LocalGet32(addr), Instruction::LocalGet64(value)]
                if let (Ok(addr), Ok(value), Some(memory_arg)) =
                    (u8::try_from(addr), u8::try_from(value), compact_arg) =>
            {
                let memory_arg_idx = data.push_operand64(Operand64::from(memory_arg))?;
                replace!(output, *read, 2 => Instruction::StoreLocalLocal64(MemoryLocalArg { memory_arg_idx, local1: addr, local2: value }));
            }
            _ => {}
        }
    }
    Ok(())
}

macro_rules! local_const_set {
    ($data:expr, 32, $op:expr, $src:expr, $dst:expr, $value:expr, $tee:expr) => {{
        let index = $data.push_operand64(Operand64::<(u16, u16, u32)>::new($src, $dst, $value as u32))?;
        if $tee {
            Instruction::BinOpLocalConstTee32(PackedOp::new($op, index))
        } else {
            Instruction::BinOpLocalConstSet32(PackedOp::new($op, index))
        }
    }};
    ($data:expr, 64, $op:expr, $src:expr, $dst:expr, $value:expr, $tee:expr) => {{
        let index = $data.push_operand128(Operand128::<(u16, u16, u64)>::new($src, $dst, $value as u64))?;
        if $tee {
            Instruction::BinOpLocalConstTee64(PackedOp::new($op, index))
        } else {
            Instruction::BinOpLocalConstSet64(PackedOp::new($op, index))
        }
    }};
    ($data:expr, 128, $op:expr, $src:expr, $dst:expr, $value:expr, $tee:expr) => {{
        let index = $data.push_operand64(Operand64::<(u16, u16, Operand128Idx<[u8; 16]>)>::new($src, $dst, $value))?;
        if $tee {
            Instruction::BinOpLocalConstTee128(PackedOp::new($op, index))
        } else {
            Instruction::BinOpLocalConstSet128(PackedOp::new($op, index))
        }
    }};
}

fn rewrite_local_set32(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    dst: u16,
) -> Result<()> {
    if *read >= output.block_start + 2 {
        match [output[*read - 2], output[*read - 1]] {
            [Instruction::I32Mul, Instruction::BinOpStackLocal32(BinOp::IAdd, acc)] if acc == dst => {
                replace!(output, *read, 2 => Instruction::MulAccLocal32(dst));
                return Ok(());
            }
            [Instruction::F32Mul, Instruction::BinOpStackLocal32(BinOp::FAdd, acc)] if acc == dst => {
                replace!(output, *read, 2 => Instruction::FMulAccLocal32(dst));
                return Ok(());
            }
            _ => {}
        }
    }
    if *read > output.block_start {
        match output[*read - 1] {
            Instruction::LocalGet32(src) if src == dst => {
                output.truncate(*read - 1);
                *read = output.len();
                return Ok(());
            }
            Instruction::LocalGet32(src) => replace!(output, *read, 1 => Instruction::LocalCopy32(src, dst)),
            Instruction::Const32(value) => {
                replace!(output, *read, 1 => Instruction::SetLocalConst32(I32LocalArg { value, local: dst }))
            }
            Instruction::BinOpLocalLocal32(op, left, right) => {
                let replacement = if op == BinOp::IAdd {
                    Instruction::AddLocalLocalSet32(LocalTripleArg { left, right, dst })
                } else {
                    Instruction::BinOpLocalLocalSet32(PackedOp::new(
                        op,
                        data.push_operand64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?,
                    ))
                };
                replace!(output, *read, 1 => replacement);
            }
            instruction if let Some((op, src, value)) = local_const32(data, instruction) => {
                if src == dst
                    && let Some(delta) = op.inc_delta(value)
                {
                    replace!(output, *read, 1 => Instruction::IncLocal32(I32LocalArg { value: delta, local: dst }));
                } else {
                    let replacement = local_const_set!(data, 32, op, src, dst, value, false);
                    replace!(output, *read, 1 => replacement);
                }
            }
            Instruction::BinOpStackLocal32(op, local) => {
                replace!(output, *read, 1 => Instruction::BinOpStackLocalSet32(op, local, dst));
            }
            Instruction::LoadLocal32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalSet32(arg));
            }
            Instruction::LoadLocal8S32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalSet8S32(arg));
            }
            Instruction::LoadLocal8U32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalSet8U32(arg));
            }
            Instruction::LoadLocal16S32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalSet16S32(arg));
            }
            Instruction::LoadLocal16U32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalSet16U32(arg));
            }
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_local_set64(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    dst: u16,
) -> Result<()> {
    if *read >= output.block_start + 3 {
        match [output[*read - 3], output[*read - 2], output[*read - 1]] {
            [Instruction::I64Mul, Instruction::LocalGet64(acc), Instruction::I64Add] if acc == dst => {
                replace!(output, *read, 3 => Instruction::MulAccLocal64(dst));
                return Ok(());
            }
            [Instruction::F64Mul, Instruction::LocalGet64(acc), Instruction::F64Add] if acc == dst => {
                replace!(output, *read, 3 => Instruction::FMulAccLocal64(dst));
                return Ok(());
            }
            _ => {}
        }
    }
    if *read > output.block_start {
        match output[*read - 1] {
            Instruction::LocalGet64(src) if src == dst => {
                output.truncate(*read - 1);
                *read = output.len();
                return Ok(());
            }
            Instruction::LocalGet64(src) => replace!(output, *read, 1 => Instruction::LocalCopy64(src, dst)),
            Instruction::Const64(index) => {
                replace!(output, *read, 1 => Instruction::SetLocalConst64(PackedOp::new(dst, index)));
            }
            Instruction::BinOpLocalLocal64(op, left, right) => {
                let index = data.push_operand64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?;
                replace!(output, *read, 1 => Instruction::BinOpLocalLocalSet64(PackedOp::new(op, index)));
            }
            instruction if let Some((op, src, value)) = local_const64(data, instruction) => {
                if src == dst && matches!(op, BinOp::IAdd | BinOp::ISub) {
                    let delta = if op == BinOp::IAdd { value } else { value.wrapping_neg() };
                    let index = data.push_operand64(Operand64::<i64>::new(delta))?;
                    replace!(output, *read, 1 => Instruction::IncLocal64(PackedOp::new(dst, index)));
                } else {
                    let replacement = local_const_set!(data, 64, op, src, dst, value, false);
                    replace!(output, *read, 1 => replacement);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_local_set128(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    dst: u16,
) -> Result<()> {
    if *read >= output.block_start + 2
        && let [Instruction::LocalGet32(local), Instruction::V128Load(index)] = [output[*read - 2], output[*read - 1]]
        && let (Ok(local), Ok(dst), Ok(memory_arg)) =
            (u8::try_from(local), u8::try_from(dst), CompactMemoryArg::try_from(data.operand128(index)))
    {
        let memory_arg_idx = data.push_operand64(Operand64::from(memory_arg))?;
        replace!(output, *read, 2 => Instruction::LoadLocalSet128(MemoryLocalArg {
            memory_arg_idx,
            local1: local,
            local2: dst,
        }));
        return Ok(());
    }
    if *read > output.block_start {
        match output[*read - 1] {
            Instruction::LocalGet128(src) if src == dst => {
                output.truncate(*read - 1);
                *read = output.len();
                return Ok(());
            }
            Instruction::LocalGet128(src) => replace!(output, *read, 1 => Instruction::LocalCopy128(src, dst)),
            Instruction::Const128(value) => {
                replace!(output, *read, 1 => Instruction::SetLocalConst128(PackedOp::new(dst, value)))
            }
            Instruction::BinOpLocalLocal128(op, left, right) => {
                let index = data.push_operand64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?;
                replace!(output, *read, 1 => Instruction::BinOpLocalLocalSet128(PackedOp::new(op, index)));
            }
            instruction if let Some((op, src, value)) = local_const128(data, instruction) => {
                let replacement = local_const_set!(data, 128, op, src, dst, value, false);
                replace!(output, *read, 1 => replacement);
            }
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_local_tee32(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    dst: u16,
) -> Result<()> {
    if *read >= output.block_start + 2 {
        match [output[*read - 2], output[*read - 1]] {
            [Instruction::Const32(value), Instruction::I32And] => {
                replace!(output, *read, 2 => Instruction::AndConstTee32(I32LocalArg { value, local: dst }));
                return Ok(());
            }
            [Instruction::Const32(value), Instruction::I32Sub] => {
                replace!(output, *read, 2 => Instruction::SubConstTee32(I32LocalArg { value, local: dst }));
                return Ok(());
            }
            _ => {}
        }
    }
    if *read > output.block_start {
        match output[*read - 1] {
            Instruction::LocalGet32(src) if src == dst => replace!(output, *read, 1 => Instruction::LocalGet32(src)),
            Instruction::BinOpLocalLocal32(op, left, right) => {
                let replacement = if op == BinOp::IAdd {
                    Instruction::AddLocalLocalTee32(LocalTripleArg { left, right, dst })
                } else {
                    Instruction::BinOpLocalLocalTee32(PackedOp::new(
                        op,
                        data.push_operand64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?,
                    ))
                };
                replace!(output, *read, 1 => replacement);
            }
            instruction if let Some((op, src, value)) = local_const32(data, instruction) => {
                let replacement = local_const_set!(data, 32, op, src, dst, value, true);
                replace!(output, *read, 1 => replacement);
            }
            Instruction::BinOpStackLocal32(op, local) => {
                replace!(output, *read, 1 => Instruction::BinOpStackLocalTee32(op, local, dst));
            }
            Instruction::LoadLocal32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalTee32(arg));
            }
            Instruction::LoadLocal8S32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalTee8S32(arg));
            }
            Instruction::LoadLocal8U32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalTee8U32(arg));
            }
            Instruction::LoadLocal16S32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalTee16S32(arg));
            }
            Instruction::LoadLocal16U32(mut arg) if let Ok(dst) = u8::try_from(dst) => {
                arg.local2 = dst;
                replace!(output, *read, 1 => Instruction::LoadLocalTee16U32(arg));
            }
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_local_tee64(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    dst: u16,
) -> Result<()> {
    if *read >= output.block_start + 2 {
        match [output[*read - 2], output[*read - 1]] {
            [Instruction::Const64(value), Instruction::I64And] => {
                replace!(output, *read, 2 => Instruction::AndConstTee64(PackedOp::new(dst, value)));
                return Ok(());
            }
            [Instruction::Const64(value), Instruction::I64Sub] => {
                replace!(output, *read, 2 => Instruction::SubConstTee64(PackedOp::new(dst, value)));
                return Ok(());
            }
            _ => {}
        }
    }
    if *read > output.block_start {
        match output[*read - 1] {
            Instruction::LocalGet64(src) if src == dst => replace!(output, *read, 1 => Instruction::LocalGet64(src)),
            Instruction::BinOpLocalLocal64(op, left, right) => {
                let index = data.push_operand64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?;
                replace!(output, *read, 1 => Instruction::BinOpLocalLocalTee64(PackedOp::new(op, index)));
            }
            instruction if let Some((op, src, value)) = local_const64(data, instruction) => {
                let replacement = local_const_set!(data, 64, op, src, dst, value, true);
                replace!(output, *read, 1 => replacement);
            }
            _ => {}
        }
    }
    Ok(())
}

fn rewrite_local_tee128(
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    dst: u16,
) -> Result<()> {
    if *read >= output.block_start + 2
        && let [Instruction::LocalGet32(local), Instruction::V128Load(index)] = [output[*read - 2], output[*read - 1]]
        && let (Ok(local), Ok(dst), Ok(memory_arg)) =
            (u8::try_from(local), u8::try_from(dst), CompactMemoryArg::try_from(data.operand128(index)))
    {
        let memory_arg_idx = data.push_operand64(Operand64::from(memory_arg))?;
        replace!(output, *read, 2 => Instruction::LoadLocalTee128(MemoryLocalArg {
            memory_arg_idx,
            local1: local,
            local2: dst,
        }));
        return Ok(());
    }
    if *read > output.block_start {
        match output[*read - 1] {
            Instruction::LocalGet128(src) if src == dst => replace!(output, *read, 1 => Instruction::LocalGet128(src)),
            Instruction::BinOpLocalLocal128(op, left, right) => {
                let index = data.push_operand64(Operand64::<(u16, u16, u16)>::new(left, right, dst))?;
                replace!(output, *read, 1 => Instruction::BinOpLocalLocalTee128(PackedOp::new(op, index)));
            }
            instruction if let Some((op, src, value)) = local_const128(data, instruction) => {
                let replacement = local_const_set!(data, 128, op, src, dst, value, true);
                replace!(output, *read, 1 => replacement);
            }
            _ => {}
        }
    }
    Ok(())
}

fn int_bin_op(instruction: Instruction) -> Option<BinOp> {
    Some(match instruction {
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

fn float_bin_op(instruction: Instruction) -> Option<BinOp> {
    Some(match instruction {
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

fn cmp_op(instruction: Instruction) -> Option<CmpOp> {
    Some(match instruction {
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

fn bin_op_128(instruction: Instruction) -> Option<BinOp128> {
    Some(match instruction {
        Instruction::V128And => BinOp128::And,
        Instruction::V128AndNot => BinOp128::AndNot,
        Instruction::V128Or => BinOp128::Or,
        Instruction::V128Xor => BinOp128::Xor,
        Instruction::I64x2Add => BinOp128::I64x2Add,
        Instruction::I64x2Mul => BinOp128::I64x2Mul,
        _ => return None,
    })
}

fn jump_cmp_local_local(
    data: &mut WasmFunctionData,
    target: u32,
    left: u16,
    right: u16,
    op: CmpOp,
    width64: bool,
) -> Result<Instruction> {
    let index = data.push_target_operand64(Operand64::<(u32, u16, u16)>::new(target, left, right))?;
    Ok(if width64 {
        Instruction::JumpCmpLocalLocal64(PackedOp::new(op, index))
    } else {
        Instruction::JumpCmpLocalLocal32(PackedOp::new(op, index))
    })
}

fn rewrite_jump(
    source: &[Instruction],
    output: &mut CompactOutput,
    index: usize,
    data: &mut WasmFunctionData,
    old_index: u32,
    target: u32,
) -> Result<()> {
    let target = resolve_jump_target(source, data, target);
    let exit = old_index + 1;
    let body = target + 1;
    if let Some(instruction) = source.get(target as usize).copied() {
        match instruction {
            Instruction::JumpCmpLocalLocal32(side)
                if resolve_jump_target(source, data, data.operand64(side.index).a()) == exit && body > target =>
            {
                let value = data.operand64(side.index);
                output[index] = jump_cmp_local_local(data, body, value.b(), value.c(), side.op.inverse(), false)?;
                return Ok(());
            }
            Instruction::JumpCmpLocalLocal64(side)
                if resolve_jump_target(source, data, data.operand64(side.index).a()) == exit && body > target =>
            {
                let value = data.operand64(side.index);
                output[index] = jump_cmp_local_local(data, body, value.b(), value.c(), side.op.inverse(), true)?;
                return Ok(());
            }
            _ => {}
        }
    }
    if matches!(output[index], Instruction::Jump(_)) && target == exit {
        output.truncate(index);
    } else {
        set_rewrite_target(&mut output[index], data, target)?;
    }
    Ok(())
}

fn jump_cmp_stack_local(
    data: &mut WasmFunctionData,
    target: u32,
    local: u16,
    op: CmpOp,
    width64: bool,
) -> Result<Instruction> {
    let index = data.push_target_operand64(Operand64::<(u32, u16)>::new(target, local))?;
    Ok(if width64 {
        Instruction::JumpCmpStackLocal64(PackedOp::new(op, index))
    } else {
        Instruction::JumpCmpStackLocal32(PackedOp::new(op, index))
    })
}

fn jump_cmp_local_const32(
    data: &mut WasmFunctionData,
    target: u32,
    local: u16,
    value: i32,
    op: CmpOp,
) -> Result<Instruction> {
    if value == 0 {
        return Ok(match op {
            CmpOp::Eq => Instruction::JumpIfLocalZero32(TargetLocalArg { target_ip: target, local }),
            CmpOp::Ne => Instruction::JumpIfLocalNonZero32(TargetLocalArg { target_ip: target, local }),
            _ => Instruction::JumpCmpLocalConst32(PackedOp::new(
                op,
                data.push_target_operand128(Operand128::<(u32, i32, u16)>::new(target, value, local))?,
            )),
        });
    }
    Ok(Instruction::JumpCmpLocalConst32(PackedOp::new(
        op,
        data.push_target_operand128(Operand128::<(u32, i32, u16)>::new(target, value, local))?,
    )))
}

fn jump_cmp_local_const64(
    data: &mut WasmFunctionData,
    target: u32,
    local: u16,
    value: i32,
    op: CmpOp,
) -> Result<Instruction> {
    if value == 0 {
        match op {
            CmpOp::Eq => return Ok(Instruction::JumpIfLocalZero64(TargetLocalArg { target_ip: target, local })),
            CmpOp::Ne => return Ok(Instruction::JumpIfLocalNonZero64(TargetLocalArg { target_ip: target, local })),
            _ => {}
        }
    }
    Ok(Instruction::JumpCmpLocalConst64(PackedOp::new(
        op,
        data.push_target_operand128(Operand128::<(u32, i32, u16)>::new(target, value, local))?,
    )))
}

fn jump_cmp_stack_const32(data: &mut WasmFunctionData, target: u32, value: i32, op: CmpOp) -> Result<Instruction> {
    if value == 0 {
        match op {
            CmpOp::Eq => return Ok(Instruction::JumpIfZero32(target)),
            CmpOp::Ne => return Ok(Instruction::JumpIfNonZero32(target)),
            _ => {}
        }
    }
    Ok(Instruction::JumpCmpStackConst32(PackedOp::new(
        op,
        data.push_target_operand64(Operand64::<(u32, i32)>::new(target, value))?,
    )))
}

fn jump_cmp_stack_const64(data: &mut WasmFunctionData, target: u32, value: i64, op: CmpOp) -> Result<Instruction> {
    if value == 0 {
        match op {
            CmpOp::Eq => return Ok(Instruction::JumpIfZero64(target)),
            CmpOp::Ne => return Ok(Instruction::JumpIfNonZero64(target)),
            _ => {}
        }
    }
    Ok(Instruction::JumpCmpStackConst64(PackedOp::new(
        op,
        data.push_target_operand128(Operand128::<(u32, i64)>::new(target, value))?,
    )))
}

fn update_jump(
    data: &mut WasmFunctionData,
    target: u32,
    immediate: i32,
    address: u32,
    op: BinOp,
    on_zero: bool,
    global: bool,
) -> Result<Instruction> {
    if let Some(delta) = op.inc_delta(immediate) {
        Ok(if global {
            Instruction::IncGlobalJump32(
                data.push_target_operand128(Operand128::<GlobalUpdateOperand>::new(target, delta, address, on_zero))?,
            )
        } else {
            Instruction::IncLocalJump32(data.push_target_operand128(Operand128::<LocalUpdateOperand>::new(
                target,
                delta,
                address as u16,
                on_zero,
            ))?)
        })
    } else {
        Ok(if global {
            Instruction::BinOpGlobalConstJump32(PackedOp::new(
                op,
                data.push_target_operand128(Operand128::<GlobalUpdateOperand>::new(
                    target, immediate, address, on_zero,
                ))?,
            ))
        } else {
            Instruction::BinOpLocalConstJump32(PackedOp::new(
                op,
                data.push_target_operand128(Operand128::<LocalUpdateOperand>::new(
                    target,
                    immediate,
                    address as u16,
                    on_zero,
                ))?,
            ))
        })
    }
}

fn rewrite_conditional(
    source: &[Instruction],
    output: &mut CompactOutput,
    read: &mut usize,
    data: &mut WasmFunctionData,
    target: u32,
    on_zero: bool,
) -> Result<()> {
    let target = resolve_jump_target(source, data, target);
    if *read > output.block_start
        && let Instruction::BinOpLocalConstTee32(packed) = output[*read - 1]
    {
        let value = data.operand64(packed.index);
        if value.a() == value.b() {
            let replacement =
                update_jump(data, target, value.c() as i32, u32::from(value.a()), packed.op, on_zero, false)?;
            replace!(output, *read, 1 => replacement);
            return Ok(());
        }
    }
    if *read >= output.block_start + 2
        && let [Instruction::BinOpGlobalConst32(packed), Instruction::GlobalTee32(dst)] =
            [output[*read - 2], output[*read - 1]]
    {
        let value = data.operand64(packed.index);
        if value.a() == dst {
            let replacement = update_jump(data, target, value.b() as i32, dst, packed.op, on_zero, true)?;
            replace!(output, *read, 2 => replacement);
            return Ok(());
        }
    }
    if *read >= output.block_start + 3
        && let [Instruction::AddConst32(value), Instruction::LocalTee32(local), Instruction::LocalGet32(cond)] =
            [output[*read - 3], output[*read - 2], output[*read - 1]]
        && local == cond
    {
        let replacement =
            Instruction::IncStackTeeLocalJump32(
                data.push_target_operand128(Operand128::<LocalUpdateOperand>::new(target, value, local, on_zero))?,
            );
        replace!(output, *read, 3 => replacement);
        return Ok(());
    }
    if *read >= output.block_start + 2 {
        let update = match [output[*read - 2], output[*read - 1]] {
            [Instruction::AndConstTee32(arg), Instruction::LocalGet32(cond)] if arg.local == cond => {
                Some((arg.local, arg.value, Some(BinOp::IAnd)))
            }
            [Instruction::SubConstTee32(arg), Instruction::LocalGet32(cond)] if arg.local == cond => {
                Some((arg.local, arg.value.wrapping_neg(), None))
            }
            _ => None,
        };
        if let Some((local, value, op)) = update {
            let replacement = if let Some(op) = op {
                Instruction::BinOpStackConstTeeLocalJump32(PackedOp::new(
                    op,
                    data.push_target_operand128(Operand128::<LocalUpdateOperand>::new(target, value, local, on_zero))?,
                ))
            } else {
                Instruction::IncStackTeeLocalJump32(
                    data.push_target_operand128(Operand128::<LocalUpdateOperand>::new(target, value, local, on_zero))?,
                )
            };
            replace!(output, *read, 2 => replacement);
            return Ok(());
        }
    }
    if *read >= output.block_start + 3
        && let [Instruction::BinOpLocalConstTee32(packed), Instruction::LocalGet32(right), raw_cmp] =
            [output[*read - 3], output[*read - 2], output[*read - 1]]
        && let Some(mut cmp) = cmp_op(raw_cmp)
    {
        let value = data.operand64(packed.index);
        if value.a() == value.b() {
            if on_zero {
                cmp = cmp.inverse();
            }
            let replacement = if let Some(delta) = packed.op.inc_delta(value.c() as i32) {
                Instruction::IncLocalJumpCmpLocal32(PackedOp::new(
                    cmp,
                    data.push_target_operand128(Operand128::<LocalUpdateCmpOperand>::new(
                        target,
                        delta,
                        value.a(),
                        right,
                    ))?,
                ))
            } else {
                Instruction::BinOpLocalConstJumpCmpLocal32(PackedOp::new(
                    (packed.op, cmp),
                    data.push_target_operand128(Operand128::<LocalUpdateCmpOperand>::new(
                        target,
                        value.c() as i32,
                        value.a(),
                        right,
                    ))?,
                ))
            };
            replace!(output, *read, 3 => replacement);
            return Ok(());
        }
    }
    if *read >= output.block_start + 2 {
        match [output[*read - 2], output[*read - 1]] {
            [Instruction::LocalGet32(local), Instruction::I32Eqz] => {
                replace!(output, *read, 2 =>
                    if on_zero {
                        Instruction::JumpIfLocalNonZero32(TargetLocalArg { target_ip: target, local })
                    } else {
                        Instruction::JumpIfLocalZero32(TargetLocalArg { target_ip: target, local })
                    }
                );
                return Ok(());
            }
            [Instruction::LocalGet64(local), Instruction::I64Eqz] => {
                replace!(output, *read, 2 =>
                    if on_zero {
                        Instruction::JumpIfLocalNonZero64(TargetLocalArg { target_ip: target, local })
                    } else {
                        Instruction::JumpIfLocalZero64(TargetLocalArg { target_ip: target, local })
                    }
                );
                return Ok(());
            }
            [Instruction::CmpLocalLocal32(op, left, right), Instruction::I32Eqz] => {
                let op = if on_zero { op } else { op.inverse() };
                let replacement = jump_cmp_local_local(data, target, left, right, op, false)?;
                replace!(output, *read, 2 => replacement);
                return Ok(());
            }
            _ => {}
        }
    }
    if *read > output.block_start {
        let previous = output[*read - 1];
        let replacement = match previous {
            Instruction::I32Eqz => {
                Some(if on_zero { Instruction::JumpIfNonZero32(target) } else { Instruction::JumpIfZero32(target) })
            }
            Instruction::I64Eqz => {
                Some(if on_zero { Instruction::JumpIfNonZero64(target) } else { Instruction::JumpIfZero64(target) })
            }
            Instruction::CmpLocalLocal32(op, left, right) => {
                Some(jump_cmp_local_local(data, target, left, right, if !on_zero { op } else { op.inverse() }, false)?)
            }
            Instruction::CmpLocalLocal64(op, left, right) => {
                Some(jump_cmp_local_local(data, target, left, right, if !on_zero { op } else { op.inverse() }, true)?)
            }
            Instruction::LocalGet32(local) => Some(if on_zero {
                Instruction::JumpIfLocalZero32(TargetLocalArg { target_ip: target, local })
            } else {
                Instruction::JumpIfLocalNonZero32(TargetLocalArg { target_ip: target, local })
            }),
            Instruction::LocalGet64(local) => Some(if on_zero {
                Instruction::JumpIfLocalZero64(TargetLocalArg { target_ip: target, local })
            } else {
                Instruction::JumpIfLocalNonZero64(TargetLocalArg { target_ip: target, local })
            }),
            _ => None,
        };
        if let Some(replacement) = replacement {
            replace!(output, *read, 1 => replacement);
            return Ok(());
        }
    }
    if *read >= output.block_start + 3 {
        let raw_cmp = output[*read - 1];
        if let Some(mut op) = cmp_op(raw_cmp) {
            if on_zero {
                op = op.inverse();
            }
            let replacement = match [output[*read - 3], output[*read - 2]] {
                [Instruction::LocalGet32(local), Instruction::Const32(value)] => {
                    Some(jump_cmp_local_const32(data, target, local, value, op)?)
                }
                [Instruction::LocalGet64(local), Instruction::Const64(index)]
                    if let Ok(value) = i32::try_from(data.operand64(index).value()) =>
                {
                    Some(jump_cmp_local_const64(data, target, local, value, op)?)
                }
                [Instruction::LocalGet32(left), Instruction::LocalGet32(right)] => {
                    Some(jump_cmp_local_local(data, target, left, right, op, false)?)
                }
                [Instruction::LocalGet64(left), Instruction::LocalGet64(right)] => {
                    Some(jump_cmp_local_local(data, target, left, right, op, true)?)
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                replace!(output, *read, 3 => replacement);
                return Ok(());
            }
        }
    }
    if *read >= output.block_start + 2 {
        let raw_cmp = output[*read - 1];
        if let Some(mut op) = cmp_op(raw_cmp) {
            if on_zero {
                op = op.inverse();
            }
            let replacement = match output[*read - 2] {
                Instruction::LocalGet32(local) => Some(jump_cmp_stack_local(data, target, local, op, false)?),
                Instruction::LocalGet64(local) => Some(jump_cmp_stack_local(data, target, local, op, true)?),
                Instruction::Const32(value) => Some(jump_cmp_stack_const32(data, target, value, op)?),
                Instruction::Const64(index) => {
                    Some(jump_cmp_stack_const64(data, target, data.operand64(index).value(), op)?)
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                replace!(output, *read, 2 => replacement);
                return Ok(());
            }
        }
    }
    Ok(())
}

fn is_unconditional_terminator(instruction: Instruction) -> bool {
    matches!(
        instruction,
        Instruction::Unreachable
            | Instruction::Jump(_)
            | Instruction::BranchTable(_)
            | Instruction::Return
            | Instruction::ReturnVoid
            | Instruction::Return32
            | Instruction::Return64
            | Instruction::Return128
            | Instruction::ReturnCall(_)
            | Instruction::ReturnCallSelf
            | Instruction::ReturnCallIndirect(_)
            | Instruction::ReturnCallRef(_)
            | Instruction::Throw(_)
            | Instruction::ThrowRef
    )
}
