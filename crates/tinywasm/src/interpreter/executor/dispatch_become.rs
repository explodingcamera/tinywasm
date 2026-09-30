use super::*;

struct Unbudgeted;
struct Bounded;

// Between two handlers only what the calling convention passes in registers stays out of memory.
// On arm64_32 (watchOS) the Rust ABI passes an aggregate larger than a pointer, the 8-byte
// `Instruction`, by reference, so every dispatch would store it and the next handler load it back.
// The C convention passes it in a register; `C-unwind` still lets a host function's panic unwind.
// Elsewhere the handlers keep the Rust ABI.
macro_rules! handler_fn {
    ($(#[$meta:meta])* fn $($rest:tt)*) => {
        #[cfg(all(target_arch = "aarch64", target_pointer_width = "32"))]
        #[allow(improper_ctypes_definitions)]
        $(#[$meta])* extern "C-unwind" fn $($rest)*
        #[cfg(not(all(target_arch = "aarch64", target_pointer_width = "32")))]
        $(#[$meta])* fn $($rest)*
    };
}

macro_rules! handler_types {
    ($($abi:literal)?) => {
        // Both sides are Rust, so the C convention's view of these types need not be FFI-safe.
        #[allow(improper_ctypes_definitions)]
        type UnbudgetedHandler =
            for<'store> $(extern $abi)? fn(&mut Executor<'store>, &[Instruction], usize, Instruction) -> ExecResult<()>;
        #[allow(improper_ctypes_definitions)]
        type BoundedHandler = for<'store> $(extern $abi)? fn(&mut Executor<'store>, usize, Instruction, u32) -> ExecResult<()>;
    };
}
#[cfg(all(target_arch = "aarch64", target_pointer_width = "32"))]
handler_types!("C-unwind");
#[cfg(not(all(target_arch = "aarch64", target_pointer_width = "32")))]
handler_types!();

macro_rules! define_unbudgeted_tail_dispatch {
    ($executor:ident, $instr_ptr:ident, $dispatch_next:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        #[inline(always)]
        fn handler_for(opcode: InstructionOpcode) -> UnbudgetedHandler {
            static HANDLERS: [UnbudgetedHandler; InstructionOpcode::COUNT] = {
                let mut handlers = [Unbudgeted::Unreachable as UnbudgetedHandler; InstructionOpcode::COUNT];
                $(handlers[InstructionOpcode::$variant as usize] = Unbudgeted::$variant;)*
                handlers
            };
            HANDLERS[opcode as usize]
        }

        $(handler_fn! {
            #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables)]
            fn $variant(
                $executor: &mut Executor<'_>,
                instructions: &[Instruction],
                $instr_ptr: usize,
                instruction: Instruction,
            ) -> ExecResult<()> {
                macro_rules! $dispatch_next {
                    ($next_instr_ptr:expr) => {{
                        let next_instr_ptr = $next_instr_ptr;
                        let Some(&next) = instructions.get(next_instr_ptr) else {
                            become Self::invalid_instr_ptr($executor, instructions, next_instr_ptr, instruction);
                        };
                        let handler = Self::handler_for(next.opcode());
                        become handler($executor, instructions, next_instr_ptr, next);
                    }};
                }
                macro_rules! $dispatch_flow {
                    ($flow:expr) => {{
                        match $flow {
                            ExecFlow::Next(next_instr_ptr) => $dispatch_next!(next_instr_ptr),
                            ExecFlow::Switch(next_instr_ptr) => {
                                $executor.cf.instr_ptr = next_instr_ptr;
                                return Ok(());
                            },
                            ExecFlow::Complete => return cold!({ $executor.completed = true; Ok(()) }),
                        }
                    }};
                }
                use tinywasm_types::Instruction::*;
                $(let $variant($($arg),*) = &instruction else {
                    become Self::handler_mismatch($executor, instructions, $instr_ptr, instruction);
                };)?
                $(let $variant { $($field),* } = &instruction else {
                    become Self::handler_mismatch($executor, instructions, $instr_ptr, instruction);
                };)?
                $body;
                $dispatch_next!($instr_ptr + 1)
            }
        })*
    };
}

macro_rules! define_bounded_tail_dispatch {
    ($executor:ident, $instr_ptr:ident, $dispatch_next:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        #[inline(always)]
        fn handler_for(opcode: InstructionOpcode) -> BoundedHandler {
            static HANDLERS: [BoundedHandler; InstructionOpcode::COUNT] = {
                let mut handlers = [Bounded::Unreachable as BoundedHandler; InstructionOpcode::COUNT];
                $(handlers[InstructionOpcode::$variant as usize] = Bounded::$variant;)*
                handlers
            };
            HANDLERS[opcode as usize]
        }

        $(handler_fn! {
            #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables)]
            fn $variant(
                $executor: &mut Executor<'_>,
                $instr_ptr: usize,
                instruction: Instruction,
                instructions_until_checkpoint: u32,
            ) -> ExecResult<()> {
                macro_rules! $dispatch_next {
                    ($next_instr_ptr:expr) => {{
                        let next_instr_ptr = $next_instr_ptr;
                        if instructions_until_checkpoint == 0 {
                            return cold!({
                                $executor.cf.instr_ptr = next_instr_ptr;
                                Ok(())
                            });
                        }

                        let Some(&next) = $executor.func.instructions.get(next_instr_ptr) else {
                            become Self::invalid_instr_ptr($executor, next_instr_ptr, instruction, instructions_until_checkpoint);
                        };
                        let handler = Self::handler_for(next.opcode());
                        become handler($executor, next_instr_ptr, next, instructions_until_checkpoint - 1);
                    }};
                }
                macro_rules! $dispatch_flow {
                    ($flow:expr) => {{
                        match $flow.next_instr_ptr() {
                            Some(next_instr_ptr) => $dispatch_next!(next_instr_ptr),
                            None => return cold!({
                                $executor.completed = true;
                                Ok(())
                            }),
                        }
                    }};
                }
                use tinywasm_types::Instruction::*;
                $(let $variant($($arg),*) = &instruction else {
                    become Self::handler_mismatch($executor, $instr_ptr, instruction, instructions_until_checkpoint);
                };)?
                $(let $variant { $($field),* } = &instruction else {
                    become Self::handler_mismatch($executor, $instr_ptr, instruction, instructions_until_checkpoint);
                };)?
                $body;
                $dispatch_next!($instr_ptr + 1)
            }
        })*
    };
}

impl Unbudgeted {
    instruction_handlers!(define_unbudgeted_tail_dispatch);

    // The handlers tail-call these cold paths instead of calling them: a call would make every
    // handler save a stack frame.

    handler_fn! {
        #[cold]
        #[inline(never)]
        fn handler_mismatch(_: &mut Executor<'_>, _: &[Instruction], _: usize, _: Instruction) -> ExecResult<()> {
            unreachable!("instruction handler mismatch")
        }
    }

    handler_fn! {
        #[cold]
        #[inline(never)]
        fn invalid_instr_ptr(_: &mut Executor<'_>, _: &[Instruction], instr_ptr: usize, _: Instruction) -> ExecResult<()> {
            unreachable!("instruction pointer {instr_ptr} out of range, this is a bug")
        }
    }
}

impl Bounded {
    instruction_handlers!(define_bounded_tail_dispatch);

    // Tail-called like `Unbudgeted`'s.

    handler_fn! {
        #[cold]
        #[inline(never)]
        fn handler_mismatch(_: &mut Executor<'_>, _: usize, _: Instruction, _: u32) -> ExecResult<()> {
            unreachable!("instruction handler mismatch")
        }
    }

    handler_fn! {
        #[cold]
        #[inline(never)]
        fn invalid_instr_ptr(_: &mut Executor<'_>, instr_ptr: usize, _: Instruction, _: u32) -> ExecResult<()> {
            unreachable!("instruction pointer {instr_ptr} out of range, this is a bug")
        }
    }

    #[inline(always)]
    fn run(executor: &mut Executor<'_>) -> ExecResult<()> {
        let instr_ptr = executor.cf.instr_ptr;
        let instruction = executor.func.instructions[instr_ptr];
        let handler = Self::handler_for(instruction.opcode());
        handler(executor, instr_ptr, instruction, CHECKPOINT_INTERVAL - 1)
    }
}

impl<'store> Executor<'store> {
    #[inline(always)]
    pub(crate) fn run_to_completion(mut self) -> Result<()> {
        loop {
            let func = self.func.clone();
            let instructions = &func.instructions;
            let instr_ptr = self.cf.instr_ptr;
            let instruction = instructions[instr_ptr];
            let handler = Unbudgeted::handler_for(instruction.opcode());
            handler(&mut self, instructions, instr_ptr, instruction)?;
            if self.completed {
                return Ok(());
            }
        }
    }

    #[cfg(feature = "std")]
    #[inline(always)]
    pub(crate) fn run_with_time_budget(mut self, time_budget: core::time::Duration) -> Result<ExecState> {
        use crate::std::time::Instant;

        if time_budget.is_zero() {
            return Ok(ExecState::Suspended(self.cf));
        }
        let start = Instant::now();

        loop {
            Bounded::run(&mut self)?;
            if self.completed {
                return cold!(Ok(ExecState::Completed));
            }
            if start.elapsed() >= time_budget {
                return cold!(Ok(ExecState::Suspended(self.cf)));
            }
        }
    }

    #[inline(always)]
    pub(crate) fn run_with_fuel(mut self, fuel: u32) -> Result<ExecState> {
        self.fuel_metered = true;
        self.store.execution_fuel = fuel;
        if self.store.execution_fuel == 0 {
            return Ok(ExecState::Suspended(self.cf));
        }

        loop {
            Bounded::run(&mut self)?;
            if self.completed {
                return cold!(Ok(ExecState::Completed));
            }
            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(CHECKPOINT_INTERVAL);
            if self.store.execution_fuel == 0 {
                return cold!(Ok(ExecState::Suspended(self.cf)));
            }
        }
    }
}
