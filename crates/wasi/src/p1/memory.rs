use tinywasm::types::MemoryArch;
use tinywasm::{Error, FuncContext, Memory, Store};

use super::abi::{Errno, FAULT, INVAL, NOMEM};

const MAX_IOVECS: usize = 1024;
const MAX_IO_BYTES: usize = 16 * 1024 * 1024;

/// Provides checked access to a guest's wasm32 memory.
pub(super) struct GuestMemory(Memory);

/// A bounded and validated guest iovec array.
pub(super) struct GuestIovecs {
    pub(super) entries: Vec<(usize, usize)>,
    pub(super) byte_len: usize,
}

impl GuestMemory {
    /// Finds and validates the guest's exported memory.
    pub(super) fn new(ctx: &FuncContext<'_>) -> tinywasm::Result<Self> {
        let memory = ctx.memory("memory")?;
        if memory.ty(ctx.store())?.arch() != MemoryArch::I32 {
            return Err(Error::UnsupportedFeature("WASI Preview 1 requires wasm32 memory"));
        }
        Ok(Self(memory))
    }

    /// Reads bytes using a wasm32 pointer and length.
    pub(super) fn read(&self, store: &Store, pointer: i32, len: i32) -> Result<Vec<u8>, Errno> {
        let len = len as u32 as usize;
        self.read_at(store, pointer as u32 as usize, len)
    }

    /// Reads bytes using host-sized offsets.
    pub(super) fn read_at(&self, store: &Store, offset: usize, len: usize) -> Result<Vec<u8>, Errno> {
        if !self.is_valid_range(store, offset, len) {
            return Err(FAULT);
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(len).map_err(|_| NOMEM)?;
        bytes.resize(len, 0);
        self.0.read_exact(store, offset, &mut bytes).map_err(|_| FAULT)?;
        Ok(bytes)
    }

    /// Writes bytes using a wasm32 pointer.
    pub(super) fn write(&self, store: &mut Store, pointer: i32, bytes: &[u8]) -> Result<(), Errno> {
        self.write_at(store, pointer as u32 as usize, bytes)
    }

    /// Writes bytes using a host-sized offset.
    pub(super) fn write_at(&self, store: &mut Store, offset: usize, bytes: &[u8]) -> Result<(), Errno> {
        self.check_range(store, offset, bytes.len())?;
        self.0.copy_from_slice(store, offset, bytes).map_err(|_| FAULT)
    }

    /// Validates that a guest memory range is accessible.
    pub(super) fn check_range(&self, store: &Store, offset: usize, len: usize) -> Result<(), Errno> {
        if self.is_valid_range(store, offset, len) { Ok(()) } else { Err(FAULT) }
    }

    /// Reads and validates a guest iovec array.
    pub(super) fn read_iovecs(&self, store: &Store, pointer: i32, count: i32) -> Result<GuestIovecs, Errno> {
        let count = count as u32 as usize;
        if count > MAX_IOVECS {
            return Err(INVAL);
        }
        let table_len = count.checked_mul(8).ok_or(INVAL)?;
        let table = self.read_at(store, pointer as u32 as usize, table_len)?;
        let mut total = 0usize;
        let mut entries = Vec::new();
        entries.try_reserve_exact(count).map_err(|_| NOMEM)?;
        for entry in table.as_chunks::<8>().0 {
            let offset = u32::from_le_bytes(entry[..4].try_into().expect("iovec field")) as usize;
            let len = u32::from_le_bytes(entry[4..].try_into().expect("iovec field")) as usize;
            total = total.checked_add(len).ok_or(INVAL)?;
            if total > MAX_IO_BYTES || !self.is_valid_range(store, offset, len) {
                return Err(if total > MAX_IO_BYTES { INVAL } else { FAULT });
            }
            entries.push((offset, len));
        }
        Ok(GuestIovecs { entries, byte_len: total })
    }

    /// Copies guest iovec contents into one contiguous buffer.
    pub(super) fn gather(&self, store: &Store, iovecs: &GuestIovecs) -> Result<Vec<u8>, Errno> {
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(iovecs.byte_len).map_err(|_| NOMEM)?;
        bytes.resize(iovecs.byte_len, 0);
        let mut destination = 0;
        for &(offset, len) in &iovecs.entries {
            self.0.read_exact(store, offset, &mut bytes[destination..destination + len]).map_err(|_| FAULT)?;
            destination += len;
        }
        Ok(bytes)
    }

    /// Copies one contiguous buffer into guest iovecs.
    pub(super) fn scatter(&self, store: &mut Store, iovecs: &GuestIovecs, bytes: &[u8]) -> Result<(), Errno> {
        let mut source = 0;
        for &(offset, len) in &iovecs.entries {
            let len = len.min(bytes.len() - source);
            self.write_at(store, offset, &bytes[source..source + len])?;
            source += len;
            if source == bytes.len() {
                break;
            }
        }
        Ok(())
    }

    fn is_valid_range(&self, store: &Store, offset: usize, len: usize) -> bool {
        self.0.len(store).ok().is_some_and(|memory_len| offset.checked_add(len).is_some_and(|end| end <= memory_len))
    }
}
