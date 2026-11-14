// These internal tail calls use Rust types even when the target requires a C ABI.
#![cfg_attr(any(tw_tail_abi = "C-unwind", tw_tail_abi = "sysv64-unwind"), expect(improper_ctypes_definitions))]

use super::instructions::{execute_instruction, instruction_handlers};
use super::*;
use crate::interpreter::simd::NativeVector;

/// Tail-dispatch handlers.
struct Tail;

macro_rules! define_tail_dispatch {
    ($abi:literal; $executor:ident, $instr_ptr:ident, $acc32:ident, $acc64:ident, $acc128:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        type Handler = for<'store> extern $abi fn(
            &mut Executor<'store>, &[Instruction], usize, Instruction, u32, f64, NativeVector
        ) -> ExecResult<()>;

        impl Tail {
                const HANDLERS: [Handler; InstructionOpcode::COUNT] = {
                    let mut handlers = [Self::Unreachable as Handler; InstructionOpcode::COUNT];
                    $(handlers[InstructionOpcode::$variant as usize] = Self::$variant;)*
                    handlers
                };

            #[inline(always)]
            fn handler_for(opcode: InstructionOpcode) -> Handler {
                Self::HANDLERS[opcode as usize]
            }

            $(
                #[expect(non_snake_case)]
                #[allow(unused_variables)]
                extern $abi fn $variant(
                    $executor: &mut Executor<'_>,
                    instructions: &[Instruction],
                    $instr_ptr: usize,
                    instruction: Instruction,
                    mut acc32_value: u32,
                    acc64_carrier: f64,
                    acc128_carrier: NativeVector,
                ) -> ExecResult<()> {
                    let mut acc64_value = acc64_carrier.to_bits();
                    let $acc32 = &mut acc32_value;
                    let $acc64 = &mut acc64_value;
                    let mut acc128_value = Value128::from_native(acc128_carrier);
                    let $acc128 = &mut acc128_value;
                    #[allow(unused_imports)]
                    use tinywasm_types::Instruction::*;
                    $(let $variant($($arg),*) = &instruction else {
                        become Self::handler_mismatch($executor, instructions, $instr_ptr, instruction, *$acc32, f64::from_bits(*$acc64), $acc128.to_native());
                    };)?
                    $(let $variant { $($field),* } = &instruction else {
                        become Self::handler_mismatch($executor, instructions, $instr_ptr, instruction, *$acc32, f64::from_bits(*$acc64), $acc128.to_native());
                    };)?
                    let flow = match execute_instruction!(
                        $executor, $instr_ptr, $acc32, $acc64, $acc128, $body
                    ) {
                        Ok(flow) => flow,
                        Err(error) => return Err($executor.settle_error($instr_ptr, error)),
                    };
                    if matches!(flow, ExecFlow::Branch(_)) {
                        let Some(target) = $executor.transition(flow, $instr_ptr) else {
                            return Ok(());
                        };
                        become Self::branch_target(
                            $executor, instructions, target, instruction, *$acc32, f64::from_bits(*$acc64), $acc128.to_native()
                        );
                    }
                    let Some(next_instr_ptr) = $executor.transition(flow, $instr_ptr) else {
                        return Ok(());
                    };
                    let instructions = if matches!(flow, ExecFlow::Jump(_)) {
                        $executor.instructions()
                    } else {
                        instructions
                    };
                    let Some(&next) = instructions.get(next_instr_ptr) else {
                        become Self::invalid_instr_ptr(
                            $executor, instructions, next_instr_ptr, instruction, *$acc32, f64::from_bits(*$acc64), $acc128.to_native()
                        );
                    };
                    let handler = Self::handler_for(next.opcode());
                    become handler($executor, instructions, next_instr_ptr, next, *$acc32, f64::from_bits(*$acc64), $acc128.to_native());
                }
            )*

            // Keep taken-branch dispatch separate so LLVM retains the conditional branch.
            #[inline(never)]
            extern $abi fn branch_target(
                executor: &mut Executor<'_>, instructions: &[Instruction], instr_ptr: usize,
                instruction: Instruction, acc32: u32, acc64: f64, acc128: NativeVector,
            ) -> ExecResult<()> {
                let Some(&next) = instructions.get(instr_ptr) else {
                    become Self::invalid_instr_ptr(executor, instructions, instr_ptr, instruction, acc32, acc64, acc128);
                };
                let handler = Self::handler_for(next.opcode());
                become handler(executor, instructions, instr_ptr, next, acc32, acc64, acc128);
            }

            // Tail-call invariant failures instead of adding diagnostic call frames.
            #[cold]
            #[inline(never)]
            extern $abi fn handler_mismatch(
                _: &mut Executor<'_>, _: &[Instruction], _: usize, _: Instruction, _: u32, _: f64, _: NativeVector,
            ) -> ExecResult<()> {
                unreachable!("instruction handler mismatch")
            }

            #[cold]
            #[inline(never)]
            extern $abi fn invalid_instr_ptr(
                _: &mut Executor<'_>, _: &[Instruction], _: usize, _: Instruction, _: u32, _: f64, _: NativeVector,
            ) -> ExecResult<()> {
                unreachable!("instruction pointer out of range")
            }
        }
    };
}

#[cfg(tw_tail_abi = "Rust")]
instruction_handlers!(define_tail_dispatch, "Rust");
#[cfg(tw_tail_abi = "C-unwind")]
instruction_handlers!(define_tail_dispatch, "C-unwind");
#[cfg(tw_tail_abi = "sysv64-unwind")]
instruction_handlers!(define_tail_dispatch, "sysv64-unwind");
#[cfg(tw_tail_abi = "rust-preserve-none")]
instruction_handlers!(define_tail_dispatch, "rust-preserve-none");

impl Executor<'_> {
    /// Dispatches across all instances until completion, suspension, a host call, or an error.
    #[inline(always)]
    pub(super) fn dispatch(&mut self, acc32: u32, acc64: u64, acc128: Value128) -> ExecResult<()> {
        let instructions = self.instructions();
        let instr_ptr = self.cf.instr_ptr;
        let instruction = instructions[instr_ptr];
        let handler = Tail::handler_for(instruction.opcode());
        handler(self, instructions, instr_ptr, instruction, acc32, f64::from_bits(acc64), acc128.to_native())
    }
}
