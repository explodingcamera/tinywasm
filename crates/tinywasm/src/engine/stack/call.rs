use alloc::vec::Vec;

#[derive(Debug)]
pub struct CallFrame {
    pub instr_ptr: usize,
    pub func_ptr: usize,

    pub local_addrs: Vec<usize>,
}
