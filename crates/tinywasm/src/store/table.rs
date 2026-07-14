use crate::{Result, Trap};
use alloc::{vec, vec::Vec};
use core::ops::Range;
use tinywasm_types::*;

const MAX_TABLE_SIZE: u32 = 10_000_000;

/// A WebAssembly Table Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#table-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct TableInstance {
    pub(crate) elements: Vec<TableElement>,
    pub(crate) kind: TableType,
}

impl TableInstance {
    pub(crate) fn new(kind: TableType) -> Self {
        Self::new_with_init(kind, TableElement::Uninitialized)
    }

    pub(crate) fn new_with_init(kind: TableType, init: TableElement) -> Self {
        Self { elements: vec![init; kind.size_initial as usize], kind }
    }

    #[inline(never)]
    #[cold]
    fn trap_oob(&self, addr: usize, len: usize) -> Trap {
        crate::Trap::TableOutOfBounds { offset: addr, len, max: self.elements.len() }
    }

    fn checked_range(&self, addr: usize, len: usize) -> Result<Range<usize>, Trap> {
        let end = addr.checked_add(len).ok_or_else(|| self.trap_oob(addr, len))?;
        if end > self.elements.len() {
            return Err(self.trap_oob(addr, len));
        }
        Ok(addr..end)
    }

    pub(crate) fn get_wasm_val(&self, addr: TableAddr) -> Result<WasmValue, Trap> {
        let val = self.get(addr)?.addr();

        Ok(match self.kind.element_type {
            WasmType::RefFunc => WasmValue::RefFunc(FuncRef::new(val)),
            WasmType::RefExtern => WasmValue::RefExtern(ExternRef::new(val)),
            _ => Err(Trap::Other("non-ref table"))?,
        })
    }

    pub(crate) fn fill(&mut self, func_addrs: &[u32], addr: usize, len: usize, val: TableElement) -> Result<(), Trap> {
        let val = val.map(|addr| self.resolve_func_ref(func_addrs, addr));
        let range = self.checked_range(addr, len)?;
        self.elements[range].fill(val);
        Ok(())
    }

    pub(crate) fn get(&self, addr: TableAddr) -> Result<&TableElement, Trap> {
        self.elements.get(addr as usize).ok_or(Trap::TableOutOfBounds {
            offset: addr as usize,
            len: 1,
            max: self.elements.len(),
        })
    }

    pub(crate) fn copy_from_slice(&mut self, dst: usize, src: &[TableElement]) -> Result<(), Trap> {
        let range = self.checked_range(dst, src.len())?;
        self.elements[range].copy_from_slice(src);
        Ok(())
    }

    pub(crate) fn load(&self, addr: usize, len: usize) -> Result<&[TableElement], Trap> {
        Ok(&self.elements[self.checked_range(addr, len)?])
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<(), Trap> {
        let src = self.checked_range(src, len)?;
        self.checked_range(dst, len)?;
        self.elements.copy_within(src, dst);
        Ok(())
    }

    pub(crate) fn set(&mut self, table_idx: TableAddr, value: TableElement) -> Result<(), Trap> {
        let range = self.checked_range(table_idx as usize, 1)?;
        self.elements[range.start] = value;
        Ok(())
    }

    pub(crate) fn grow(&mut self, n: i32, init: TableElement) -> Result<(), Trap> {
        if n < 0 {
            return Err(crate::Trap::TableOutOfBounds { offset: 0, len: 1, max: self.elements.len() });
        }

        let len = n as usize + self.elements.len();
        let max = self.kind.size_max.unwrap_or(MAX_TABLE_SIZE) as usize;
        if len > max {
            return Err(crate::Trap::TableOutOfBounds { offset: len, len: 1, max: self.elements.len() });
        }

        self.elements.resize(len, init);
        Ok(())
    }

    pub(crate) fn size(&self) -> i32 {
        self.elements.len() as i32
    }

    fn resolve_func_ref(&self, func_addrs: &[u32], addr: Addr) -> Addr {
        if self.kind.element_type != WasmType::RefFunc {
            return addr;
        }

        *func_addrs
            .get(addr as usize)
            .expect("error initializing table: function not found. This should have been caught by the validator")
    }

    pub(crate) fn init(&mut self, offset: i64, init: &[TableElement]) -> Result<(), Trap> {
        let offset = offset as usize;
        let range = self.checked_range(offset, init.len())?;
        self.elements[range].copy_from_slice(init);
        Ok(())
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) enum TableElement {
    Uninitialized,
    Initialized(TableAddr),
}

impl From<Option<Addr>> for TableElement {
    fn from(addr: Option<Addr>) -> Self {
        match addr {
            None => Self::Uninitialized,
            Some(addr) => Self::Initialized(addr),
        }
    }
}

impl TableElement {
    pub(crate) fn addr(&self) -> Option<Addr> {
        match self {
            Self::Uninitialized => None,
            Self::Initialized(addr) => Some(*addr),
        }
    }

    pub(crate) fn map(self, f: impl FnOnce(Addr) -> Addr) -> Self {
        match self {
            Self::Uninitialized => Self::Uninitialized,
            Self::Initialized(addr) => Self::Initialized(f(addr)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a dummy TableType
    fn dummy_table_type() -> TableType {
        TableType { element_type: WasmType::RefFunc, size_initial: 10, size_max: Some(20) }
    }

    #[test]
    fn test_table_instance_creation() {
        let kind = dummy_table_type();
        let table_instance = TableInstance::new(kind.clone());
        assert_eq!(table_instance.size(), kind.size_initial as i32, "Table instance creation failed: size mismatch");
    }

    #[test]
    fn test_get_wasm_val() {
        let kind = dummy_table_type();
        let mut table_instance = TableInstance::new(kind);

        table_instance.set(0, TableElement::Initialized(0)).expect("Setting table element failed");
        table_instance.set(1, TableElement::Uninitialized).expect("Setting table element failed");

        match table_instance.get_wasm_val(0) {
            Ok(WasmValue::RefFunc(_)) => {}
            _ => panic!("get_wasm_val failed to return the correct WasmValue"),
        }

        match table_instance.get_wasm_val(1) {
            Ok(WasmValue::RefFunc(f)) if f.is_null() => {}
            _ => panic!("get_wasm_val failed to return the correct WasmValue"),
        }

        match table_instance.get_wasm_val(999) {
            Err(Trap::TableOutOfBounds { .. }) => {}
            _ => panic!("get_wasm_val failed to handle undefined element correctly"),
        }
    }

    #[test]
    fn test_set_and_get() {
        let kind = dummy_table_type();
        let mut table_instance = TableInstance::new(kind);

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
        let mut table_instance = TableInstance::new(kind);

        let init_elements = vec![TableElement::Initialized(0); 5];
        let result = table_instance.init(0, &init_elements);

        assert!(result.is_ok(), "Initializing table with elements failed");

        for i in 0..5 {
            let elem = table_instance.get(i);
            assert!(
                elem.is_ok() && matches!(elem.unwrap(), &TableElement::Initialized(_)),
                "Element not initialized correctly at index {i}"
            );
        }
    }
}
