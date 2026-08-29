use alloc::vec::Vec;

use super::*;
use crate::engine::Config;
use crate::interpreter::{InternalValue, Value32, Value64, Value128};

/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
pub(crate) struct State {
    // Concrete type indexes in store instances address this canonical type space.
    pub(crate) canonical_types: Vec<SubType>,
    pub(crate) canonical_rec_group_lengths: Vec<u32>,
    pub(crate) funcs: Functions,
    pub(crate) tables: Vec<TableInstance>,
    pub(crate) memories: Vec<MemoryInstance>,
    pub(crate) globals: Globals,
    pub(crate) tags: Vec<TagInstance>,
    pub(crate) elements: Vec<ElementInstance>,
    pub(crate) data: Vec<DataInstance>,
    pub(crate) gc: gc::GcHeap,
    pub(crate) roots: gc::Roots,
}

impl State {
    pub(crate) fn new(config: &Config) -> Self {
        Self {
            canonical_types: Vec::new(),
            canonical_rec_group_lengths: Vec::new(),
            funcs: Functions::default(),
            tables: Vec::new(),
            memories: Vec::new(),
            globals: Globals::default(),
            tags: Vec::new(),
            elements: Vec::new(),
            data: Vec::new(),
            gc: gc::GcHeap::new(config),
            roots: gc::Roots::new(),
        }
    }

    pub(crate) fn collect_gc(&mut self, additional: impl IntoIterator<Item = ValueRef>) -> Result<(), gc::AllocError> {
        let canonical_types = &self.canonical_types;
        let store_roots = self
            .globals
            .globals_32()
            .filter(|(_, ty)| Self::type_may_contain_gc_in(canonical_types, &ty.ty))
            .map(|(value, _)| ValueRef::from_raw(*value))
            .chain(
                self.tables
                    .iter()
                    .filter(|table| {
                        Self::type_may_contain_gc_in(canonical_types, &WasmType::Ref(table.kind.element_type))
                    })
                    .flat_map(|table| table.elements.iter().copied()),
            )
            .chain(
                self.elements
                    .iter()
                    .filter(|element| Self::type_may_contain_gc_in(canonical_types, &WasmType::Ref(element.ty)))
                    .flat_map(|element| element.items.iter().flatten().copied()),
            )
            .chain(additional);
        let roots = self.roots.values().chain(store_roots);
        self.gc.collect(roots)
    }

    fn type_may_contain_gc_in(types: &[SubType], ty: &WasmType) -> bool {
        let WasmType::Ref(ty) = ty else { return false };
        if let Some(type_addr) = ty.type_index() {
            return matches!(types[type_addr as usize].composite, CompositeType::Struct(_) | CompositeType::Array(_));
        }
        matches!(
            ty.abstract_heap_type(),
            Some(
                AbstractHeapType::Any
                    | AbstractHeapType::Eq
                    | AbstractHeapType::Struct
                    | AbstractHeapType::Array
                    | AbstractHeapType::Extern
                    | AbstractHeapType::Exn
            )
        )
    }

    pub(crate) fn check_gc_allocation(&self, type_addr: TypeAddr, value_count: usize) -> Result<(), Trap> {
        let trace_references = match &self.get_type(type_addr).composite {
            CompositeType::Struct(ty) => {
                ty.fields.iter().any(|field| matches!(field.storage, StorageType::Value(WasmType::Ref(_))))
            }
            CompositeType::Array(ty) => matches!(ty.field.storage, StorageType::Value(WasmType::Ref(_))),
            CompositeType::Func(_) => unreachable!("GC object type is not a function"),
        };
        self.gc.check_allocation(value_count, trace_references)
    }

    /// Allocates an object, collecting from all runtime roots when needed.
    pub(crate) fn alloc_gc_object(
        &mut self,
        type_addr: TypeAddr,
        values: Vec<RuntimeValue>,
        additional_roots: impl IntoIterator<Item = ValueRef>,
    ) -> Result<ValueRef, Trap> {
        let trace_references = match &self.get_type(type_addr).composite {
            CompositeType::Struct(ty) => {
                ty.fields.iter().any(|field| matches!(field.storage, StorageType::Value(WasmType::Ref(_))))
            }
            CompositeType::Array(ty) => matches!(ty.field.storage, StorageType::Value(WasmType::Ref(_))),
            CompositeType::Func(_) => unreachable!("GC object type is not a function"),
        };
        if self.gc.should_collect(values.len(), trace_references) {
            let roots = additional_roots.into_iter().chain(values.iter().filter_map(|value| match value {
                RuntimeValue::ValueRef(value) => Some(*value),
                _ => None,
            }));
            cold_err!(self.collect_gc(roots)).map_err(|_| Trap::OutOfMemory)?;
        }
        self.gc.alloc(type_addr, values, trace_references)
    }

    /// Allocates a traced exception object, collecting with its payload as temporary roots.
    pub(crate) fn alloc_exception(
        &mut self,
        tag_addr: TagAddr,
        payload: Vec<RuntimeValue>,
        additional_roots: impl IntoIterator<Item = ValueRef>,
    ) -> Result<ValueRef, Trap> {
        let type_addr = self.get_tag(tag_addr).type_addr;
        let mut trace_fields = Vec::new();
        trace_fields.try_reserve_exact(payload.len()).map_err(|_| Trap::OutOfMemory)?;
        trace_fields.extend(
            self.get_canonical_func_type(type_addr)
                .params()
                .iter()
                .map(|ty| Self::type_may_contain_gc_in(&self.canonical_types, ty)),
        );
        if self.gc.should_collect(payload.len(), true) {
            let roots = additional_roots.into_iter().chain(payload.iter().filter_map(|value| match value {
                RuntimeValue::ValueRef(value) => Some(*value),
                _ => None,
            }));
            cold_err!(self.collect_gc(roots)).map_err(|_| Trap::OutOfMemory)?;
        }
        self.gc.alloc_exception(tag_addr, payload, &trace_fields)
    }

    /// Resolves a non-null object of the expected canonical type.
    pub(crate) fn gc_object(&self, reference: ValueRef, expected_type: TypeAddr) -> Result<gc::Handle, Trap> {
        if reference.is_null() {
            return Err(if self.get_type(expected_type).as_array().is_some() {
                Trap::NullArrayReference
            } else {
                Trap::NullStructReference
            });
        }
        let object = self.gc.get(reference).ok_or(Trap::Other("invalid GC reference"))?;
        let gc::GcObjectKind::Composite(type_addr) = object.kind else {
            return Err(Trap::Other("GC reference is not a struct or array"));
        };
        if !self.type_addr_is_subtype(type_addr, expected_type) {
            return Err(Trap::Other("GC reference type mismatch"));
        }
        self.gc.handle(reference).ok_or(Trap::Other("invalid GC reference"))
    }

    /// Returns whether one canonical type is a subtype of another.
    pub(crate) fn type_addr_is_subtype(&self, mut actual: TypeAddr, expected: TypeAddr) -> bool {
        loop {
            if actual == expected {
                return true;
            }
            let Some(supertype) = self.get_type(actual).supertype else { return false };
            actual = supertype;
        }
    }

    /// Returns whether one reference type is a subtype of another.
    pub(crate) fn ref_type_is_subtype(&self, actual: RefType, expected: RefType) -> bool {
        if actual.is_nullable() && !expected.is_nullable() {
            return false;
        }
        self.heap_type_is_subtype(actual, expected)
    }

    /// Returns whether one value type is a subtype of another.
    pub(crate) fn value_type_is_subtype(&self, actual: WasmType, expected: WasmType) -> bool {
        match (actual, expected) {
            (WasmType::Ref(actual), WasmType::Ref(expected)) => self.ref_type_is_subtype(actual, expected),
            _ => actual == expected,
        }
    }

    fn heap_type_is_subtype(&self, actual: RefType, expected: RefType) -> bool {
        if let Some(expected_addr) = expected.type_index() {
            let Some(actual_addr) = actual.type_index() else {
                return matches!(
                    (actual.abstract_heap_type(), &self.get_type(expected_addr).composite),
                    (Some(AbstractHeapType::NoFunc), CompositeType::Func(_))
                        | (Some(AbstractHeapType::None), CompositeType::Struct(_) | CompositeType::Array(_))
                );
            };
            return self.type_addr_is_subtype(actual_addr, expected_addr);
        }

        let expected = expected.abstract_heap_type().expect("abstract reference type");
        if let Some(actual_addr) = actual.type_index() {
            return match &self.get_type(actual_addr).composite {
                CompositeType::Func(_) => expected == AbstractHeapType::Func,
                CompositeType::Struct(_) => {
                    matches!(expected, AbstractHeapType::Struct | AbstractHeapType::Eq | AbstractHeapType::Any)
                }
                CompositeType::Array(_) => {
                    matches!(expected, AbstractHeapType::Array | AbstractHeapType::Eq | AbstractHeapType::Any)
                }
            };
        }

        let actual = actual.abstract_heap_type().expect("abstract reference type");
        actual == expected
            || match actual {
                AbstractHeapType::None => matches!(
                    expected,
                    AbstractHeapType::I31
                        | AbstractHeapType::Struct
                        | AbstractHeapType::Array
                        | AbstractHeapType::Eq
                        | AbstractHeapType::Any
                ),
                AbstractHeapType::I31 | AbstractHeapType::Struct | AbstractHeapType::Array => {
                    matches!(expected, AbstractHeapType::Eq | AbstractHeapType::Any)
                }
                AbstractHeapType::Eq => expected == AbstractHeapType::Any,
                AbstractHeapType::NoFunc => expected == AbstractHeapType::Func,
                AbstractHeapType::NoExtern => expected == AbstractHeapType::Extern,
                AbstractHeapType::NoExn => expected == AbstractHeapType::Exn,
                _ => false,
            }
    }

    /// Returns whether a runtime reference has the expected type.
    pub(crate) fn value_ref_matches(&self, value: ValueRef, expected: RefType) -> bool {
        if value.is_null() {
            return expected.is_nullable();
        }
        if expected.abstract_heap_type() == Some(AbstractHeapType::Extern) {
            return true;
        }
        let expected_func = expected.type_index().is_some_and(|addr| self.get_type(addr).as_func().is_some())
            || expected.abstract_heap_type() == Some(AbstractHeapType::Func);
        if expected_func {
            let Some(func_addr) = value.addr() else { return false };
            if !self.funcs.contains(func_addr) {
                return false;
            }
            let type_addr = self.funcs.type_addr(func_addr);
            return self.ref_type_is_subtype(RefType::new_concrete(false, type_addr), expected);
        }
        if expected.abstract_heap_type() == Some(AbstractHeapType::Exn) {
            return matches!(self.gc.get(value).map(|object| object.kind), Some(gc::GcObjectKind::Exception(_)));
        }
        if value.is_i31() {
            return self.ref_type_is_subtype(RefType::new_abstract(false, AbstractHeapType::I31), expected);
        }
        if value.is_host_any() {
            return self.ref_type_is_subtype(RefType::new_abstract(false, AbstractHeapType::Any), expected);
        }

        let Some(object) = self.gc.get(value) else { return false };
        let gc::GcObjectKind::Composite(type_addr) = object.kind else { return false };
        let actual = RefType::new_concrete(false, type_addr);
        self.ref_type_is_subtype(actual, expected)
    }

    #[inline]
    pub(crate) fn get_func_type(&self, addr: FuncAddr) -> &FuncType {
        self.get_canonical_func_type(self.funcs.type_addr(addr))
    }

    #[inline]
    pub(crate) fn get_type(&self, addr: TypeAddr) -> &SubType {
        &self.canonical_types[addr as usize]
    }

    #[inline]
    pub(crate) fn get_canonical_func_type(&self, addr: TypeAddr) -> &FuncType {
        self.get_type(addr).as_func().expect("validated function address references a function type")
    }

    #[inline]
    fn get_disjoint_mut<'a, T>(items: &'a mut [T], addr: Addr, addr2: Addr, kind: &str) -> (&'a mut T, &'a mut T) {
        let [item_a, item_b] = items
            .get_disjoint_mut([addr as usize, addr2 as usize])
            .unwrap_or_else(|_| unreachable!("invalid {kind} addresses: {addr}, {addr2}"));
        (item_a, item_b)
    }

    #[inline]
    pub(crate) fn get_tag(&self, addr: TagAddr) -> &TagInstance {
        &self.tags[addr as usize]
    }

    /// Get the memory at the actual index in the store
    #[inline]
    pub(crate) fn get_mem(&self, addr: MemAddr) -> &MemoryInstance {
        &self.memories[addr as usize]
    }

    /// Get the memory at the actual index in the store
    #[inline]
    pub(crate) fn get_mem_mut(&mut self, addr: MemAddr) -> &mut MemoryInstance {
        &mut self.memories[addr as usize]
    }

    /// Get the memory at the actual index in the store
    #[inline]
    pub(crate) fn get_mems_mut(&mut self, addr: MemAddr, addr2: MemAddr) -> (&mut MemoryInstance, &mut MemoryInstance) {
        Self::get_disjoint_mut(&mut self.memories, addr, addr2, "memory")
    }

    /// Get the table at the actual index in the store
    #[inline]
    pub(crate) fn get_table(&self, addr: TableAddr) -> &TableInstance {
        &self.tables[addr as usize]
    }

    /// Get the table at the actual index in the store
    #[inline]
    pub(crate) fn get_table_mut(&mut self, addr: TableAddr) -> &mut TableInstance {
        &mut self.tables[addr as usize]
    }

    /// Get two mutable tables at the actual index in the store
    pub(crate) fn get_tables_mut(
        &mut self,
        addr: TableAddr,
        addr2: TableAddr,
    ) -> (&mut TableInstance, &mut TableInstance) {
        Self::get_disjoint_mut(&mut self.tables, addr, addr2, "table")
    }

    /// Get the data at the actual index in the store
    #[inline]
    pub(crate) fn get_data_mut(&mut self, addr: DataAddr) -> &mut DataInstance {
        &mut self.data[addr as usize]
    }

    /// Get the element at the actual index in the store
    #[inline]
    pub(crate) fn get_elem_mut(&mut self, addr: ElemAddr) -> &mut ElementInstance {
        &mut self.elements[addr as usize]
    }

    /// Returns a global in its internal value representation.
    pub(crate) fn global_value(&self, addr: GlobalAddr) -> RuntimeValue {
        let ty = self.globals.ty(addr).ty;
        match ty {
            WasmType::I32 | WasmType::F32 => RuntimeValue::Value32(Value32::global_get(&self.globals, addr)),
            WasmType::I64 | WasmType::F64 => RuntimeValue::Value64(Value64::global_get(&self.globals, addr)),
            WasmType::Ref(_) => RuntimeValue::ValueRef(ValueRef::global_get(&self.globals, addr)),
            WasmType::V128 => RuntimeValue::Value128(Value128::global_get(&self.globals, addr)),
        }
    }

    /// Sets a global from its internal value representation.
    pub(crate) fn set_global_value(&mut self, addr: GlobalAddr, value: RuntimeValue) -> Result<()> {
        let ty = self.globals.ty(addr);
        if !ty.mutable {
            cold_path();
            return Err(Error::other("global is immutable"));
        }
        match value {
            RuntimeValue::Value32(value) => Value32::global_set(&mut self.globals, addr, value),
            RuntimeValue::Value64(value) => Value64::global_set(&mut self.globals, addr, value),
            RuntimeValue::ValueRef(value) => ValueRef::global_set(&mut self.globals, addr, value),
            RuntimeValue::Value128(value) => Value128::global_set(&mut self.globals, addr, value),
        }
        Ok(())
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new(&Config::default())
    }
}
