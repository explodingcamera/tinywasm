use crate::{log, unlikely};
use crate::{Error, Result, Trap};
use alloc::{vec, vec::Vec};
use tinywasm_types::*;

const MAX_TABLE_SIZE: u32 = 10000000;

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

    pub(crate) fn get_wasm_val(&self, addr: usize) -> Result<WasmValue> {
        let val = self.get(addr)?.addr();

        Ok(match self.kind.element_type {
            ValType::RefFunc => val.map(WasmValue::RefFunc).unwrap_or(WasmValue::RefNull(ValType::RefFunc)),
            ValType::RefExtern => val.map(WasmValue::RefExtern).unwrap_or(WasmValue::RefNull(ValType::RefExtern)),
            _ => Err(Error::UnsupportedFeature("non-ref table".into()))?,
        })
    }

    pub(crate) fn get(&self, addr: usize) -> Result<&TableElement> {
        self.elements.get(addr).ok_or_else(|| Error::Trap(Trap::UndefinedElement { index: addr }))
    }

    pub(crate) fn set(&mut self, table_idx: usize, value: Addr) -> Result<()> {
        self.grow_to_fit(table_idx + 1).map(|_| self.elements[table_idx] = TableElement::Initialized(value))
    }

    pub(crate) fn grow_to_fit(&mut self, new_size: usize) -> Result<()> {
        if new_size > self.elements.len() {
            if unlikely(new_size > self.kind.size_max.unwrap_or(MAX_TABLE_SIZE) as usize) {
                return Err(crate::Trap::TableOutOfBounds { offset: new_size, len: 1, max: self.elements.len() }.into());
            }

            self.elements.resize(new_size, TableElement::Uninitialized);
        }
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
