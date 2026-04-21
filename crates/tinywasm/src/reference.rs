use core::hint::cold_path;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{ffi::CString, format};

use crate::store::{GlobalInstance, TableElement, TableInstance};
use crate::{Error, MemoryInstance, Result, Store, Trap};
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
    pub(crate) fn validate_store(&self, store: &Store) -> Result<(), Trap> {
        if self.store_id != store.id() {
            return Err(Trap::Other("invalid store"));
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

/// A cursor over a [`Memory`] instance.
///
/// Available with the `std` feature enabled.
#[cfg(feature = "std")]
pub struct MemoryCursor<'a> {
    memory: &'a mut MemoryInstance,
    position: u64,
}

#[cfg(feature = "std")]
impl<'a> MemoryCursor<'a> {
    fn new(memory: &'a mut MemoryInstance, position: u64) -> Self {
        Self { memory, position }
    }

    fn offset(&self) -> crate::std::io::Result<usize> {
        usize::try_from(self.position).map_err(|_| {
            crate::std::io::Error::new(crate::std::io::ErrorKind::InvalidInput, "cursor position exceeds usize")
        })
    }

    fn advance(&mut self, amount: usize) -> crate::std::io::Result<()> {
        self.position = self.position.checked_add(amount as u64).ok_or_else(|| {
            crate::std::io::Error::new(crate::std::io::ErrorKind::InvalidInput, "cursor position overflow")
        })?;
        Ok(())
    }

    /// Returns the current cursor position.
    pub const fn position(&self) -> u64 {
        self.position
    }

    /// Sets the current cursor position.
    pub fn set_position(&mut self, position: u64) {
        self.position = position;
    }
}

#[cfg(feature = "std")]
impl crate::std::io::Read for MemoryCursor<'_> {
    fn read(&mut self, buf: &mut [u8]) -> crate::std::io::Result<usize> {
        let offset = self.offset()?;
        let read = self.memory.inner.read(offset, buf);
        self.advance(read)?;
        Ok(read)
    }
}

#[cfg(feature = "std")]
impl crate::std::io::Write for MemoryCursor<'_> {
    fn write(&mut self, buf: &[u8]) -> crate::std::io::Result<usize> {
        let offset = self.offset()?;
        let written = self.memory.inner.write(offset, buf);
        self.advance(written)?;
        Ok(written)
    }

    fn flush(&mut self) -> crate::std::io::Result<()> {
        Ok(())
    }
}

#[cfg(feature = "std")]
impl crate::std::io::Seek for MemoryCursor<'_> {
    fn seek(&mut self, pos: crate::std::io::SeekFrom) -> crate::std::io::Result<u64> {
        let len = self.memory.inner.len() as i128;
        let current = i128::from(self.position);

        let next = match pos {
            crate::std::io::SeekFrom::Start(offset) => i128::from(offset),
            crate::std::io::SeekFrom::End(offset) => len + i128::from(offset),
            crate::std::io::SeekFrom::Current(offset) => current + i128::from(offset),
        };

        if next < 0 {
            return Err(crate::std::io::Error::new(
                crate::std::io::ErrorKind::InvalidInput,
                "invalid seek before start",
            ));
        }

        let next = u64::try_from(next).map_err(|_| {
            crate::std::io::Error::new(crate::std::io::ErrorKind::InvalidInput, "invalid seek position")
        })?;
        self.position = next;
        Ok(next)
    }
}

impl Memory {
    #[inline]
    pub(crate) const fn from_store_addr(store_id: usize, addr: MemAddr) -> Self {
        Self(StoreItem::new(store_id, addr))
    }

    /// Create a new memory in the given store.
    pub fn new(store: &mut Store, ty: MemoryType) -> Result<Self> {
        if let MemoryArch::I64 = ty.arch() {
            return Err(Error::UnsupportedFeature("64-bit memories"));
        }
        let addr = store.state.memories.len() as MemAddr;
        store.state.memories.push(MemoryInstance::new(ty, &store.engine.config().memory_backend)?);
        Ok(Self::from_store_addr(store.id(), addr))
    }

    /// Creates a cursor positioned at the start of this memory.
    ///
    /// Available with the `std` feature enabled.
    #[cfg(feature = "std")]
    pub fn cursor<'a>(&self, store: &'a mut Store) -> Result<MemoryCursor<'a>> {
        self.cursor_at(store, 0)
    }

    /// Creates a cursor positioned at `position` bytes from the start of this memory.
    ///
    /// Available with the `std` feature enabled.
    #[cfg(feature = "std")]
    pub fn cursor_at<'a>(&self, store: &'a mut Store, position: u64) -> Result<MemoryCursor<'a>> {
        Ok(MemoryCursor::new(self.instance_mut(store)?, position))
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

    /// Returns the raw memory byte length.
    pub fn len(&self, store: &Store) -> Result<usize> {
        Ok(self.instance(store)?.inner.len())
    }

    /// Returns the memory type, including page size and limits.
    pub fn ty(&self, store: &Store) -> Result<MemoryType> {
        Ok(self.instance(store)?.kind)
    }

    /// Reads up to `dst.len()` bytes from memory and returns the number of bytes read.
    ///
    /// Depending on the configured backend, this may return fewer bytes than requested even when
    /// more data is available. Use [`Self::read_exact`] or [`Self::read_vec`] when you need a full
    /// range.
    pub fn read(&self, store: &Store, offset: usize, dst: &mut [u8]) -> Result<usize> {
        Ok(self.instance(store)?.inner.read(offset, dst))
    }

    /// Writes up to `src.len()` bytes into memory and returns the number of bytes written.
    ///
    /// Depending on the configured backend, this may return fewer bytes than requested even when
    /// more space is available. Use [`Self::copy_from_slice`] when you need the full slice written.
    pub fn write(&self, store: &mut Store, offset: usize, src: &[u8]) -> Result<usize> {
        Ok(self.instance_mut(store)?.inner.write(offset, src))
    }

    /// Reads exactly `dst.len()` bytes from memory.
    pub fn read_exact(&self, store: &Store, offset: usize, dst: &mut [u8]) -> Result<()> {
        self.instance(store)?.inner.read_exact(offset, dst).ok_or_else(|| {
            Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset,
                len: dst.len(),
                max: self.instance(store).unwrap().inner.len(),
            })
        })
    }

    /// Reads `len` bytes from memory into a newly allocated buffer.
    pub fn read_vec(&self, store: &Store, offset: usize, len: usize) -> Result<Vec<u8>> {
        self.instance(store)?.inner.read_vec(offset, len).ok_or_else(|| {
            Error::Trap(crate::Trap::MemoryOutOfBounds { offset, len, max: self.instance(store).unwrap().inner.len() })
        })
    }

    /// Grow the memory by the given number of pages.
    pub fn grow(&self, store: &mut Store, delta_pages: i64) -> Result<Option<i64>> {
        self.instance_mut(store)?.grow(delta_pages, true).map_err(Into::into)
    }

    /// Get the current size of the memory in pages.
    pub fn page_count(&self, store: &Store) -> Result<usize> {
        Ok(self.instance(store)?.page_count)
    }

    /// Copy a slice of memory to another place in memory.
    pub fn copy_within(&self, store: &mut Store, src: usize, dst: usize, len: usize) -> Result<()> {
        self.instance_mut(store)?.copy_within(dst, src, len)?;
        Ok(())
    }

    /// Fill a slice of memory with a value.
    pub fn fill(&self, store: &mut Store, offset: usize, len: usize, val: u8) -> Result<()> {
        self.instance_mut(store)?.inner.fill(offset, len, val).ok_or_else(|| {
            Error::Trap(crate::Trap::MemoryOutOfBounds { offset, len, max: self.instance(store).unwrap().inner.len() })
        })
    }

    /// Copies a full slice into memory.
    pub fn copy_from_slice(&self, store: &mut Store, offset: usize, data: &[u8]) -> Result<()> {
        self.instance_mut(store)?.inner.write_all(offset, data).ok_or_else(|| {
            Error::Trap(crate::Trap::MemoryOutOfBounds {
                offset,
                len: data.len(),
                max: self.instance(store).unwrap().inner.len(),
            })
        })
    }

    /// Reads a C-style string from memory.
    pub fn read_cstring(&self, store: &Store, offset: usize, len: usize) -> Result<CString> {
        CString::from_vec_with_nul(self.read_vec(store, offset, len)?)
            .map_err(|e| crate::Error::Other(format!("Invalid C-style string: {e}")))
    }

    /// Reads a C-style string from memory, stopping at the first null byte.
    pub fn read_cstring_until_null(&self, store: &Store, offset: usize, max_len: usize) -> Result<CString> {
        let bytes = self.read_vec(store, offset, max_len)?;
        let Some(null) = bytes.iter().position(|byte| *byte == 0) else {
            return Err(crate::Error::Other("Invalid C-style string: missing null terminator".to_string()));
        };

        CString::from_vec_with_nul(bytes[..=null].to_vec())
            .map_err(|e| crate::Error::Other(format!("Invalid C-style string: {e}")))
    }

    /// Reads a UTF-8 string from memory.
    pub fn read_string(&self, store: &Store, offset: usize, len: usize) -> Result<String> {
        String::from_utf8(self.read_vec(store, offset, len)?)
            .map_err(|e| crate::Error::Other(format!("Invalid UTF-8 string: {e}")))
    }

    /// Reads a JavaScript-style utf-16 string from memory.
    pub fn read_js_string(&self, store: &Store, offset: usize, len: usize) -> Result<String> {
        let bytes = self.read_vec(store, offset, len)?;
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

fn table_value_to_element(element_type: WasmType, value: WasmValue) -> Result<TableElement, Trap> {
    match (element_type, value) {
        (WasmType::RefFunc, WasmValue::RefFunc(func_ref)) => Ok(TableElement::from(func_ref.addr())),
        (WasmType::RefExtern, WasmValue::RefExtern(extern_ref)) => Ok(TableElement::from(extern_ref.addr())),
        _ => Err(Trap::Other("invalid table value type")),
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
    fn instance_mut<'a>(&self, store: &'a mut Store) -> Result<&'a mut TableInstance, Trap> {
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
        Ok(self.instance(store)?.get_wasm_val(index)?)
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
    pub fn set(&self, store: &mut Store, index: TableAddr, value: WasmValue) -> Result<(), Trap> {
        let table = self.instance_mut(store)?;
        let value = table_value_to_element(table.kind.element_type, value)?;
        table.set(index, value)
    }

    /// Copy elements within the same table.
    pub fn copy_within(&self, store: &mut Store, src: usize, dst: usize, len: usize) -> Result<(), Trap> {
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
