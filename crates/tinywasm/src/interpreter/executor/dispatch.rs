use super::*;

macro_rules! define_stable_dispatch {
    ($executor:ident, $instr_ptr:ident, $acc32:ident, $acc64:ident, $acc_ref:ident, $dispatch_next:ident, $dispatch_flow:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        #[inline(always)]
        fn exec_step(
            $executor: &mut Self,
            $instr_ptr: usize,
            mut $acc32: u32,
            mut $acc64: u64,
            mut $acc_ref: ValueRef,
        ) -> ExecResult<(ExecFlow, u32, u64, ValueRef)> {
            macro_rules! $dispatch_next {
                ($next_instr_ptr:expr) => {{
                    return Ok((ExecFlow::next($next_instr_ptr), $acc32, $acc64, $acc_ref));
                }};
            }
            macro_rules! $dispatch_flow {
                ($flow:expr) => {{
                    return Ok(($flow, $acc32, $acc64, $acc_ref));
                }};
            }
            use tinywasm_types::Instruction::*;
            match &$executor.func.instructions[$instr_ptr] {
                $($variant $(($($arg),*))? $({ $($field),* })? => $body,)*
            }
            Ok((ExecFlow::next($instr_ptr + 1), $acc32, $acc64, $acc_ref))
        }
    };
}

impl Executor<'_> {
    instruction_handlers!(define_stable_dispatch);

    #[inline(always)]
    pub(crate) fn run_to_completion(mut self) -> Result<()> {
        let mut instr_ptr = self.cf.instr_ptr;
        let (mut acc32, mut acc64, mut acc_ref) = (self.cf.acc32, self.cf.acc64, self.cf.acc_ref);
        loop {
            let (flow, next_acc32, next_acc64, next_acc_ref) =
                Self::exec_step(&mut self, instr_ptr, acc32, acc64, acc_ref)?;
            (acc32, acc64, acc_ref) = (next_acc32, next_acc64, next_acc_ref);
            match flow.next_instr_ptr() {
                Some(next_instr_ptr) => instr_ptr = next_instr_ptr,
                None => return cold!(Ok(())),
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
        let mut instr_ptr = self.cf.instr_ptr;
        let (mut acc32, mut acc64, mut acc_ref) = (self.cf.acc32, self.cf.acc64, self.cf.acc_ref);
        loop {
            for _ in 0..CHECKPOINT_INTERVAL {
                let (flow, next_acc32, next_acc64, next_acc_ref) =
                    Self::exec_step(&mut self, instr_ptr, acc32, acc64, acc_ref)?;
                (acc32, acc64, acc_ref) = (next_acc32, next_acc64, next_acc_ref);
                match flow.next_instr_ptr() {
                    Some(next_instr_ptr) => instr_ptr = next_instr_ptr,
                    None => return Ok(ExecState::Completed),
                }
            }

            if start.elapsed() >= time_budget {
                self.cf.instr_ptr = instr_ptr;
                self.cf.acc32 = acc32;
                self.cf.acc64 = acc64;
                self.cf.acc_ref = acc_ref;
                return Ok(ExecState::Suspended(self.cf));
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

        let mut instr_ptr = self.cf.instr_ptr;
        let (mut acc32, mut acc64, mut acc_ref) = (self.cf.acc32, self.cf.acc64, self.cf.acc_ref);
        loop {
            for _ in 0..CHECKPOINT_INTERVAL {
                let (flow, next_acc32, next_acc64, next_acc_ref) =
                    Self::exec_step(&mut self, instr_ptr, acc32, acc64, acc_ref)?;
                (acc32, acc64, acc_ref) = (next_acc32, next_acc64, next_acc_ref);
                match flow.next_instr_ptr() {
                    Some(next_instr_ptr) => instr_ptr = next_instr_ptr,
                    None => return Ok(ExecState::Completed),
                }
            }

            self.store.execution_fuel = self.store.execution_fuel.saturating_sub(CHECKPOINT_INTERVAL);
            if self.store.execution_fuel == 0 {
                self.cf.instr_ptr = instr_ptr;
                self.cf.acc32 = acc32;
                self.cf.acc64 = acc64;
                self.cf.acc_ref = acc_ref;
                return Ok(ExecState::Suspended(self.cf));
            }
        }
    }
}
