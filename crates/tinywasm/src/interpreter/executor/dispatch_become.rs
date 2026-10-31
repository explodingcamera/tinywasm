use super::*;

struct Tail;

macro_rules! define_tail_dispatch {
    ($abi:literal; $executor:ident, $instr_ptr:ident, $dispatch_jump:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        #[allow(improper_ctypes_definitions)]
        type Handler = for<'store> extern $abi fn(
            &mut Executor<'store>, &[Instruction], usize, Instruction, usize
        ) -> ExecResult<()>;

        impl Tail {
            #[inline(always)]
            fn handler_for(opcode: InstructionOpcode) -> Handler {
                static HANDLERS: [Handler; InstructionOpcode::COUNT] = {
                    let mut handlers = [Tail::Unreachable as Handler; InstructionOpcode::COUNT];
                    $(handlers[InstructionOpcode::$variant as usize] = Tail::$variant;)*
                    handlers
                };
                HANDLERS[opcode as usize]
            }

            $(
                #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables, improper_ctypes_definitions)]
                extern $abi fn $variant(
                    $executor: &mut Executor<'_>,
                    instructions: &[Instruction],
                    $instr_ptr: usize,
                    instruction: Instruction,
                    height: usize,
                ) -> ExecResult<()> {
                    use tinywasm_types::Instruction::*;
                    $(let $variant($($arg),*) = &instruction else {
                        become Self::handler_mismatch($executor, instructions, $instr_ptr, instruction, height);
                    };)?
                    $(let $variant { $($field),* } = &instruction else {
                        become Self::handler_mismatch($executor, instructions, $instr_ptr, instruction, height);
                    };)?
                    $executor.value_stack.stack_32.set_height(height);
                    let flow = match execute_instruction!(
                        $executor, $instr_ptr, $dispatch_jump, $dispatch_flow, $body
                    ) {
                        Ok(flow) => flow,
                        Err(error) => return Err($executor.settle_error($instr_ptr, error)),
                    };
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
                            $executor, instructions, next_instr_ptr, instruction, $executor.value_stack.stack_32.len()
                        );
                    };
                    let handler = Self::handler_for(next.opcode());
                    become handler($executor, instructions, next_instr_ptr, next, $executor.value_stack.stack_32.len());
                }
            )*

            // Tail-call invariant failures so ordinary handlers stay frame-free.
            #[cold]
            #[inline(never)]
            #[allow(improper_ctypes_definitions)]
            extern $abi fn handler_mismatch(
                _: &mut Executor<'_>, _: &[Instruction], _: usize, _: Instruction, _: usize,
            ) -> ExecResult<()> {
                unreachable!("instruction handler mismatch")
            }

            #[cold]
            #[inline(never)]
            #[allow(improper_ctypes_definitions)]
            extern $abi fn invalid_instr_ptr(
                _: &mut Executor<'_>, _: &[Instruction], instr_ptr: usize, _: Instruction, _: usize,
            ) -> ExecResult<()> {
                unreachable!("instruction pointer {instr_ptr} out of range, this is a bug")
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
    pub(super) fn dispatch(&mut self) -> ExecResult<()> {
        let instructions = self.instructions();
        let instr_ptr = self.cf.instr_ptr;
        let instruction = instructions[instr_ptr];
        let handler = Tail::handler_for(instruction.opcode());
        let height = self.value_stack.stack_32.len();
        handler(self, instructions, instr_ptr, instruction, height)
    }
}
