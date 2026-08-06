use alloc::vec::Vec;

use super::*;

#[derive(Default)]
/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
pub(crate) struct State {
    // Concrete type indexes in store instances address this canonical type space.
    pub(crate) canonical_types: Vec<SubType>,
    pub(crate) canonical_rec_group_lengths: Vec<u32>,
    pub(crate) funcs: Vec<FunctionInstance>,
    pub(crate) tables: Vec<TableInstance>,
    pub(crate) memories: Vec<MemoryInstance>,
    pub(crate) globals: Vec<GlobalInstance>,
    pub(crate) elements: Vec<ElementInstance>,
    pub(crate) data: Vec<DataInstance>,
}

impl State {
    #[inline]
    pub(crate) fn get_func_type(&self, addr: FuncAddr) -> &FuncType {
        self.get_canonical_func_type(self.get_func(addr).type_addr)
    }

    #[inline]
    pub(crate) fn get_type(&self, addr: TypeAddr) -> &SubType {
        Self::get(&self.canonical_types, addr, "canonical type")
    }

    #[inline]
    pub(crate) fn get_canonical_func_type(&self, addr: TypeAddr) -> &FuncType {
        self.get_type(addr).as_func().expect("validated function address references a function type")
    }

    pub(crate) fn value_matches_type(&self, value: WasmValue, expected: WasmType) -> bool {
        match (value, expected) {
            (WasmValue::Ref(RefValue::Null), WasmType::Ref(expected)) => expected.is_nullable(),
            (WasmValue::Ref(RefValue::Func(func)), WasmType::Ref(expected)) => {
                self.funcs.get(func.addr() as usize).is_some_and(|func| match expected.type_index() {
                    Some(expected) => func.type_addr == expected,
                    None => matches!(expected.abstract_heap_type(), Some(AbstractHeapType::Func)),
                })
            }
            (_, WasmType::Ref(expected)) if expected.is_concrete() => false,
            _ => value.matches_type(expected),
        }
    }

    pub(super) fn get<'a, T>(items: &'a [T], addr: Addr, kind: &str) -> &'a T {
        items.get(addr as usize).unwrap_or_else(|| unreachable!("invalid {kind} address: {addr}"))
    }

    fn get_mut<'a, T>(items: &'a mut [T], addr: Addr, kind: &str) -> &'a mut T {
        items.get_mut(addr as usize).unwrap_or_else(|| unreachable!("invalid {kind} address: {addr}"))
    }

    fn get_disjoint_mut<'a, T>(items: &'a mut [T], addr: Addr, addr2: Addr, kind: &str) -> (&'a mut T, &'a mut T) {
        let [item_a, item_b] = items
            .get_disjoint_mut([addr as usize, addr2 as usize])
            .unwrap_or_else(|_| unreachable!("invalid {kind} addresses: {addr}, {addr2}"));
        (item_a, item_b)
    }

    /// Get the function at the actual index in the store
    pub(crate) fn get_func(&self, addr: FuncAddr) -> &FunctionInstance {
        Self::get(&self.funcs, addr, "function")
    }

    /// Get a wasm function at the actual index in the store, panicking if it's a host function (which should be guaranteed by the validator)
    pub(crate) fn get_wasm_func(&self, addr: FuncAddr) -> &WasmFunctionInstance {
        match self.funcs.get(addr as usize).map(|func| &func.kind) {
            Some(FunctionKind::Wasm(wasm_func)) => wasm_func,
            _ => unreachable!("invalid wasm function address: {addr}"),
        }
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem(&self, addr: MemAddr) -> &MemoryInstance {
        Self::get(&self.memories, addr, "memory")
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem_mut(&mut self, addr: MemAddr) -> &mut MemoryInstance {
        Self::get_mut(&mut self.memories, addr, "memory")
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mems_mut(&mut self, addr: MemAddr, addr2: MemAddr) -> (&mut MemoryInstance, &mut MemoryInstance) {
        Self::get_disjoint_mut(&mut self.memories, addr, addr2, "memory")
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table(&self, addr: TableAddr) -> &TableInstance {
        Self::get(&self.tables, addr, "table")
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table_mut(&mut self, addr: TableAddr) -> &mut TableInstance {
        Self::get_mut(&mut self.tables, addr, "table")
    }

    /// Get two mutable tables at the actual index in the store
    pub(crate) fn get_tables_mut(
        &mut self,
        addr: TableAddr,
        addr2: TableAddr,
    ) -> (&mut TableInstance, &mut TableInstance) {
        Self::get_disjoint_mut(&mut self.tables, addr, addr2, "table")
    }

    /// Get the data at the actual index in the store
    pub(crate) fn get_data_mut(&mut self, addr: DataAddr) -> &mut DataInstance {
        Self::get_mut(&mut self.data, addr, "data")
    }

    /// Get the element at the actual index in the store
    pub(crate) fn get_elem_mut(&mut self, addr: ElemAddr) -> &mut ElementInstance {
        Self::get_mut(&mut self.elements, addr, "element")
    }

    /// Get the global at the actual index in the store
    pub(crate) fn get_global(&self, addr: GlobalAddr) -> &GlobalInstance {
        Self::get(&self.globals, addr, "global")
    }

    /// Get the global at the actual index in the store
    pub(crate) fn get_global_mut(&mut self, addr: GlobalAddr) -> &mut GlobalInstance {
        Self::get_mut(&mut self.globals, addr, "global")
    }

    /// Get the global at the actual index in the store
    pub(crate) fn get_global_val(&self, addr: GlobalAddr) -> TinyWasmValue {
        self.get_global(addr).value
    }

    /// Set the global at the actual index in the store
    pub(crate) fn set_global_val(&mut self, addr: GlobalAddr, value: TinyWasmValue) {
        self.get_global_mut(addr).value = value;
    }
}
