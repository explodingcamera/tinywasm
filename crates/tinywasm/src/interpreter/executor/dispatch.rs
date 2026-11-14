use super::instructions::{execute_instruction, instruction_handlers};
use super::*;

macro_rules! define_stable_dispatch {
    ($executor:ident, $instr_ptr:ident, $acc32:ident, $acc64:ident, $acc128:ident;
     $($variant:ident $(($($arg:pat),*))? $({ $($field:ident),* })? => $body:expr),* $(,)?) => {
        #[inline(always)]
        fn exec_step(
            $executor: &mut Self, $instr_ptr: usize, instruction: Instruction,
            $acc32: &mut u32, $acc64: &mut u64,
            $acc128: &mut Value128,
        ) -> ExecResult<ExecFlow> {
            use tinywasm_types::Instruction::*;
            match &instruction {
                $($variant $(($($arg),*))? $({ $($field),* })? => {
                    execute_instruction!($executor, $instr_ptr, $acc32, $acc64, $acc128, $body)
                },)*
            }
        }
    };
}

impl Executor<'_> {
    instruction_handlers!(define_stable_dispatch);

    /// Dispatches across all instances until completion, suspension, a host call, or an error.
    #[inline(always)]
    pub(super) fn dispatch(&mut self, mut acc32: u32, mut acc64: u64, mut acc128: Value128) -> ExecResult<()> {
        let mut instructions = self.instructions();
        let mut instr_ptr = self.cf.instr_ptr;
        loop {
            let instruction = instructions[instr_ptr];
            let flow = match Self::exec_step(self, instr_ptr, instruction, &mut acc32, &mut acc64, &mut acc128) {
                Ok(flow) => flow,
                Err(error) => return Err(self.settle_error(instr_ptr, error)),
            };
            if matches!(flow, ExecFlow::Jump(_)) {
                instructions = self.instructions();
            }
            match self.transition(flow, instr_ptr) {
                Some(next_instr_ptr) => instr_ptr = next_instr_ptr,
                None => return Ok(()),
            }
        }
    }
}
