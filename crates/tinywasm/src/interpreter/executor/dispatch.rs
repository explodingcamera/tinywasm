use super::*;

macro_rules! define_stable_dispatch {
    ($executor:ident, $instr_ptr:ident, $dispatch_next:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        #[inline(always)]
        fn exec_step($executor: &mut Self, $instr_ptr: usize) -> ExecResult<ExecFlow> {
            macro_rules! $dispatch_next {
                ($next_instr_ptr:expr) => {{
                    return Ok(ExecFlow::Next($next_instr_ptr));
                }};
            }
            macro_rules! $dispatch_flow {
                ($flow:expr) => {{
                    return Ok($flow);
                }};
            }
            use tinywasm_types::Instruction::*;
            match &$executor.func.instructions[$instr_ptr] {
                $($variant $(($($arg),*))? $({ $($field),* })? => $body,)*
            }
            Ok(ExecFlow::Next($instr_ptr + 1))
        }
    };
}

impl Executor<'_, '_> {
    instruction_handlers!(define_stable_dispatch);

    /// Runs until the call completes (`None`) or continues in another module instance's frame.
    #[inline(always)]
    pub(crate) fn run_to_completion(mut self) -> Result<Option<CallFrame>> {
        let mut instr_ptr = self.cf.instr_ptr;
        loop {
            match Self::exec_step(&mut self, instr_ptr)?.next_instr_ptr() {
                Some(next_instr_ptr) => instr_ptr = next_instr_ptr,
                None => return cold!(Ok(self.left())),
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
        let mut instr_ptr = self.cf.instr_ptr;
        loop {
            while chunk_left != 0 {
                chunk_left -= 1;
                match Self::exec_step(&mut self, instr_ptr)?.next_instr_ptr() {
                    Some(next_instr_ptr) => instr_ptr = next_instr_ptr,
                    None => return Ok(self.run_end(chunk_left)),
                }
            }
            chunk_left = CHECKPOINT_INTERVAL;

            if start.elapsed() >= time_budget {
                self.cf.instr_ptr = instr_ptr;
                return Ok(RunEnd::State(ExecState::Suspended(self.cf)));
            }
        }
    }

    /// Runs `chunk_left` instructions, then checkpoint by checkpoint until the store's fuel is out.
    #[inline(always)]
    pub(crate) fn run_with_fuel(mut self, mut chunk_left: u32) -> Result<RunEnd> {
        self.fuel_metered = true;
        let mut instr_ptr = self.cf.instr_ptr;
        loop {
            while chunk_left != 0 {
                chunk_left -= 1;
                match Self::exec_step(&mut self, instr_ptr)?.next_instr_ptr() {
                    Some(next_instr_ptr) => instr_ptr = next_instr_ptr,
                    None => return Ok(self.run_end(chunk_left)),
                }
            }
            chunk_left = CHECKPOINT_INTERVAL;

            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(CHECKPOINT_INTERVAL);
            if self.store.execution_fuel == 0 {
                self.cf.instr_ptr = instr_ptr;
                return Ok(RunEnd::State(ExecState::Suspended(self.cf)));
            }
        }
    }

    fn run_end(&self, chunk_left: u32) -> RunEnd {
        match self.left() {
            Some(frame) => RunEnd::Left(frame, chunk_left),
            None => RunEnd::State(ExecState::Completed),
        }
    }
}
