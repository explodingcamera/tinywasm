use crate::{Result, Trap, interpreter::ValueRef};
use alloc::vec::Vec;
use core::ops::Range;
use tinywasm_types::*;

const MAX_TABLE_SIZE: usize = 10_000_000;

/// A WebAssembly Table Instance
///
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#table-instances>
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct TableInstance {
    pub(crate) elements: Vec<ValueRef>,
    pub(crate) kind: TableType,
}

impl TableInstance {
    #[cfg(test)]
    pub(crate) fn new(kind: TableType) -> Result<Self> {
        Self::new_with_init(kind, ValueRef::NULL)
    }

    pub(crate) fn new_with_init(kind: TableType, init: ValueRef) -> Result<Self> {
        let size = usize::try_from(kind.size_initial).map_err(|_| Trap::OutOfMemory)?;
        if size > MAX_TABLE_SIZE {
            return Err(Trap::OutOfMemory.into());
        }
        let mut elements = Vec::new();
        elements.try_reserve_exact(size).map_err(|_| Trap::OutOfMemory)?;
        elements.resize(size, init);
        Ok(Self { elements, kind })
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

    pub(crate) fn fill(&mut self, addr: usize, len: usize, val: ValueRef) -> Result<(), Trap> {
        let range = self.checked_range(addr, len)?;
        self.elements[range].fill(val);
        Ok(())
    }

    pub(crate) fn get(&self, addr: usize) -> Result<&ValueRef, Trap> {
        self.elements.get(addr).ok_or_else(|| self.trap_oob(addr, 1))
    }

    pub(crate) fn copy_from_slice(&mut self, dst: usize, src: &[ValueRef]) -> Result<(), Trap> {
        let range = self.checked_range(dst, src.len())?;
        self.elements[range].copy_from_slice(src);
        Ok(())
    }

    pub(crate) fn load(&self, addr: usize, len: usize) -> Result<&[ValueRef], Trap> {
        Ok(&self.elements[self.checked_range(addr, len)?])
    }

    pub(crate) fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> Result<(), Trap> {
        let src = self.checked_range(src, len)?;
        self.checked_range(dst, len)?;
        self.elements.copy_within(src, dst);
        Ok(())
    }

    pub(crate) fn set(&mut self, table_idx: usize, value: ValueRef) -> Result<(), Trap> {
        let range = self.checked_range(table_idx, 1)?;
        self.elements[range.start] = value;
        Ok(())
    }

    pub(crate) fn grow(&mut self, n: usize, init: ValueRef) -> Result<(), Trap> {
        let len = n.checked_add(self.elements.len()).ok_or(Trap::OutOfMemory)?;
        let declared_max = self.kind.size_max.and_then(|max| usize::try_from(max).ok()).unwrap_or(usize::MAX);
        let max = declared_max.min(MAX_TABLE_SIZE);
        if len > max {
            return Err(crate::Trap::TableOutOfBounds { offset: len, len: 1, max: self.elements.len() });
        }

        self.elements.try_reserve_exact(n).map_err(|_| Trap::OutOfMemory)?;
        self.elements.resize(len, init);
        Ok(())
    }

    pub(crate) fn size(&self) -> usize {
        self.elements.len()
    }

    pub(crate) fn init(&mut self, offset: usize, init: &[ValueRef]) -> Result<(), Trap> {
        let range = self.checked_range(offset, init.len())?;
        self.elements[range].copy_from_slice(init);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // Helper to create a dummy TableType
    fn dummy_table_type() -> TableType {
        TableType::new(RefType::FUNCREF, 10, Some(20))
    }

    #[test]
    fn test_table_instance_creation() {
        let kind = dummy_table_type();
        let table_instance = TableInstance::new(kind).unwrap();
        assert_eq!(table_instance.size() as u64, kind.size_initial, "Table instance creation failed: size mismatch");
    }

    #[test]
    fn test_set_and_get() {
        let kind = dummy_table_type();
        let mut table_instance = TableInstance::new(kind).unwrap();

        let value = ValueRef::from_raw(2);
        let result = table_instance.set(0, value);
        assert!(result.is_ok(), "Setting table element failed");

        let elem = table_instance.get(0);
        assert!(elem.is_ok() && elem.unwrap() == &value, "Getting table element failed or returned incorrect value");
    }

    #[test]
    fn test_table_init() {
        let kind = dummy_table_type();
        let mut table_instance = TableInstance::new(kind).unwrap();

        let init_elements = vec![ValueRef::from_raw(2); 5];
        let result = table_instance.init(0, &init_elements);

        assert!(result.is_ok(), "Initializing table with elements failed");

        for i in 0..5 {
            let elem = table_instance.get(i);
            assert!(elem.is_ok() && !elem.unwrap().is_null(), "Element not initialized correctly at index {i}");
        }
    }
}
