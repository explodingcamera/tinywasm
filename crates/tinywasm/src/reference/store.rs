use core::hint::cold_path;

use alloc::ffi::CString;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::managed::{ReferentKind, StoreId, StoredRef};
use crate::interpreter::ValueRef;
use crate::store::TableInstance;
use crate::{ArrayRef, Error, ExnRef, I31Ref, MemoryInstance, Result, Store, StructRef, Trap, WasmValue};
use tinywasm_types::{
    AbstractHeapType, Addr, CompositeType, FieldType, FuncType, GlobalType, MemAddr, MemoryType, StorageType,
    TableAddr, TableType, TagAddr, TypeAddr, WasmType,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub(crate) struct StoreItem {
    pub(crate) store_id: StoreId,
    pub(crate) addr: Addr,
}

impl StoreItem {
    #[inline]
    /// Creates a handle for an address owned by a store.
    pub(crate) const fn new(store_id: StoreId, addr: Addr) -> Self {
        Self { store_id, addr }
    }

    #[inline]
    pub(crate) fn validate_store(&self, store: &Store) -> Result<(), Trap> {
        if self.store_id != store.store_id() {
            return Err(Trap::InvalidStore);
        }
        Ok(())
    }
}

/// A memory instance in a store.
///
/// ## Example
/// ```rust
/// # fn main() -> tinywasm::Result<()> {
/// use tinywasm::types::MemoryType;
/// use tinywasm::{Memory, Store};
///
/// let mut store = Store::default();
/// let memory = Memory::try_new(&mut store, MemoryType::default().with_page_count_initial(1))?;
///
/// memory.copy_from_slice(&mut store, 0, b"hi")?;
/// assert_eq!(memory.read_vec(&store, 0, 2)?, b"hi");
/// assert_eq!(memory.page_count(&store)?, 1);
/// memory.grow(&mut store, 1)?;
/// assert_eq!(memory.page_count(&store)?, 2);
/// # Ok(())
/// # }
/// ```
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

/// A tag instance in a store.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct Tag(pub(crate) StoreItem);

/// An opaque canonical function, struct, or array type owned by a Store.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct GcType(StoreItem);

/// The composite kind of a [`GcType`].
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum GcTypeKind {
    /// A function type.
    Function,
    /// A struct type.
    Struct,
    /// An array type.
    Array,
}

/// Metadata for a struct field or array element.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct GcFieldType {
    storage: GcStorageType,
    mutable: bool,
}

impl GcFieldType {
    /// Returns the field's storage type.
    pub const fn storage(self) -> GcStorageType {
        self.storage
    }

    /// Returns whether the field can be changed.
    pub const fn is_mutable(self) -> bool {
        self.mutable
    }
}

/// Host-visible storage metadata for a GC field or array element.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum GcStorageType {
    /// A packed 8-bit integer.
    I8,
    /// A packed 16-bit integer.
    I16,
    /// An unpacked WebAssembly value.
    Value(GcValueType),
}

/// Host-visible value metadata that does not expose concrete reference encodings.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum GcValueType {
    /// A 32-bit integer.
    I32,
    /// A 64-bit integer.
    I64,
    /// A 32-bit float.
    F32,
    /// A 64-bit float.
    F64,
    /// A 128-bit vector.
    V128,
    /// A reference type.
    Ref(GcRefType),
}

/// Host-visible reference metadata with opaque concrete types.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub struct GcRefType {
    nullable: bool,
    heap_type: GcHeapType,
}

impl GcRefType {
    /// Returns whether the reference type accepts null.
    pub const fn is_nullable(self) -> bool {
        self.nullable
    }

    /// Returns the abstract or opaque concrete heap type.
    pub const fn heap_type(self) -> GcHeapType {
        self.heap_type
    }
}

/// Host-visible heap type metadata.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "debug", derive(Debug))]
pub enum GcHeapType {
    /// An abstract WebAssembly heap type.
    Abstract(AbstractHeapType),
    /// An opaque canonical type owned by the Store.
    Concrete(GcType),
}

impl GcType {
    fn subtype(self, store: &Store) -> Result<&tinywasm_types::SubType> {
        self.0.validate_store(store)?;
        store.state.canonical_types.get(self.0.addr as usize).ok_or_else(|| Trap::InvalidReference.into())
    }

    /// Returns this type's composite kind.
    pub fn kind(self, store: &Store) -> Result<GcTypeKind> {
        Ok(match &self.subtype(store)?.composite {
            CompositeType::Func(_) => GcTypeKind::Function,
            CompositeType::Struct(_) => GcTypeKind::Struct,
            CompositeType::Array(_) => GcTypeKind::Array,
        })
    }

    /// Returns whether this type is a nominal subtype of `other`.
    pub fn is_subtype_of(self, store: &Store, other: Self) -> Result<bool> {
        self.subtype(store)?;
        other.subtype(store)?;
        Ok(store.state.type_addr_is_subtype(self.0.addr, other.0.addr))
    }

    /// Returns the number of struct fields.
    pub fn field_count(self, store: &Store) -> Result<usize> {
        self.subtype(store)?
            .as_struct()
            .map(|ty| ty.fields.len())
            .ok_or_else(|| Error::other("GC type is not a struct"))
    }

    /// Returns metadata for one struct field.
    pub fn field(self, store: &Store, index: usize) -> Result<GcFieldType> {
        let field = self
            .subtype(store)?
            .as_struct()
            .ok_or_else(|| Error::other("GC type is not a struct"))?
            .fields
            .get(index)
            .copied()
            .ok_or_else(|| Error::other("struct field out of bounds"))?;
        Ok(gc_field_type(store, field))
    }

    /// Returns metadata for an array element.
    pub fn array_element(self, store: &Store) -> Result<GcFieldType> {
        let field = self.subtype(store)?.as_array().ok_or_else(|| Error::other("GC type is not an array"))?.field;
        Ok(gc_field_type(store, field))
    }
}

fn gc_field_type(store: &Store, field: FieldType) -> GcFieldType {
    let storage = match field.storage {
        StorageType::I8 => GcStorageType::I8,
        StorageType::I16 => GcStorageType::I16,
        StorageType::Value(ty) => GcStorageType::Value(match ty {
            WasmType::I32 => GcValueType::I32,
            WasmType::I64 => GcValueType::I64,
            WasmType::F32 => GcValueType::F32,
            WasmType::F64 => GcValueType::F64,
            WasmType::V128 => GcValueType::V128,
            WasmType::Ref(ty) => GcValueType::Ref(GcRefType {
                nullable: ty.is_nullable(),
                heap_type: match ty.type_index() {
                    Some(addr) => GcHeapType::Concrete(GcType(StoreItem::new(store.store_id(), addr))),
                    None => GcHeapType::Abstract(ty.abstract_heap_type().expect("abstract reference type")),
                },
            }),
        }),
    };
    GcFieldType { storage, mutable: field.mutable }
}

fn rooted_object<T: StoredRef>(store: &Store, root: &T, kind: ReferentKind) -> Result<(ValueRef, TypeAddr)> {
    let value = store.resolve_ref(root)?;
    if root.rooted_item().kind != kind {
        return Err(Trap::InvalidReference.into());
    }
    let object = store.state.gc.get(value).ok_or(Trap::InvalidReference)?;
    let crate::store::GcObjectKind::Composite(type_addr) = object.kind else {
        return Err(Trap::InvalidReference.into());
    };
    Ok((value, type_addr))
}

/// A cursor over a [`Memory`] instance.
///
/// Available with the `std` feature enabled.
#[cfg(feature = "std")]
pub struct MemoryCursor<'a> {
    memory: &'a mut MemoryInstance,
    position: u64,
}

#[cfg(feature = "std")]
impl MemoryCursor<'_> {
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
    /// Create a new memory in the given store.
    pub fn try_new(store: &mut Store, ty: MemoryType) -> Result<Self> {
        let addr = store.state.memories.len() as MemAddr;
        let limiter = store.engine.config().resource_limiter.clone();
        store.state.memories.push(MemoryInstance::new(ty, limiter.as_deref())?);
        Ok(Self(StoreItem::new(store.store_id(), addr)))
    }

    /// Creates a cursor positioned at the start of this memory.
    ///
    /// Available with the `std` feature enabled.
    ///
    /// ## Example
    ///
    /// ```rust
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::io::{Read, Seek, SeekFrom, Write};
    /// use tinywasm::types::MemoryType;
    /// use tinywasm::{Memory, Store};
    ///
    /// let mut store = Store::default();
    /// let memory = Memory::try_new(&mut store, MemoryType::default().with_page_count_initial(1))?;
    /// let mut cursor = memory.cursor(&mut store)?;
    ///
    /// cursor.seek(SeekFrom::Start(2))?;
    /// cursor.write_all(b"abc")?;
    /// cursor.seek(SeekFrom::Start(0))?;
    ///
    /// let mut bytes = [0; 5];
    /// cursor.read_exact(&mut bytes)?;
    /// assert_eq!(bytes, [0, 0, b'a', b'b', b'c']);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(feature = "std")]
    pub fn cursor<'a>(&self, store: &'a mut Store) -> Result<MemoryCursor<'a>> {
        self.cursor_at(store, 0)
    }

    /// Creates a cursor positioned at `position` bytes from the start of this memory.
    ///
    /// Available with the `std` feature enabled.
    #[cfg(feature = "std")]
    pub fn cursor_at<'a>(&self, store: &'a mut Store, position: u64) -> Result<MemoryCursor<'a>> {
        Ok(MemoryCursor { memory: self.instance_mut(store)?, position })
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
    /// This returns fewer bytes than requested when the range extends past the end of memory. Use
    /// [`Self::read_exact`] or [`Self::read_vec`] when you need a full range.
    pub fn read(&self, store: &Store, offset: usize, dst: &mut [u8]) -> Result<usize> {
        Ok(self.instance(store)?.inner.read(offset, dst))
    }

    /// Writes up to `src.len()` bytes into memory and returns the number of bytes written.
    ///
    /// This returns fewer bytes than requested when the range extends past the end of memory. Use
    /// [`Self::copy_from_slice`] when you need the full slice written.
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

    /// Grows the memory by the given number of pages.
    ///
    /// Returns the previous size, or `None` if growth fails or is rejected by the resource limiter.
    /// A limiter-provided trap is returned as an error.
    pub fn grow(&self, store: &mut Store, delta_pages: i64) -> Result<Option<i64>> {
        let limiter = store.engine.config().resource_limiter.clone();
        let mem = self.instance_mut(store)?;
        mem.grow(delta_pages, limiter.as_deref()).map_err(Into::into)
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

    /// Copies a nul-terminated C string into memory.
    pub fn write_cstring(&self, store: &mut Store, offset: usize, string: &CString) -> Result<()> {
        self.copy_from_slice(store, offset, string.as_bytes_with_nul())
    }

    /// Copies a UTF-8 string into memory and appends a trailing nul byte.
    pub fn write_cstring_bytes(&self, store: &mut Store, offset: usize, string: &str) -> Result<()> {
        let mut bytes = Vec::with_capacity(string.len() + 1);
        bytes.extend_from_slice(string.as_bytes());
        bytes.push(0);
        self.copy_from_slice(store, offset, &bytes)
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

fn table_value_to_element(
    store: &Store,
    element_type: tinywasm_types::RefType,
    value: WasmValue,
) -> Result<ValueRef, Trap> {
    let WasmValue::Ref(reference) = &value else {
        return Err(Trap::Other("invalid table value type"));
    };
    if !store.value_matches_type(&value, WasmType::Ref(element_type)) {
        return Err(Trap::Other("invalid table value type"));
    }
    store.encode_ref(reference).map_err(|error| match error {
        Error::Trap(trap) => trap,
        _ => Trap::Other("invalid table value type"),
    })
}

impl I31Ref {
    /// Returns the signed i31 value.
    pub fn value(&self, store: &Store) -> Result<i32> {
        if self.rooted_item().kind != ReferentKind::I31 {
            return Err(Trap::InvalidReference.into());
        }
        store.resolve_ref(self)?.i31_s().ok_or_else(|| Trap::InvalidReference.into())
    }
}

impl StructRef {
    /// Returns this struct's canonical type.
    pub fn ty(&self, store: &Store) -> Result<GcType> {
        let (_, type_addr) = rooted_object(store, self, ReferentKind::Struct)?;
        Ok(GcType(StoreItem::new(store.store_id(), type_addr)))
    }

    /// Reads one field.
    ///
    /// Packed integer fields are returned as zero-extended `i32` values. Mutable
    /// Store access is required to register owned reference results.
    pub fn field(&self, store: &mut Store, index: usize) -> Result<WasmValue> {
        let (reference, type_addr) = rooted_object(store, self, ReferentKind::Struct)?;
        let storage = store
            .state
            .get_type(type_addr)
            .as_struct()
            .unwrap()
            .fields
            .get(index)
            .ok_or_else(|| Error::other("struct field out of bounds"))?
            .storage;
        let value = *store.state.gc.get(reference).unwrap().values.get(index).unwrap();
        value.into_wasm(store, storage.unpacked())
    }

    /// Reads all fields in declaration order.
    ///
    /// Mutable Store access is required because reference results are registered with the Store.
    pub fn fields(&self, store: &mut Store) -> Result<Vec<WasmValue>> {
        let (reference, type_addr) = rooted_object(store, self, ReferentKind::Struct)?;
        let field_count = store.state.get_type(type_addr).as_struct().unwrap().fields.len();
        let mut values = Vec::new();
        values.try_reserve_exact(field_count).map_err(|_| Trap::OutOfMemory)?;
        for index in 0..field_count {
            let storage = store.state.get_type(type_addr).as_struct().unwrap().fields[index].storage;
            let value = store.state.gc.get(reference).unwrap().values[index];
            values.push(value.into_wasm(store, storage.unpacked())?);
        }
        Ok(values)
    }

    /// Writes one mutable field after validating its canonical value type.
    ///
    /// Writes to packed integer fields truncate the input to the field width.
    pub fn set_field(&self, store: &mut Store, index: usize, value: WasmValue) -> Result<()> {
        let (reference, type_addr) = rooted_object(store, self, ReferentKind::Struct)?;
        let field = *store
            .state
            .get_type(type_addr)
            .as_struct()
            .unwrap()
            .fields
            .get(index)
            .ok_or_else(|| Error::other("struct field out of bounds"))?;
        set_gc_value(store, reference, index, field, value, "struct field")
    }
}

impl ArrayRef {
    /// Returns this array's canonical type.
    pub fn ty(&self, store: &Store) -> Result<GcType> {
        let (_, type_addr) = rooted_object(store, self, ReferentKind::Array)?;
        Ok(GcType(StoreItem::new(store.store_id(), type_addr)))
    }

    /// Returns the number of array elements.
    pub fn len(&self, store: &Store) -> Result<usize> {
        let (reference, _) = rooted_object(store, self, ReferentKind::Array)?;
        Ok(store.state.gc.get(reference).unwrap().values.len())
    }

    /// Returns whether the array contains no elements.
    pub fn is_empty(&self, store: &Store) -> Result<bool> {
        Ok(self.len(store)? == 0)
    }

    /// Reads one array element.
    ///
    /// Packed integer elements are returned as zero-extended `i32` values. Mutable
    /// Store access is required to register owned reference results.
    pub fn get(&self, store: &mut Store, index: usize) -> Result<WasmValue> {
        let (reference, type_addr) = rooted_object(store, self, ReferentKind::Array)?;
        let storage = store.state.get_type(type_addr).as_array().unwrap().field.storage;
        let value = *store.state.gc.get(reference).unwrap().values.get(index).ok_or(Trap::ArrayOutOfBounds)?;
        value.into_wasm(store, storage.unpacked())
    }

    /// Writes one mutable array element after validating its canonical value type.
    ///
    /// Writes to packed integer elements truncate the input to the element width.
    pub fn set(&self, store: &mut Store, index: usize, value: WasmValue) -> Result<()> {
        let (reference, type_addr) = rooted_object(store, self, ReferentKind::Array)?;
        let field = store.state.get_type(type_addr).as_array().unwrap().field;
        if index >= store.state.gc.get(reference).unwrap().values.len() {
            return Err(Trap::ArrayOutOfBounds.into());
        }
        set_gc_value(store, reference, index, field, value, "array element")
    }
}

fn set_gc_value(
    store: &mut Store,
    reference: ValueRef,
    index: usize,
    field: FieldType,
    value: WasmValue,
    field_name: &'static str,
) -> Result<()> {
    if !field.mutable {
        return Err(Error::other(format!("{field_name} is immutable")));
    }
    let expected = field.storage.unpacked();
    if !store.value_matches_type(&value, expected) {
        return Err(Error::other(format!("invalid {field_name} value type")));
    }
    let value = match (field.storage, value.to_runtime(store)?) {
        (StorageType::I8, crate::interpreter::RuntimeValue::Value32(value)) => {
            crate::interpreter::RuntimeValue::Value32(value as u8 as u32)
        }
        (StorageType::I16, crate::interpreter::RuntimeValue::Value32(value)) => {
            crate::interpreter::RuntimeValue::Value32(value as u16 as u32)
        }
        (_, value) => value,
    };
    let handle = store.state.gc.handle(reference).ok_or(Trap::InvalidReference)?;
    store.state.gc.set(handle, index, value).ok_or_else(|| Trap::InvalidReference.into())
}

impl ExnRef {
    /// Returns the exception's tag.
    pub fn tag(&self, store: &Store) -> Result<Tag> {
        let (_, tag_addr) = exception_object(store, self)?;
        Ok(Tag(StoreItem::new(store.store_id(), tag_addr)))
    }

    /// Reads one exception payload field.
    ///
    /// Mutable Store access is required because reference results are registered with the Store.
    pub fn field(&self, store: &mut Store, index: usize) -> Result<WasmValue> {
        let (reference, tag_addr) = exception_object(store, self)?;
        let type_addr = store.state.get_tag(tag_addr).type_addr;
        let ty = *store
            .state
            .get_canonical_func_type(type_addr)
            .params()
            .get(index)
            .ok_or_else(|| Error::other("exception payload field out of bounds"))?;
        let value = *store
            .state
            .gc
            .get(reference)
            .unwrap()
            .values
            .get(index)
            .ok_or_else(|| Error::other("exception payload field out of bounds"))?;
        value.into_wasm(store, ty)
    }

    /// Reads all exception payload fields.
    ///
    /// Mutable Store access is required because reference results are registered with the Store.
    pub fn fields(&self, store: &mut Store) -> Result<Vec<WasmValue>> {
        let (reference, tag_addr) = exception_object(store, self)?;
        let type_addr = store.state.get_tag(tag_addr).type_addr;
        let field_count = store.state.gc.get(reference).unwrap().values.len();
        let mut fields = Vec::new();
        fields.try_reserve_exact(field_count).map_err(|_| Trap::OutOfMemory)?;
        for index in 0..field_count {
            let ty = store.state.get_canonical_func_type(type_addr).params()[index];
            let value = store.state.gc.get(reference).unwrap().values[index];
            fields.push(value.into_wasm(store, ty)?);
        }
        Ok(fields)
    }
}

fn exception_object(store: &Store, root: &ExnRef) -> Result<(ValueRef, TagAddr)> {
    let reference = store.resolve_ref(root)?;
    if root.rooted_item().kind != ReferentKind::Exception {
        return Err(Trap::InvalidReference.into());
    }
    let object = store.state.gc.get(reference).ok_or(Trap::InvalidReference)?;
    let crate::store::GcObjectKind::Exception(tag_addr) = object.kind else {
        return Err(Trap::InvalidReference.into());
    };
    Ok((reference, tag_addr))
}

impl Table {
    /// Create a new table in the given store.
    pub fn try_new(store: &mut Store, ty: TableType, init: WasmValue) -> Result<Self> {
        if ty.element_type.is_concrete() {
            return Err(Error::other("host tables cannot use module-relative concrete reference types"));
        }
        let init = table_value_to_element(store, ty.element_type, init).map_err(Error::from)?;
        let limiter = store.engine.config().resource_limiter.clone();
        let addr = store.state.tables.len() as TableAddr;
        store.state.tables.push(TableInstance::new(ty, init, limiter.as_deref())?);
        Ok(Self(StoreItem::new(store.store_id(), addr)))
    }

    #[inline]
    fn instance<'a>(&self, store: &'a Store) -> Result<&'a TableInstance> {
        self.0.validate_store(store)?;
        Ok(store.state.get_table(self.0.addr))
    }

    /// Get the type of the table.
    pub fn ty(&self, store: &Store) -> Result<TableType> {
        Ok(self.instance(store)?.kind)
    }

    /// Get the current number of elements in the table.
    pub fn size(&self, store: &Store) -> Result<usize> {
        Ok(self.instance(store)?.size())
    }

    /// Get a table element as a wasm reference value.
    ///
    /// Mutable Store access is required because managed results are registered with the Store.
    pub fn get(&self, store: &mut Store, index: TableAddr) -> Result<WasmValue> {
        self.0.validate_store(store)?;
        let table = store.state.get_table(self.0.addr);
        let value = *table.get(index as usize)?;
        let element_type = table.kind.element_type;
        Ok(WasmValue::Ref(store.decode_ref(value, element_type)?))
    }

    /// Load a range of table elements and iterate over wasm reference values.
    ///
    /// Mutable Store access is required because managed results are registered with the Store.
    pub fn load(&self, store: &mut Store, offset: usize, len: usize) -> Result<alloc::vec::IntoIter<WasmValue>> {
        self.0.validate_store(store)?;
        let table = store.state.get_table(self.0.addr);
        let element_type = table.kind.element_type;
        let elements = table.load(offset, len)?.to_vec();
        let mut values = Vec::new();
        values.try_reserve_exact(elements.len()).map_err(|_| Trap::OutOfMemory)?;
        for value in elements {
            values.push(WasmValue::Ref(store.decode_ref(value, element_type)?));
        }
        Ok(values.into_iter())
    }

    /// Set a table element.
    pub fn set(&self, store: &mut Store, index: TableAddr, value: WasmValue) -> Result<(), Trap> {
        self.0.validate_store(store)?;
        let element_type = store.state.get_table(self.0.addr).kind.element_type;
        let value = table_value_to_element(store, element_type, value)?;
        store.state.get_table_mut(self.0.addr).set(index as usize, value)
    }

    /// Copy elements within the same table.
    pub fn copy_within(&self, store: &mut Store, src: usize, dst: usize, len: usize) -> Result<(), Trap> {
        self.0.validate_store(store)?;
        store.state.get_table_mut(self.0.addr).copy_within(dst, src, len)
    }

    /// Grows the table and returns the previous size.
    ///
    /// Returns `None` if growth fails or is rejected by the resource limiter. A limiter-provided
    /// trap is returned as an error.
    pub fn grow(&self, store: &mut Store, delta: i32, init: WasmValue) -> Result<Option<usize>> {
        self.0.validate_store(store)?;
        let table = store.state.get_table(self.0.addr);
        let old_size = table.size();
        let init = table_value_to_element(store, table.kind.element_type, init)?;
        let Ok(delta) = usize::try_from(delta) else {
            return Ok(None);
        };
        let limiter = store.engine.config().resource_limiter.clone();
        match store.state.get_table_mut(self.0.addr).grow(delta, init, limiter.as_deref())? {
            true => Ok(Some(old_size)),
            false => Ok(None),
        }
    }
}

impl Global {
    /// Create a new global in the given store.
    pub fn try_new(store: &mut Store, ty: GlobalType, value: WasmValue) -> Result<Self> {
        if matches!(ty.ty, WasmType::Ref(ty) if ty.is_concrete()) {
            return Err(Error::other("host globals cannot use module-relative concrete reference types"));
        }
        if !store.value_matches_type(&value, ty.ty) {
            cold_path();
            return Err(Error::Other("invalid global value type".to_string()));
        }
        let value = value.to_runtime(store)?;
        let addr = store.state.globals.push(ty, value);
        Ok(Self(StoreItem::new(store.store_id(), addr)))
    }

    /// Get the type of the global.
    pub fn ty(&self, store: &Store) -> Result<GlobalType> {
        self.0.validate_store(store)?;
        Ok(store.state.globals.ty(self.0.addr))
    }

    /// Get the current value of the global.
    ///
    /// Mutable Store access is required because an owned reference result is registered with the Store.
    pub fn get(&self, store: &mut Store) -> Result<WasmValue> {
        self.0.validate_store(store)?;
        let ty = store.state.globals.ty(self.0.addr).ty;
        let value = store.state.global_value(self.0.addr);
        value.into_wasm(store, ty)
    }

    /// Set the current value of the global.
    pub fn set(&self, store: &mut Store, value: WasmValue) -> Result<()> {
        self.0.validate_store(store)?;
        let ty = store.state.globals.ty(self.0.addr).ty;
        if !store.value_matches_type(&value, ty) {
            return Err(Error::other("invalid global value type"));
        }
        let value = value.to_runtime(store)?;
        store.state.set_global_value(self.0.addr, value)
    }
}

impl Tag {
    /// Create a new exception tag in the given store.
    pub fn try_new(store: &mut Store, ty: FuncType) -> Result<Self> {
        if !ty.results().is_empty() {
            return Err(Error::other("tag types must not have results"));
        }
        if ty.params().iter().any(|ty| matches!(ty, WasmType::Ref(ty) if ty.is_concrete())) {
            return Err(Error::other("host tags cannot use concrete reference types"));
        }
        let type_addr = store.register_host_type(&ty);
        let addr = store.state.tags.len() as TagAddr;
        store.state.tags.push(crate::store::TagInstance { type_addr });
        Ok(Self(StoreItem::new(store.store_id(), addr)))
    }

    /// Get the payload type of the tag.
    pub fn ty<'a>(&self, store: &'a Store) -> Result<&'a FuncType> {
        self.0.validate_store(store)?;
        Ok(store.state.get_canonical_func_type(store.state.get_tag(self.0.addr).type_addr))
    }
}
