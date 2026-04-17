use core::ffi::CStr;
use core::hint::cold_path;

use alloc::string::{String, ToString};
use alloc::{ffi::CString, format};

use crate::store::{GlobalInstance, TableElement, TableInstance};
use crate::{Error, MemoryInstance, Result, Store};
use tinywasm_types::{
    Addr, ExternRef, FuncRef, GlobalAddr, GlobalType, MemAddr, MemoryArch, MemoryType, TableAddr, TableType, WasmType,
    WasmValue,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct StoreItem {
    pub(crate) store_id: usize,
    pub(crate) addr: Addr,
}

impl StoreItem {
    #[inline]
    pub(crate) const fn new(store_id: usize, addr: Addr) -> Self {
        Self { store_id, addr }
    }

    #[inline]
    pub(crate) fn validate_store(&self, store: &Store) -> Result<()> {
        if self.store_id != store.id() {
            return Err(Error::InvalidStore);
        }
        Ok(())
    }
}

/// A memory instance in a store.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Memory(pub(crate) StoreItem);

/// A table instance in a store.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Table(pub(crate) StoreItem);

/// A global instance in a store.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Global(pub(crate) StoreItem);

impl Memory {
    #[inline]
    pub(crate) const fn from_store_addr(store_id: usize, addr: MemAddr) -> Self {
        Self(StoreItem::new(store_id, addr))
    }

    /// Create a new memory in the given store.
    pub fn new(store: &mut Store, ty: MemoryType) -> Result<Self> {
        if let MemoryArch::I64 = ty.arch() {
            return Err(Error::UnsupportedFeature("64-bit memories".to_string()));
        }
        let addr = store.state.memories.len() as MemAddr;
        store.state.memories.push(MemoryInstance::new(ty));
        Ok(Self::from_store_addr(store.id(), addr))
    }

    #[inline]
    fn instance<'a>(&self, store: &'a Store) -> Result<&'a MemoryInstance> {
        self.0.validate_store(store)?;
        Ok(store.state.get_mem(self.0.addr))
    }

    #[inline]
    fn instance_mut<'a>(&self, store: &'a mut Store) -> Result<&'a mut MemoryInstance> {
        self.0.validate_store(store)?;
        Ok(store.state.get_mem_mut(self.0.addr))
    }

    /// Returns the full raw memory data.
    pub fn data<'a>(&self, store: &'a Store) -> Result<&'a [u8]> {
        Ok(&self.instance(store)?.data)
    }

    /// Returns the full raw mutable memory data.
    pub fn data_mut<'a>(&self, store: &'a mut Store) -> Result<&'a mut [u8]> {
        Ok(&mut self.instance_mut(store)?.data)
    }

    /// Returns the raw memory byte length.
    pub fn data_size(&self, store: &Store) -> Result<usize> {
        Ok(self.instance(store)?.data.len())
    }

    /// Load a slice of memory.
    pub fn load<'a>(&self, store: &'a Store, offset: usize, len: usize) -> Result<&'a [u8]> {
        self.instance(store)?.load(offset, len)
    }

    /// Grow the memory by the given number of pages.
    pub fn grow(&self, store: &mut Store, delta_pages: i64) -> Result<Option<i64>> {
        Ok(self.instance_mut(store)?.grow(delta_pages))
    }

    /// Get the current size of the memory in pages.
    pub fn page_count(&self, store: &Store) -> Result<usize> {
        Ok(self.instance(store)?.page_count)
    }

    /// Copy a slice of memory to another place in memory.
    pub fn copy_within(&self, store: &mut Store, src: usize, dst: usize, len: usize) -> Result<()> {
        self.instance_mut(store)?.copy_within(dst, src, len)
    }

    /// Fill a slice of memory with a value.
    pub fn fill(&self, store: &mut Store, offset: usize, len: usize, val: u8) -> Result<()> {
        self.instance_mut(store)?.fill(offset, len, val)
    }

    /// Store a slice of memory.
    pub fn store(&self, store: &mut Store, offset: usize, data: &[u8]) -> Result<()> {
        self.instance_mut(store)?.store(offset, data)
    }

    /// Load a C-style string from memory.
    pub fn load_cstr<'a>(&self, store: &'a Store, offset: usize, len: usize) -> Result<&'a CStr> {
        CStr::from_bytes_with_nul(self.load(store, offset, len)?)
            .map_err(|e| crate::Error::Other(format!("Invalid C-style string: {e}")))
    }

    /// Load a C-style string from memory, stopping at the first nul byte.
    pub fn load_cstr_until_nul<'a>(&self, store: &'a Store, offset: usize, max_len: usize) -> Result<&'a CStr> {
        CStr::from_bytes_until_nul(self.load(store, offset, max_len)?)
            .map_err(|e| crate::Error::Other(format!("Invalid C-style string: {e}")))
    }

    /// Load a UTF-8 string from memory.
    pub fn load_string(&self, store: &Store, offset: usize, len: usize) -> Result<String> {
        String::from_utf8(self.load(store, offset, len)?.to_vec())
            .map_err(|e| crate::Error::Other(format!("Invalid UTF-8 string: {e}")))
    }

    /// Load a C-style string from memory.
    pub fn load_cstring(&self, store: &Store, offset: usize, len: usize) -> Result<CString> {
        Ok(CString::from(self.load_cstr(store, offset, len)?))
    }

    /// Load a C-style string from memory, stopping at the first nul byte.
    pub fn load_cstring_until_nul(&self, store: &Store, offset: usize, max_len: usize) -> Result<CString> {
        Ok(CString::from(self.load_cstr_until_nul(store, offset, max_len)?))
    }

    /// Load a JavaScript-style utf-16 string from memory.
    pub fn load_js_string(&self, store: &Store, offset: usize, len: usize) -> Result<String> {
        let bytes = self.load(store, offset, len)?;
        let mut string = String::new();
        for i in 0..(len / 2) {
            let c = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
            string.push(
                char::from_u32(u32::from(c)).ok_or_else(|| crate::Error::Other("Invalid UTF-16 string".to_string()))?,
            );
        }
        Ok(string)
    }
}

fn table_element_to_value(element_type: WasmType, element: TableElement) -> WasmValue {
    match element_type {
        WasmType::RefFunc => WasmValue::RefFunc(FuncRef::new(element.addr())),
        WasmType::RefExtern => WasmValue::RefExtern(ExternRef::new(element.addr())),
        _ => unreachable!("table element type must be a reference type"),
    }
}

fn table_value_to_element(element_type: WasmType, value: WasmValue) -> Result<TableElement> {
    match (element_type, value) {
        (WasmType::RefFunc, WasmValue::RefFunc(func_ref)) => Ok(TableElement::from(func_ref.addr())),
        (WasmType::RefExtern, WasmValue::RefExtern(extern_ref)) => Ok(TableElement::from(extern_ref.addr())),
        _ => Err(Error::Other("invalid table value type".to_string())),
    }
}

impl Table {
    #[inline]
    pub(crate) const fn from_store_addr(store_id: usize, addr: TableAddr) -> Self {
        Self(StoreItem::new(store_id, addr))
    }

    /// Create a new table in the given store.
    pub fn new(store: &mut Store, ty: TableType, init: WasmValue) -> Result<Self> {
        let init = match (ty.element_type, init) {
            (WasmType::RefFunc, WasmValue::RefFunc(func_ref)) => TableElement::from(func_ref.addr()),
            (WasmType::RefExtern, WasmValue::RefExtern(extern_ref)) => TableElement::from(extern_ref.addr()),
            _ => return Err(Error::Other("invalid table init value".to_string())),
        };
        let addr = store.state.tables.len() as TableAddr;
        store.state.tables.push(TableInstance::new_with_init(ty, init));
        Ok(Self::from_store_addr(store.id(), addr))
    }

    #[inline]
    fn instance<'a>(&self, store: &'a Store) -> Result<&'a TableInstance> {
        self.0.validate_store(store)?;
        Ok(store.state.get_table(self.0.addr))
    }

    #[inline]
    fn instance_mut<'a>(&self, store: &'a mut Store) -> Result<&'a mut TableInstance> {
        self.0.validate_store(store)?;
        Ok(store.state.get_table_mut(self.0.addr))
    }

    /// Get the type of the table.
    pub fn ty(&self, store: &Store) -> Result<TableType> {
        Ok(self.instance(store)?.kind.clone())
    }

    /// Get the current number of elements in the table.
    pub fn size(&self, store: &Store) -> Result<usize> {
        Ok(self.instance(store)?.size() as usize)
    }

    /// Get a table element as a wasm reference value.
    pub fn get(&self, store: &Store, index: TableAddr) -> Result<WasmValue> {
        self.instance(store)?.get_wasm_val(index)
    }

    /// Load a range of table elements and iterate over wasm reference values.
    pub fn load(&self, store: &Store, offset: usize, len: usize) -> Result<alloc::vec::IntoIter<WasmValue>> {
        let table = self.instance(store)?;
        let element_type = table.kind.element_type;
        let elements = table.load(offset, len)?;
        Ok(elements
            .iter()
            .copied()
            .map(move |element| table_element_to_value(element_type, element))
            .collect::<alloc::vec::Vec<_>>()
            .into_iter())
    }

    /// Set a table element.
    pub fn set(&self, store: &mut Store, index: TableAddr, value: WasmValue) -> Result<()> {
        let table = self.instance_mut(store)?;
        let value = table_value_to_element(table.kind.element_type, value)?;
        table.set(index, value)
    }

    /// Copy elements within the same table.
    pub fn copy_within(&self, store: &mut Store, src: usize, dst: usize, len: usize) -> Result<()> {
        self.instance_mut(store)?.copy_within(dst, src, len)
    }

    /// Grow the table and return the previous size.
    pub fn grow(&self, store: &mut Store, delta: i32, init: WasmValue) -> Result<usize> {
        let table = self.instance_mut(store)?;
        let old_size = table.size() as usize;
        let init = table_value_to_element(table.kind.element_type, init)?;
        table.grow(delta, init)?;
        Ok(old_size)
    }
}

impl Global {
    #[inline]
    pub(crate) const fn from_store_addr(store_id: usize, addr: GlobalAddr) -> Self {
        Self(StoreItem::new(store_id, addr))
    }

    /// Create a new global in the given store.
    pub fn new(store: &mut Store, ty: GlobalType, value: WasmValue) -> Result<Self> {
        let addr = store.state.globals.len() as GlobalAddr;
        store.state.globals.push(GlobalInstance::new(ty, value.into()));
        Ok(Self::from_store_addr(store.id(), addr))
    }

    #[inline]
    fn instance<'a>(&self, store: &'a Store) -> Result<&'a GlobalInstance> {
        self.0.validate_store(store)?;
        Ok(store.state.get_global(self.0.addr))
    }

    #[inline]
    fn instance_mut<'a>(&self, store: &'a mut Store) -> Result<&'a mut GlobalInstance> {
        self.0.validate_store(store)?;
        Ok(store.state.get_global_mut(self.0.addr))
    }

    /// Get the type of the global.
    pub fn ty(&self, store: &Store) -> Result<GlobalType> {
        Ok(self.instance(store)?.ty)
    }

    /// Get the current value of the global.
    pub fn get(&self, store: &Store) -> Result<WasmValue> {
        let global = self.instance(store)?;
        let value = global.value.get().attach_type(global.ty.ty);
        Ok(value.unwrap_or_else(|| unreachable!("Global value type does not match global type, this is a bug")))
    }

    /// Set the current value of the global.
    pub fn set(&self, store: &mut Store, value: WasmValue) -> Result<()> {
        let global = self.instance_mut(store)?;
        if !global.ty.mutable {
            cold_path();
            return Err(Error::Other("global is immutable".to_string()));
        }
        if WasmType::from(value) != global.ty.ty {
            cold_path();
            return Err(Error::Other("invalid global value type".to_string()));
        }
        global.value.set(value.into());
        Ok(())
    }
}
