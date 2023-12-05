use alloc::boxed::Box;
use tinywasm_types::{ValType, WasmValue};

#[derive(Debug)]
pub struct CallFrame {
    pub instr_ptr: usize,
    pub func_ptr: usize,

    pub locals: Box<[WasmValue]>,
    pub local_count: usize,
}

impl CallFrame {
    pub fn new<'a>(
        func_ptr: usize,
        params: &[WasmValue],
        local_types: impl Iterator<Item = &'a ValType>,
    ) -> Self {
        let mut locals = params.to_vec();
        locals.extend(local_types.map(|ty| WasmValue::default_for(*ty)));
        let locals = locals.into_boxed_slice();

        Self {
            instr_ptr: 0,
            func_ptr,
            local_count: locals.len(),
            locals,
        }
    }

    #[inline]
    pub(crate) fn set_local(&mut self, local_index: usize, value: WasmValue) {
        if local_index >= self.local_count {
            panic!("Invalid local index");
        }

        self.locals[local_index] = value;
    }

    #[inline]
    pub(crate) fn get_local(&self, local_index: usize) -> WasmValue {
        if local_index >= self.local_count {
            panic!("Invalid local index");
        }

        self.locals[local_index]
    }
}
