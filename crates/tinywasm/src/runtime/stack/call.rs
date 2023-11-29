use alloc::vec::Vec;

pub struct CallFrame {
    pub instr_ptr: usize,
    pub func_ptr: usize,

    pub local_addrs: Vec<usize>,
}
