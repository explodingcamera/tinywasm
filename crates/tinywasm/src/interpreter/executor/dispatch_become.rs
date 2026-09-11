use super::*;

struct Unbudgeted;
struct Bounded;

#[cold]
#[inline(never)]
fn instruction_handler_mismatch() -> ! {
    unreachable!("instruction handler mismatch")
}

macro_rules! define_unbudgeted_tail_dispatch {
    ($executor:ident, $instr_ptr:ident, $acc32:ident, $acc64:ident, $acc_ref:ident, $dispatch_next:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        fn dispatch(
            $executor: &mut Executor<'_>,
            $instr_ptr: usize,
            instruction: Instruction,
            $acc32: u32,
            $acc64: u64,
            $acc_ref: ValueRef,
        ) -> ExecResult<()> {
            use tinywasm_types::Instruction::*;

            match &instruction {
                $($variant { .. } => become Self::$variant(
                    $executor,
                    $instr_ptr,
                    instruction,
                    $acc32,
                    $acc64,
                    $acc_ref,
                ),)*
            }
        }

        $(
            #[allow(
                non_snake_case,
                unreachable_code,
                unused_assignments,
                unused_imports,
                unused_macros,
                unused_mut,
                unused_variables
            )]
            fn $variant(
                $executor: &mut Executor<'_>,
                $instr_ptr: usize,
                instruction: Instruction,
                mut $acc32: u32,
                mut $acc64: u64,
                mut $acc_ref: ValueRef,
            ) -> ExecResult<()> {
                macro_rules! $dispatch_next {
                    ($next_instr_ptr:expr) => {{
                        let next_instr_ptr = $next_instr_ptr;
                        let instruction = $executor.func.instructions[next_instr_ptr];
                        become Self::dispatch($executor, next_instr_ptr, instruction, $acc32, $acc64, $acc_ref);
                    }};
                }
                macro_rules! $dispatch_flow {
                    ($flow:expr) => {{
                        match $flow.next_instr_ptr() {
                            Some(next_instr_ptr) => $dispatch_next!(next_instr_ptr),
                            None => return cold!(Ok(())),
                        }
                    }};
                }
                use tinywasm_types::Instruction::*;
                $(let $variant($($arg),*) = &instruction else {
                    cold!(instruction_handler_mismatch())
                };)?
                $(let $variant { $($field),* } = &instruction else {
                    cold!(instruction_handler_mismatch())
                };)?
                $body;
                $dispatch_next!($instr_ptr + 1)
            }
        )*
    };
}

macro_rules! define_bounded_tail_dispatch {
    ($executor:ident, $instr_ptr:ident, $acc32:ident, $acc64:ident, $acc_ref:ident, $dispatch_next:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        fn dispatch(
            $executor: &mut Executor<'_>,
            $instr_ptr: usize,
            instruction: Instruction,
            $acc32: u32,
            $acc64: u64,
            $acc_ref: ValueRef,
            instructions_until_checkpoint: u32,
        ) -> ExecResult<()> {
            use tinywasm_types::Instruction::*;

            match &instruction {
                $($variant { .. } => become Self::$variant(
                    $executor,
                    $instr_ptr,
                    instruction,
                    $acc32,
                    $acc64,
                    $acc_ref,
                    instructions_until_checkpoint,
                ),)*
            }
        }

        $(
            #[allow(
                non_snake_case,
                unreachable_code,
                unused_assignments,
                unused_imports,
                unused_macros,
                unused_mut,
                unused_variables
            )]
            fn $variant(
                $executor: &mut Executor<'_>,
                $instr_ptr: usize,
                instruction: Instruction,
                mut $acc32: u32,
                mut $acc64: u64,
                mut $acc_ref: ValueRef,
                instructions_until_checkpoint: u32,
            ) -> ExecResult<()> {
                macro_rules! $dispatch_next {
                    ($next_instr_ptr:expr) => {{
                        let next_instr_ptr = $next_instr_ptr;
                        if instructions_until_checkpoint == 0 {
                            return cold!({
                                $executor.cf.instr_ptr = next_instr_ptr;
                                $executor.cf.acc32 = $acc32;
                                $executor.cf.acc64 = $acc64;
                                $executor.cf.acc_ref = $acc_ref;
                                Ok(())
                            });
                        }

                        let instruction = $executor.func.instructions[next_instr_ptr];
                        become Self::dispatch(
                            $executor,
                            next_instr_ptr,
                            instruction,
                            $acc32,
                            $acc64,
                            $acc_ref,
                            instructions_until_checkpoint - 1,
                        );
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
                    cold!(instruction_handler_mismatch())
                };)?
                $(let $variant { $($field),* } = &instruction else {
                    cold!(instruction_handler_mismatch())
                };)?
                $body;
                $dispatch_next!($instr_ptr + 1)
            }
        )*
    };
}

impl Unbudgeted {
    instruction_handlers!(define_unbudgeted_tail_dispatch);
}

impl Bounded {
    instruction_handlers!(define_bounded_tail_dispatch);

    #[inline(always)]
    fn run(executor: &mut Executor<'_>) -> ExecResult<()> {
        let instr_ptr = executor.cf.instr_ptr;
        let instruction = executor.func.instructions[instr_ptr];
        Self::dispatch(
            executor,
            instr_ptr,
            instruction,
            executor.cf.acc32,
            executor.cf.acc64,
            executor.cf.acc_ref,
            CHECKPOINT_INTERVAL - 1,
        )
    }
}

impl<'store> Executor<'store> {
    #[inline(always)]
    pub(crate) fn run_to_completion(mut self) -> Result<()> {
        let instr_ptr = self.cf.instr_ptr;
        let instruction = self.func.instructions[instr_ptr];
        let (acc32, acc64, acc_ref) = (self.cf.acc32, self.cf.acc64, self.cf.acc_ref);
        Ok(Unbudgeted::dispatch(&mut self, instr_ptr, instruction, acc32, acc64, acc_ref)?)
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
