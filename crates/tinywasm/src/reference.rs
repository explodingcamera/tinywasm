use core::ffi::CStr;

use alloc::string::{String, ToString};
use alloc::{ffi::CString, format};

use crate::store::{GlobalInstance, TableElement, TableInstance};
use crate::{Error, MemoryInstance, Result};
use tinywasm_types::{ExternRef, FuncRef, GlobalType, TableAddr, TableType, ValType, WasmValue};

// This module essentially contains the public APIs to interact with the data stored in the store

/// A reference to a memory instance
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct MemoryRef<'a>(pub(crate) &'a MemoryInstance);

/// A mutable reference to a memory instance.
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct MemoryRefMut<'a>(pub(crate) &'a mut MemoryInstance);

/// A reference to a table instance.
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct TableRef<'a>(pub(crate) &'a TableInstance);

/// A mutable reference to a table instance.
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct TableRefMut<'a>(pub(crate) &'a mut TableInstance);

/// A reference to a global instance.
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct GlobalRef<'a>(pub(crate) &'a GlobalInstance);

/// A mutable reference to a global instance.
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct GlobalRefMut<'a>(pub(crate) &'a mut GlobalInstance);

fn table_element_to_value(element_type: ValType, element: TableElement) -> WasmValue {
    match element_type {
        ValType::RefFunc => WasmValue::RefFunc(FuncRef::new(element.addr())),
        ValType::RefExtern => WasmValue::RefExtern(ExternRef::new(element.addr())),
        _ => unreachable!("table element type must be a reference type"),
    }
}

fn table_value_to_element(element_type: ValType, value: WasmValue) -> Result<TableElement> {
    match (element_type, value) {
        (ValType::RefFunc, WasmValue::RefFunc(func_ref)) => Ok(TableElement::from(func_ref.addr())),
        (ValType::RefExtern, WasmValue::RefExtern(extern_ref)) => Ok(TableElement::from(extern_ref.addr())),
        _ => Err(Error::Other("invalid table value type".to_string())),
    }
}

impl MemoryRefLoad for MemoryRef<'_> {
    /// Load a slice of memory
    fn load(&self, offset: usize, len: usize) -> Result<&[u8]> {
        self.0.load(offset, len)
    }
}

impl MemoryRefLoad for MemoryRefMut<'_> {
    /// Load a slice of memory
    fn load(&self, offset: usize, len: usize) -> Result<&[u8]> {
        self.0.load(offset, len)
    }
}

impl MemoryRef<'_> {
    /// Returns the full raw memory data.
    pub fn data(&self) -> &[u8] {
        &self.0.data
    }

    /// Returns the raw memory byte length.
    pub fn data_size(&self) -> usize {
        self.0.data.len()
    }

    /// Load a slice of memory
    pub fn load(&self, offset: usize, len: usize) -> Result<&[u8]> {
        self.0.load(offset, len)
    }
}

impl MemoryRefMut<'_> {
    /// Returns the full raw memory data.
    pub fn data(&self) -> &[u8] {
        &self.0.data
    }

    /// Returns the full raw mutable memory data.
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.0.data
    }

    /// Returns the raw memory byte length.
    pub fn data_size(&self) -> usize {
        self.0.data.len()
    }

    /// Load a slice of memory
    pub fn load(&self, offset: usize, len: usize) -> Result<&[u8]> {
        self.0.load(offset, len)
    }

    /// Grow the memory by the given number of pages
    pub fn grow(&mut self, delta_pages: i64) -> Option<i64> {
        self.0.grow(delta_pages)
    }

    /// Get the current size of the memory in pages
    pub fn page_count(&mut self) -> usize {
        self.0.page_count
    }

    /// Copy a slice of memory to another place in memory
    pub fn copy_within(&mut self, src: usize, dst: usize, len: usize) -> Result<()> {
        self.0.copy_within(dst, src, len)
    }

    /// Fill a slice of memory with a value
    pub fn fill(&mut self, offset: usize, len: usize, val: u8) -> Result<()> {
        self.0.fill(offset, len, val)
    }

    /// Store a slice of memory
    pub fn store(&mut self, offset: usize, len: usize, data: &[u8]) -> Result<()> {
        self.0.store(offset, len, data)
    }
}

impl TableRef<'_> {
    /// Get the type of the table.
    pub fn ty(&self) -> TableType {
        self.0.kind.clone()
    }

    /// Get the current number of elements in the table.
    pub fn size(&self) -> usize {
        self.0.size() as usize
    }

    /// Get a table element as a wasm reference value.
    pub fn get(&self, index: TableAddr) -> Result<WasmValue> {
        self.0.get_wasm_val(index)
    }

    /// Load a range of table elements and iterate over wasm reference values.
    pub fn load(&self, offset: usize, len: usize) -> Result<impl Iterator<Item = WasmValue> + '_> {
        let element_type = self.0.kind.element_type;
        let elements = self.0.load(offset, len)?;
        Ok(elements.iter().copied().map(move |element| table_element_to_value(element_type, element)))
    }
}

impl TableRefMut<'_> {
    /// Get the type of the table.
    pub fn ty(&self) -> TableType {
        self.0.kind.clone()
    }

    /// Get the current number of elements in the table.
    pub fn size(&self) -> usize {
        self.0.size() as usize
    }

    /// Get a table element as a wasm reference value.
    pub fn get(&self, index: TableAddr) -> Result<WasmValue> {
        self.0.get_wasm_val(index)
    }

    /// Load a range of table elements and iterate over wasm reference values.
    pub fn load(&self, offset: usize, len: usize) -> Result<impl Iterator<Item = WasmValue> + '_> {
        let element_type = self.0.kind.element_type;
        let elements = self.0.load(offset, len)?;
        Ok(elements.iter().copied().map(move |element| table_element_to_value(element_type, element)))
    }

    /// Set a table element.
    pub fn set(&mut self, index: TableAddr, value: WasmValue) -> Result<()> {
        let value = table_value_to_element(self.0.kind.element_type, value)?;
        self.0.set(index, value)
    }

    /// Copy elements within the same table.
    pub fn copy_within(&mut self, src: usize, dst: usize, len: usize) -> Result<()> {
        self.0.copy_within(dst, src, len)
    }

    /// Grow the table and return the previous size.
    pub fn grow(&mut self, delta: i32, init: WasmValue) -> Result<usize> {
        let old_size = self.size();
        let init = table_value_to_element(self.0.kind.element_type, init)?;
        self.0.grow(delta, init)?;
        Ok(old_size)
    }
}

impl GlobalRef<'_> {
    /// Get the type of the global.
    pub fn ty(&self) -> GlobalType {
        self.0.ty
    }

    /// Get the current value of the global.
    pub fn get(&self) -> WasmValue {
        self.0.value.get().attach_type(self.0.ty.ty)
    }
}

impl GlobalRefMut<'_> {
    /// Get the type of the global.
    pub fn ty(&self) -> GlobalType {
        self.0.ty
    }

    /// Get the current value of the global.
    pub fn get(&self) -> WasmValue {
        self.0.value.get().attach_type(self.0.ty.ty)
    }

    /// Set the current value of the global.
    pub fn set(&mut self, value: WasmValue) -> Result<()> {
        if !self.0.ty.mutable {
            return Err(Error::Other("global is immutable".to_string()));
        }
        if value.val_type() != self.0.ty.ty {
            return Err(Error::Other("invalid global value type".to_string()));
        }
        self.0.value.set(value.into());
        Ok(())
    }
}

#[doc(hidden)]
pub trait MemoryRefLoad {
    fn load(&self, offset: usize, len: usize) -> Result<&[u8]>;
}

/// Convenience methods for loading strings from memory
pub trait MemoryStringExt: MemoryRefLoad {
    /// Load a C-style string from memory
    fn load_cstr(&self, offset: usize, len: usize) -> Result<&CStr> {
        let bytes = self.load(offset, len)?;
        CStr::from_bytes_with_nul(bytes).map_err(|e| crate::Error::Other(format!("Invalid C-style string: {e}")))
    }

    /// Load a C-style string from memory, stopping at the first nul byte
    fn load_cstr_until_nul(&self, offset: usize, max_len: usize) -> Result<&CStr> {
        let bytes = self.load(offset, max_len)?;
        CStr::from_bytes_until_nul(bytes).map_err(|e| crate::Error::Other(format!("Invalid C-style string: {e}")))
    }

    /// Load a UTF-8 string from memory
    fn load_string(&self, offset: usize, len: usize) -> Result<String> {
        let bytes = self.load(offset, len)?;
        String::from_utf8(bytes.to_vec()).map_err(|e| crate::Error::Other(format!("Invalid UTF-8 string: {e}")))
    }

    /// Load a C-style string from memory
    fn load_cstring(&self, offset: usize, len: usize) -> Result<CString> {
        Ok(CString::from(self.load_cstr(offset, len)?))
    }

    /// Load a C-style string from memory, stopping at the first nul byte
    fn load_cstring_until_nul(&self, offset: usize, max_len: usize) -> Result<CString> {
        Ok(CString::from(self.load_cstr_until_nul(offset, max_len)?))
    }

    /// Load a JavaScript-style utf-16 string from memory
    fn load_js_string(&self, offset: usize, len: usize) -> Result<String> {
        let bytes = self.load(offset, len)?;
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

impl MemoryStringExt for MemoryRef<'_> {}
impl MemoryStringExt for MemoryRefMut<'_> {}
