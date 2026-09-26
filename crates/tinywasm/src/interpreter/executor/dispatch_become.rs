use super::*;

struct Unbudgeted;
struct Bounded;

type UnbudgetedHandler =
    for<'store, 'module> fn(&mut Executor<'store, 'module>, &[Instruction], usize, Instruction) -> ExecResult<()>;
type BoundedHandler =
    for<'store, 'module> fn(&mut Executor<'store, 'module>, usize, Instruction, u32) -> ExecResult<()>;

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

        $(
            #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables)]
            fn $variant(
                $executor: &mut Executor<'_, '_>,
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
                            ExecFlow::Complete => return cold!({
                                if !$executor.left {
                                    $executor.completed = true;
                                }
                                Ok(())
                            }),
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
        )*
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

        $(
            #[allow(non_snake_case, unreachable_code, unused_imports, unused_macros, unused_variables)]
            fn $variant(
                $executor: &mut Executor<'_, '_>,
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
                                if $executor.left {
                                    $executor.chunk_left = instructions_until_checkpoint;
                                } else {
                                    $executor.completed = true;
                                }
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
        )*
    };
}

impl Unbudgeted {
    instruction_handlers!(define_unbudgeted_tail_dispatch);

    // The handlers tail-call these cold paths instead of calling them: a call would make every
    // handler save a stack frame.

    #[cold]
    #[inline(never)]
    fn handler_mismatch(_: &mut Executor<'_, '_>, _: &[Instruction], _: usize, _: Instruction) -> ExecResult<()> {
        unreachable!("instruction handler mismatch")
    }

    #[cold]
    #[inline(never)]
    fn invalid_instr_ptr(_: &mut Executor<'_, '_>, _: &[Instruction], instr_ptr: usize, _: Instruction) -> ExecResult<()> {
        unreachable!("instruction pointer {instr_ptr} out of range, this is a bug")
    }
}

impl Bounded {
    instruction_handlers!(define_bounded_tail_dispatch);

    // Tail-called like `Unbudgeted`'s.

    #[cold]
    #[inline(never)]
    fn handler_mismatch(_: &mut Executor<'_, '_>, _: usize, _: Instruction, _: u32) -> ExecResult<()> {
        unreachable!("instruction handler mismatch")
    }

    #[cold]
    #[inline(never)]
    fn invalid_instr_ptr(_: &mut Executor<'_, '_>, instr_ptr: usize, _: Instruction, _: u32) -> ExecResult<()> {
        unreachable!("instruction pointer {instr_ptr} out of range, this is a bug")
    }

    /// Runs up to `chunk_left` (at least 1) instructions from `executor.cf`.
    #[inline(always)]
    fn run(executor: &mut Executor<'_, '_>, chunk_left: u32) -> ExecResult<()> {
        let instr_ptr = executor.cf.instr_ptr;
        let instruction = executor.func.instructions[instr_ptr];
        let handler = Self::handler_for(instruction.opcode());
        handler(executor, instr_ptr, instruction, chunk_left - 1)
    }
}

impl Executor<'_, '_> {
    /// Runs until the call completes (`None`) or continues in another module instance's frame.
    #[inline(always)]
    pub(crate) fn run_to_completion(mut self) -> Result<Option<CallFrame>> {
        loop {
            let func = self.func;
            let instructions = &func.instructions;
            let instr_ptr = self.cf.instr_ptr;
            let instruction = instructions[instr_ptr];
            let handler = Unbudgeted::handler_for(instruction.opcode());
            handler(&mut self, instructions, instr_ptr, instruction)?;
            if self.completed || self.left {
                return Ok(self.left());
            }
        }
    }

    /// Runs `chunk_left` instructions, then checkpoint by checkpoint until `time_budget` has
    /// elapsed since `start`.
    #[cfg(feature = "std")]
    #[inline(always)]
    pub(crate) fn run_with_time_budget(
        mut self,
        start: crate::std::time::Instant,
        time_budget: core::time::Duration,
        mut chunk_left: u32,
    ) -> Result<RunEnd> {
        loop {
            if chunk_left != 0 {
                Bounded::run(&mut self, chunk_left)?;
                if let Some(end) = self.run_end() {
                    return Ok(end);
                }
            }
            chunk_left = CHECKPOINT_INTERVAL;
            if start.elapsed() >= time_budget {
                return cold!(Ok(RunEnd::State(ExecState::Suspended(self.cf))));
            }
        }
    }

    /// Runs `chunk_left` instructions, then checkpoint by checkpoint until the store's fuel is out.
    #[inline(always)]
    pub(crate) fn run_with_fuel(mut self, mut chunk_left: u32) -> Result<RunEnd> {
        self.fuel_metered = true;
        loop {
            if chunk_left != 0 {
                Bounded::run(&mut self, chunk_left)?;
                if let Some(end) = self.run_end() {
                    return Ok(end);
                }
            }
            chunk_left = CHECKPOINT_INTERVAL;
            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(CHECKPOINT_INTERVAL);
            if self.store.execution_fuel == 0 {
                return cold!(Ok(RunEnd::State(ExecState::Suspended(self.cf))));
            }
        }
    }

    /// How a bounded chain that stopped ended, unless it stopped at a checkpoint.
    #[inline(always)]
    fn run_end(&self) -> Option<RunEnd> {
        if self.completed {
            return cold!(Some(RunEnd::State(ExecState::Completed)));
        }
        self.left().map(|frame| RunEnd::Left(frame, self.chunk_left))
    }
}
