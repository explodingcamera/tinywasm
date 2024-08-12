use crate::log;
use crate::{Error, Result, Trap};
use alloc::{vec, vec::Vec};
use tinywasm_types::*;

const MAX_TABLE_SIZE: u32 = 10_000_000;

/// A WebAssembly Table Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#table-instances>
#[derive(Debug)]
pub(crate) struct TableInstance {
    pub(crate) elements: Vec<TableElement>,
    pub(crate) kind: TableType,
    pub(crate) _owner: ModuleInstanceAddr, // index into store.module_instances
}

impl TableInstance {
    pub(crate) fn new(kind: TableType, owner: ModuleInstanceAddr) -> Self {
        Self { elements: vec![TableElement::Uninitialized; kind.size_initial as usize], kind, _owner: owner }
    }

    #[inline(never)]
    #[cold]
    fn trap_oob(&self, addr: usize, len: usize) -> Error {
        Error::Trap(crate::Trap::TableOutOfBounds { offset: addr, len, max: self.elements.len() })
    }

    pub(crate) fn get_wasm_val(&self, addr: TableAddr) -> Result<WasmValue> {
        let val = self.get(addr)?.addr();

        Ok(match self.kind.element_type {
            ValType::RefFunc => val.map_or(WasmValue::RefNull(ValType::RefFunc), WasmValue::RefFunc),
            ValType::RefExtern => val.map_or(WasmValue::RefNull(ValType::RefExtern), WasmValue::RefExtern),
            _ => Err(Error::UnsupportedFeature("non-ref table".into()))?,
        })
    }

    pub(crate) fn fill(&mut self, func_addrs: &[u32], addr: usize, len: usize, val: TableElement) -> Result<()> {
        let val = val.map(|addr| self.resolve_func_ref(func_addrs, addr));
        let end = addr.checked_add(len).ok_or_else(|| self.trap_oob(addr, len))?;
        if end > self.elements.len() {
            return Err(self.trap_oob(addr, len));
        }

        self.elements[addr..end].fill(val);
        Ok(())
    }

    pub(crate) fn get(&self, addr: TableAddr) -> Result<&TableElement> {
        // self.elements.get(addr as usize).ok_or_else(|| Error::Trap(Trap::UndefinedElement { index: addr as usize }))
        self.elements.get(addr as usize).ok_or_else(|| {
            Error::Trap(Trap::TableOutOfBounds { offset: addr as usize, len: 1, max: self.elements.len() })
        })
    }

    pub(crate) fn copy_from_slice(&mut self, dst: usize, src: &[TableElement]) -> Result<()> {
        let end = dst.checked_add(src.len()).ok_or_else(|| self.trap_oob(dst, src.len()))?;

        if end > self.elements.len() {
            return Err(self.trap_oob(dst, src.len()));
        }

        self.elements[dst..end].copy_from_slice(src);
        Ok(())
    }

    pub(crate) fn load(&self, addr: usize, len: usize) -> Result<&[TableElement]> {
        let Some(end) = addr.checked_add(len) else {
            return Err(self.trap_oob(addr, len));
        };

        if end > self.elements.len() || end < addr {
            return Err(self.trap_oob(addr, len));
        }

        Ok(&self.elements[addr..end])
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<()> {
        // Calculate the end of the source slice
        let src_end = src.checked_add(len).ok_or_else(|| self.trap_oob(src, len))?;
        if src_end > self.elements.len() {
            return Err(self.trap_oob(src, len));
        }

        // Calculate the end of the destination slice
        let dst_end = dst.checked_add(len).ok_or_else(|| self.trap_oob(dst, len))?;
        if dst_end > self.elements.len() {
            return Err(self.trap_oob(dst, len));
        }

        // Perform the copy
        self.elements.copy_within(src..src_end, dst);
        Ok(())
    }

    pub(crate) fn set(&mut self, table_idx: TableAddr, value: TableElement) -> Result<()> {
        if table_idx as usize >= self.elements.len() {
            return Err(self.trap_oob(table_idx as usize, 1));
        }

        self.elements[table_idx as usize] = value;
        Ok(())
    }

    pub(crate) fn grow(&mut self, n: i32, init: TableElement) -> Result<()> {
        if n < 0 {
            return Err(Error::Trap(crate::Trap::TableOutOfBounds { offset: 0, len: 1, max: self.elements.len() }));
        }

        let len = n as usize + self.elements.len();
        let max = self.kind.size_max.unwrap_or(MAX_TABLE_SIZE) as usize;
        if len > max {
            return Err(Error::Trap(crate::Trap::TableOutOfBounds { offset: len, len: 1, max: self.elements.len() }));
        }

        self.elements.resize(len, init);
        Ok(())
    }

    pub(crate) fn size(&self) -> i32 {
        self.elements.len() as i32
    }

    fn resolve_func_ref(&self, func_addrs: &[u32], addr: Addr) -> Addr {
        if self.kind.element_type != ValType::RefFunc {
            return addr;
        }

        *func_addrs
            .get(addr as usize)
            .expect("error initializing table: function not found. This should have been caught by the validator")
    }

    // Initialize the table with the given elements
    pub(crate) fn init_raw(&mut self, offset: i32, init: &[TableElement]) -> Result<()> {
        let offset = offset as usize;
        let end = offset.checked_add(init.len()).ok_or_else(|| {
            Error::Trap(crate::Trap::TableOutOfBounds { offset, len: init.len(), max: self.elements.len() })
        })?;

        if end > self.elements.len() || end < offset {
            return Err(crate::Trap::TableOutOfBounds { offset, len: init.len(), max: self.elements.len() }.into());
        }

        self.elements[offset..end].copy_from_slice(init);
        log::debug!("table: {:?}", self.elements);
        Ok(())
    }

    // Initialize the table with the given elements (resolves function references)
    pub(crate) fn init(&mut self, func_addrs: &[u32], offset: i32, init: &[TableElement]) -> Result<()> {
        let init = init.iter().map(|item| item.map(|addr| self.resolve_func_ref(func_addrs, addr))).collect::<Vec<_>>();
        self.init_raw(offset, &init)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TableElement {
    Uninitialized,
    Initialized(TableAddr),
}

impl From<Option<Addr>> for TableElement {
    fn from(addr: Option<Addr>) -> Self {
        match addr {
            None => TableElement::Uninitialized,
            Some(addr) => TableElement::Initialized(addr),
        }
    }
}

impl TableElement {
    pub(crate) fn addr(&self) -> Option<Addr> {
        match self {
            TableElement::Uninitialized => None,
            TableElement::Initialized(addr) => Some(*addr),
        }
    }

    pub(crate) fn map<F: FnOnce(Addr) -> Addr>(self, f: F) -> Self {
        match self {
            TableElement::Uninitialized => TableElement::Uninitialized,
            TableElement::Initialized(addr) => TableElement::Initialized(f(addr)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a dummy TableType
    fn dummy_table_type() -> TableType {
        TableType { element_type: ValType::RefFunc, size_initial: 10, size_max: Some(20) }
    }

    #[test]
    fn test_table_instance_creation() {
        let kind = dummy_table_type();
        let table_instance = TableInstance::new(kind.clone(), 0);
        assert_eq!(table_instance.size(), kind.size_initial as i32, "Table instance creation failed: size mismatch");
    }

    #[test]
    fn test_get_wasm_val() {
        let kind = dummy_table_type();
        let mut table_instance = TableInstance::new(kind, 0);

        table_instance.set(0, TableElement::Initialized(0)).expect("Setting table element failed");
        table_instance.set(1, TableElement::Uninitialized).expect("Setting table element failed");

        match table_instance.get_wasm_val(0) {
            Ok(WasmValue::RefFunc(_)) => {}
            _ => panic!("get_wasm_val failed to return the correct WasmValue"),
        }

        match table_instance.get_wasm_val(1) {
            Ok(WasmValue::RefNull(ValType::RefFunc)) => {}
            _ => panic!("get_wasm_val failed to return the correct WasmValue"),
        }

        match table_instance.get_wasm_val(999) {
            Err(Error::Trap(Trap::TableOutOfBounds { .. })) => {}
            _ => panic!("get_wasm_val failed to handle undefined element correctly"),
        }
    }

    #[test]
    fn test_set_and_get() {
        let kind = dummy_table_type();
        let mut table_instance = TableInstance::new(kind, 0);

        let result = table_instance.set(0, TableElement::Initialized(1));
        assert!(result.is_ok(), "Setting table element failed");

        let elem = table_instance.get(0);
        assert!(
            elem.is_ok() && matches!(elem.unwrap(), &TableElement::Initialized(1)),
            "Getting table element failed or returned incorrect value"
        );
    }

    #[test]
    fn test_table_init() {
        let kind = dummy_table_type();
        let mut table_instance = TableInstance::new(kind, 0);

        let init_elements = vec![TableElement::Initialized(0); 5];
        let func_addrs = vec![0, 1, 2, 3, 4];
        let result = table_instance.init(&func_addrs, 0, &init_elements);

        assert!(result.is_ok(), "Initializing table with elements failed");

        for i in 0..5 {
            let elem = table_instance.get(i);
            assert!(
                elem.is_ok() && matches!(elem.unwrap(), &TableElement::Initialized(_)),
                "Element not initialized correctly at index {}",
                i
            );
        }
    }
}
