#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs, rust_2018_idioms)]

//! Opt-in, allocation-free access to a small subset of the component canonical ABI.
//!
//! This crate does not parse, validate, instantiate, or link components. It helps
//! hosts implementing WIT-shaped imports for core Wasm modules read and write
//! canonical-ABI-shaped values in a selected wasm32 linear memory. The memory
//! is supplied explicitly: canonical ABI memories need not be named `memory`.
//!
//! Only UTF-8 strings, `list<u8>`, and `list<u32>` are covered here. Full
//! component values, resource handles, `realloc`, post-return, and async calls
//! require further work. Per-transfer bounds do not replace store fuel, memory
//! limits, or per-instance resource quotas.

extern crate alloc;

use alloc::string::ToString;
use core::fmt;
use tinywasm::types::MemoryArch;
use tinywasm::{Memory, Store};

/// The canonical ABI's maximum string/list byte length.
pub const CANONICAL_MAX_BYTES: usize = (1 << 28) - 1;

/// A host-selected bound on each string or list transfer.
///
/// The effective bound is also capped by [`CANONICAL_MAX_BYTES`]. A host
/// should use a smaller value appropriate to its device and interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum bytes read or written in one operation.
    pub max_transfer_bytes: usize,
}

/// A failed canonical ABI memory operation.
#[derive(Debug)]
pub enum AbiError {
    /// The memory handle is invalid for this store or memory access failed.
    Runtime(tinywasm::Error),
    /// This initial helper only handles wasm32 memory.
    Memory64,
    /// A string or list exceeds the host or canonical ABI bound.
    TooLarge,
    /// Pointer arithmetic overflowed or the region is outside linear memory.
    OutOfBounds,
    /// A list pointer does not satisfy its element alignment.
    Misaligned,
    /// The canonical UTF-8 string has invalid encoding.
    InvalidUtf8,
}

impl fmt::Display for AbiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => write!(f, "{error}"),
            Self::Memory64 => f.write_str("canonical ABI helper requires wasm32 memory"),
            Self::TooLarge => f.write_str("canonical ABI transfer exceeds the configured limit"),
            Self::OutOfBounds => f.write_str("canonical ABI memory range is out of bounds"),
            Self::Misaligned => f.write_str("canonical ABI list pointer is misaligned"),
            Self::InvalidUtf8 => f.write_str("canonical ABI string is not valid UTF-8"),
        }
    }
}

impl From<tinywasm::Error> for AbiError {
    fn from(error: tinywasm::Error) -> Self {
        Self::Runtime(error)
    }
}

impl From<AbiError> for tinywasm::Error {
    fn from(error: AbiError) -> Self {
        match error {
            AbiError::Runtime(error) => error,
            error => Self::Other(error.to_string()),
        }
    }
}

/// Checked access to one explicitly selected wasm32 linear memory.
///
/// Reads borrow the store's memory; they neither allocate nor copy. The borrow
/// must end before the guest can execute or grow that memory again.
pub struct CanonicalMemory {
    memory: Memory,
    limits: Limits,
}

impl CanonicalMemory {
    /// Binds a memory handle and an explicit per-transfer limit.
    pub fn new(memory: Memory, store: &Store, limits: Limits) -> Result<Self, AbiError> {
        if memory.ty(store)?.arch() != MemoryArch::I32 {
            return Err(AbiError::Memory64);
        }
        Ok(Self { memory, limits })
    }

    /// Borrows a canonical `list<u8>` from guest memory.
    pub fn bytes<'a>(&self, store: &'a Store, ptr: u32, len: u32) -> Result<&'a [u8], AbiError> {
        let memory = self.memory.data(store)?;
        self.range(memory.len(), ptr, len as usize, 1).map(|range| &memory[range])
    }

    /// Borrows a canonical UTF-8 string from guest memory.
    pub fn utf8<'a>(&self, store: &'a Store, ptr: u32, len: u32) -> Result<&'a str, AbiError> {
        core::str::from_utf8(self.bytes(store, ptr, len)?).map_err(|_| AbiError::InvalidUtf8)
    }

    /// Borrows a canonical `list<u32>` without allocating or copying.
    ///
    /// Values are decoded from little-endian bytes by the returned iterator.
    pub fn u32_list<'a>(&self, store: &'a Store, ptr: u32, len: u32) -> Result<U32List<'a>, AbiError> {
        let bytes = (len as usize).checked_mul(4).ok_or(AbiError::TooLarge)?;
        let memory = self.memory.data(store)?;
        let range = self.range(memory.len(), ptr, bytes, 4)?;
        Ok(U32List { chunks: memory[range].as_chunks::<4>().0.iter() })
    }

    /// Writes into an already allocated canonical `list<u8>` or UTF-8 region.
    ///
    /// Allocation and `realloc` are deliberately left to the caller.
    pub fn write_bytes(&self, store: &mut Store, ptr: u32, bytes: &[u8]) -> Result<(), AbiError> {
        let memory = self.memory.data_mut(store)?;
        let range = self.range(memory.len(), ptr, bytes.len(), 1)?;
        memory[range].copy_from_slice(bytes);
        Ok(())
    }

    fn range(
        &self,
        memory_len: usize,
        ptr: u32,
        bytes: usize,
        align: usize,
    ) -> Result<core::ops::Range<usize>, AbiError> {
        if bytes > self.limits.max_transfer_bytes.min(CANONICAL_MAX_BYTES) {
            return Err(AbiError::TooLarge);
        }
        let start = ptr as usize;
        if !start.is_multiple_of(align) {
            return Err(AbiError::Misaligned);
        }
        let end = start.checked_add(bytes).ok_or(AbiError::OutOfBounds)?;
        if end > memory_len {
            return Err(AbiError::OutOfBounds);
        }
        Ok(start..end)
    }
}

/// A borrowed iterator over canonical little-endian `list<u32>` elements.
pub struct U32List<'a> {
    chunks: core::slice::Iter<'a, [u8; 4]>,
}

impl Iterator for U32List<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        self.chunks.next().map(|bytes| u32::from_le_bytes(*bytes))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let count = self.chunks.len();
        (count, Some(count))
    }
}

impl ExactSizeIterator for U32List<'_> {}
