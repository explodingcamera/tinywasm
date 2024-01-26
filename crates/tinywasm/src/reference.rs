use core::{
    cell::{Ref, RefCell},
    ffi::CStr,
};

use crate::{GlobalInstance, MemoryInstance, Result};
use alloc::{
    ffi::CString,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use tinywasm_types::WasmValue;

// This module essentially contains the public APIs to interact with the data stored in the store

/// A reference to a memory instance
#[derive(Debug, Clone)]
pub struct MemoryRef {
    pub(crate) instance: Rc<RefCell<MemoryInstance>>,
}

/// A borrowed reference to a memory instance
#[derive(Debug)]
pub struct BorrowedMemory<'a> {
    pub(crate) instance: Ref<'a, MemoryInstance>,
}

impl<'a> BorrowedMemory<'a> {
    /// Load a slice of memory
    pub fn load(&self, offset: usize, len: usize) -> Result<&[u8]> {
        self.instance.load(offset, 0, len)
    }

    /// Load a C-style string from memory
    pub fn load_cstr(&self, offset: usize, len: usize) -> Result<&CStr> {
        let bytes = self.load(offset, len)?;
        CStr::from_bytes_with_nul(bytes).map_err(|_| crate::Error::Other("Invalid C-style string".to_string()))
    }

    /// Load a C-style string from memory, stopping at the first nul byte
    pub fn load_cstr_until_nul(&self, offset: usize, max_len: usize) -> Result<&CStr> {
        let bytes = self.load(offset, max_len)?;
        CStr::from_bytes_until_nul(bytes).map_err(|_| crate::Error::Other("Invalid C-style string".to_string()))
    }
}

impl MemoryRef {
    /// Borrow the memory instance
    ///
    /// This is useful for when you want to load only a reference to a slice of memory
    /// without copying the data. The borrow should be dropped before any other memory
    /// operations are performed.
    pub fn borrow(&self) -> BorrowedMemory<'_> {
        BorrowedMemory { instance: self.instance.borrow() }
    }

    /// Load a slice of memory
    pub fn load_vec(&self, offset: usize, len: usize) -> Result<Vec<u8>> {
        self.instance.borrow().load(offset, 0, len).map(|x| x.to_vec())
    }

    /// Grow the memory by the given number of pages
    pub fn grow(&self, delta_pages: i32) -> Option<i32> {
        self.instance.borrow_mut().grow(delta_pages)
    }

    /// Get the current size of the memory in pages
    pub fn page_count(&self) -> usize {
        self.instance.borrow().page_count()
    }

    /// Copy a slice of memory to another place in memory
    pub fn copy_within(&self, src: usize, dst: usize, len: usize) -> Result<()> {
        self.instance.borrow_mut().copy_within(src, dst, len)
    }

    /// Fill a slice of memory with a value
    pub fn fill(&self, offset: usize, len: usize, val: u8) -> Result<()> {
        self.instance.borrow_mut().fill(offset, len, val)
    }

    /// Load a UTF-8 string from memory
    pub fn load_string(&self, offset: usize, len: usize) -> Result<String> {
        let bytes = self.load_vec(offset, len)?;
        Ok(String::from_utf8(bytes).map_err(|_| crate::Error::Other("Invalid UTF-8 string".to_string()))?)
    }

    /// Load a C-style string from memory
    pub fn load_cstring(&self, offset: usize, len: usize) -> Result<CString> {
        Ok(CString::from(self.borrow().load_cstr(offset, len)?))
    }

    /// Load a C-style string from memory, stopping at the first nul byte
    pub fn load_cstring_until_nul(&self, offset: usize, max_len: usize) -> Result<CString> {
        Ok(CString::from(self.borrow().load_cstr_until_nul(offset, max_len)?))
    }

    /// Load a JavaScript-style utf-16 string from memory
    pub fn load_js_string(&self, offset: usize, len: usize) -> Result<String> {
        let memref = self.borrow();
        let bytes = memref.load(offset, len)?;
        let mut string = String::new();
        for i in 0..(len / 2) {
            let c = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]);
            string.push(
                char::from_u32(c as u32).ok_or_else(|| crate::Error::Other("Invalid UTF-16 string".to_string()))?,
            );
        }
        Ok(string)
    }

    /// Store a slice of memory
    pub fn store(&self, offset: usize, len: usize, data: &[u8]) -> Result<()> {
        self.instance.borrow_mut().store(offset, 0, data, len)
    }
}

/// A reference to a global instance
#[derive(Debug, Clone)]
pub struct GlobalRef {
    pub(crate) instance: Rc<RefCell<GlobalInstance>>,
}

impl GlobalRef {
    /// Get the value of the global
    pub fn get(&self) -> WasmValue {
        self.instance.borrow().get()
    }

    /// Set the value of the global
    pub fn set(&self, val: WasmValue) -> Result<()> {
        self.instance.borrow_mut().set(val)
    }
}
