use alloc::vec::Vec;

use super::*;

/// Global state that can be manipulated by WebAssembly programs
///
/// Data should only be addressable by the module that owns it
/// See <https://webassembly.github.io/spec/core/exec/runtime.html#store>
pub(crate) struct State {
    // Concrete type indexes in store instances address this canonical type space.
    pub(crate) canonical_types: Vec<SubType>,
    pub(crate) canonical_rec_group_lengths: Vec<u32>,
    pub(crate) funcs: Vec<FunctionInstance>,
    pub(crate) tables: Vec<TableInstance>,
    pub(crate) memories: Vec<MemoryInstance>,
    pub(crate) globals: Globals,
    pub(crate) tags: Vec<TagInstance>,
    pub(crate) exceptions: Vec<ExceptionInstance>,
    pub(crate) elements: Vec<ElementInstance>,
    pub(crate) data: Vec<DataInstance>,
    pub(crate) gc: Box<gc::GcHeap>,
}

impl State {
    pub(crate) fn new(gc_collection_threshold: usize) -> Self {
        Self {
            canonical_types: Vec::new(),
            canonical_rec_group_lengths: Vec::new(),
            funcs: Vec::new(),
            tables: Vec::new(),
            memories: Vec::new(),
            globals: Globals::default(),
            tags: Vec::new(),
            exceptions: Vec::new(),
            elements: Vec::new(),
            data: Vec::new(),
            gc: Box::new(gc::GcHeap::new(gc_collection_threshold)),
        }
    }

    /// Returns whether values of this type can contain a managed GC object.
    pub(crate) fn type_may_contain_gc(&self, ty: &WasmType) -> bool {
        Self::type_may_contain_gc_in(&self.canonical_types, ty)
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
            )
        )
    }

    /// Precomputes whether a canonical function signature can carry GC objects.
    pub(crate) fn func_gc_metadata(&self, type_addr: TypeAddr) -> FunctionGcMetadata {
        let ty = self.get_canonical_func_type(type_addr);
        FunctionGcMetadata {
            params: ty.params().iter().any(|ty| self.type_may_contain_gc(ty)),
            results: ty.results().iter().any(|ty| self.type_may_contain_gc(ty)),
        }
    }

    /// Allocates an object, collecting from all runtime roots when needed.
    pub(crate) fn alloc_gc_object(
        &mut self,
        type_addr: TypeAddr,
        values: Vec<TinyWasmValue>,
        stack_32: impl IntoIterator<Item = u32>,
    ) -> Result<ValueRef, Trap> {
        let trace_references = match &self.get_type(type_addr).composite {
            CompositeType::Struct(ty) => {
                ty.fields.iter().any(|field| matches!(field.storage, StorageType::Value(WasmType::Ref(_))))
            }
            CompositeType::Array(ty) => matches!(ty.field.storage, StorageType::Value(WasmType::Ref(_))),
            CompositeType::Func(_) => unreachable!("GC object type is not a function"),
        };
        if self.gc.should_collect(values.len(), trace_references) {
            let canonical_types = &self.canonical_types;
            let roots = stack_32
                .into_iter()
                .map(ValueRef::from_raw)
                .chain(
                    self.globals
                        .globals_32()
                        .filter(|(_, ty)| Self::type_may_contain_gc_in(canonical_types, &ty.ty))
                        .map(|(value, _)| ValueRef::from_raw(*value)),
                )
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
                .chain(self.exceptions.iter().flat_map(|exception| {
                    exception.payload.iter().filter_map(|value| match value {
                        TinyWasmValue::ValueRef(value) => Some(*value),
                        _ => None,
                    })
                }))
                .chain(values.iter().filter_map(|value| match value {
                    TinyWasmValue::ValueRef(value) => Some(*value),
                    _ => None,
                }));
            cold_err!(self.gc.collect(roots)).map_err(|_| Trap::OutOfMemory)?;
        }
        cold_err!(self.gc.alloc(type_addr, values, trace_references)).map_err(|_| Trap::OutOfMemory)
    }

    /// Pins a host-visible reference when it resolves to a managed GC object.
    pub(crate) fn pin_host_ref(&self, value: RefValue) {
        let raw = match value {
            RefValue::Any(value) => value.raw(),
            RefValue::Extern(value) => value.raw(),
            RefValue::Null | RefValue::Func(_) | RefValue::Exn(_) => return,
        };
        self.gc.pin(ValueRef::from_raw(raw));
    }

    /// Pins GC references that have crossed into host-visible values.
    pub(crate) fn pin_host_values(&self, values: &[WasmValue]) {
        for &value in values {
            if let WasmValue::Ref(value) = value {
                self.pin_host_ref(value);
            }
        }
    }

    /// Resolves a non-null object of the expected canonical type.
    pub(crate) fn gc_object(&self, reference: ValueRef, expected_type: TypeAddr) -> Result<&gc::GcObject, Trap> {
        if reference.is_null() {
            return Err(if self.get_type(expected_type).as_array().is_some() {
                Trap::NullArrayReference
            } else {
                Trap::NullStructReference
            });
        }
        let object = self.gc.get(reference).ok_or(Trap::Other("invalid GC reference"))?;
        if !self.type_addr_is_subtype(object.type_addr, expected_type) {
            return Err(Trap::Other("GC reference type mismatch"));
        }
        Ok(object)
    }

    /// Converts an internal reference using canonical heap type information.
    pub(crate) fn to_ref_value(&self, value: ValueRef, ty: RefType) -> RefValue {
        if value.is_null() {
            return RefValue::Null;
        }

        if let Some(type_addr) = ty.type_index() {
            return match &self.get_type(type_addr).composite {
                CompositeType::Func(_) => {
                    RefValue::Func(FuncRef::new(value.addr().expect("non-null reference has an address")))
                }
                CompositeType::Struct(_) | CompositeType::Array(_) => RefValue::Any(AnyRef::from_raw(value.raw())),
            };
        }
        if ty.is_func() {
            return RefValue::Func(FuncRef::new(value.addr().expect("non-null reference has an address")));
        }
        if ty.is_extern() {
            return RefValue::Extern(ExternRef::from_raw(value.raw()));
        }
        if ty.is_exn() {
            return RefValue::Exn(ExnRef::new(value.addr().expect("non-null reference has an address")));
        }
        RefValue::Any(AnyRef::from_raw(value.raw()))
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
            let Some(func) = self.funcs.get(func_addr as usize) else { return false };
            return self.ref_type_is_subtype(RefType::new_concrete(false, func.type_addr), expected);
        }
        if expected.abstract_heap_type() == Some(AbstractHeapType::Exn) {
            return value.addr().is_some_and(|addr| self.exceptions.get(addr as usize).is_some());
        }
        if value.is_i31() {
            return self.ref_type_is_subtype(RefType::new_abstract(false, AbstractHeapType::I31), expected);
        }
        if value.is_host_any() {
            return self.ref_type_is_subtype(RefType::new_abstract(false, AbstractHeapType::Any), expected);
        }

        let Some(object) = self.gc.get(value) else { return false };
        let actual = RefType::new_concrete(false, object.type_addr);
        self.ref_type_is_subtype(actual, expected)
    }

    #[inline]
    pub(crate) fn get_func_type(&self, addr: FuncAddr) -> &FuncType {
        self.get_canonical_func_type(self.get_func(addr).type_addr)
    }

    #[inline]
    pub(crate) fn get_type(&self, addr: TypeAddr) -> &SubType {
        Self::get(&self.canonical_types, addr, "canonical type")
    }

    #[inline]
    pub(crate) fn get_canonical_func_type(&self, addr: TypeAddr) -> &FuncType {
        self.get_type(addr).as_func().expect("validated function address references a function type")
    }

    pub(crate) fn value_matches_type(&self, value: WasmValue, expected: WasmType) -> bool {
        match (value, expected) {
            (WasmValue::Ref(RefValue::Null), WasmType::Ref(expected)) => expected.is_nullable(),
            (WasmValue::Ref(RefValue::Func(func)), WasmType::Ref(expected)) => self
                .funcs
                .get(func.addr() as usize)
                .is_some_and(|func| self.ref_type_is_subtype(RefType::new_concrete(false, func.type_addr), expected)),
            (WasmValue::Ref(RefValue::Any(_)), WasmType::Ref(expected))
                if expected.is_func() || expected.is_extern() || expected.is_exn() =>
            {
                false
            }
            (WasmValue::Ref(RefValue::Exn(value)), WasmType::Ref(expected)) => {
                expected.abstract_heap_type() == Some(AbstractHeapType::Exn)
                    && self.exceptions.get(value.addr() as usize).is_some()
            }
            (WasmValue::Ref(RefValue::Any(value)), WasmType::Ref(expected)) => {
                self.value_ref_matches(ValueRef::from_raw(value.raw()), expected)
            }
            (_, WasmType::Ref(expected)) if expected.is_concrete() => false,
            _ => value.matches_type(expected),
        }
    }

    pub(super) fn get<'a, T>(items: &'a [T], addr: Addr, kind: &str) -> &'a T {
        items.get(addr as usize).unwrap_or_else(|| unreachable!("invalid {kind} address: {addr}"))
    }

    fn get_mut<'a, T>(items: &'a mut [T], addr: Addr, kind: &str) -> &'a mut T {
        items.get_mut(addr as usize).unwrap_or_else(|| unreachable!("invalid {kind} address: {addr}"))
    }

    fn get_disjoint_mut<'a, T>(items: &'a mut [T], addr: Addr, addr2: Addr, kind: &str) -> (&'a mut T, &'a mut T) {
        let [item_a, item_b] = items
            .get_disjoint_mut([addr as usize, addr2 as usize])
            .unwrap_or_else(|_| unreachable!("invalid {kind} addresses: {addr}, {addr2}"));
        (item_a, item_b)
    }

    /// Get the function at the actual index in the store
    pub(crate) fn get_func(&self, addr: FuncAddr) -> &FunctionInstance {
        Self::get(&self.funcs, addr, "function")
    }

    pub(crate) fn get_tag(&self, addr: TagAddr) -> &TagInstance {
        Self::get(&self.tags, addr, "tag")
    }

    /// Get a wasm function at the actual index in the store, panicking if it's a host function (which should be guaranteed by the validator)
    pub(crate) fn get_wasm_func(&self, addr: FuncAddr) -> &WasmFunctionInstance {
        match self.funcs.get(addr as usize).map(|func| &func.kind) {
            Some(FunctionKind::Wasm(wasm_func)) => wasm_func,
            _ => unreachable!("invalid wasm function address: {addr}"),
        }
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem(&self, addr: MemAddr) -> &MemoryInstance {
        Self::get(&self.memories, addr, "memory")
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mem_mut(&mut self, addr: MemAddr) -> &mut MemoryInstance {
        Self::get_mut(&mut self.memories, addr, "memory")
    }

    /// Get the memory at the actual index in the store
    pub(crate) fn get_mems_mut(&mut self, addr: MemAddr, addr2: MemAddr) -> (&mut MemoryInstance, &mut MemoryInstance) {
        Self::get_disjoint_mut(&mut self.memories, addr, addr2, "memory")
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table(&self, addr: TableAddr) -> &TableInstance {
        Self::get(&self.tables, addr, "table")
    }

    /// Get the table at the actual index in the store
    pub(crate) fn get_table_mut(&mut self, addr: TableAddr) -> &mut TableInstance {
        Self::get_mut(&mut self.tables, addr, "table")
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
    pub(crate) fn get_data_mut(&mut self, addr: DataAddr) -> &mut DataInstance {
        Self::get_mut(&mut self.data, addr, "data")
    }

    /// Get the element at the actual index in the store
    pub(crate) fn get_elem_mut(&mut self, addr: ElemAddr) -> &mut ElementInstance {
        Self::get_mut(&mut self.elements, addr, "element")
    }

    /// Converts a global directly to its public value representation.
    pub(crate) fn get_global_wasmvalue(&self, addr: GlobalAddr) -> WasmValue {
        let ty = self.globals.ty(addr).ty;
        match ty {
            WasmType::I32 => WasmValue::I32(self.globals.get_32(addr) as i32),
            WasmType::I64 => WasmValue::I64(self.globals.get_64(addr) as i64),
            WasmType::F32 => WasmValue::F32(f32::from_bits(self.globals.get_32(addr))),
            WasmType::F64 => WasmValue::F64(f64::from_bits(self.globals.get_64(addr))),
            WasmType::Ref(ty) => WasmValue::Ref(self.to_ref_value(ValueRef::from_raw(self.globals.get_32(addr)), ty)),
            WasmType::V128 => WasmValue::V128(self.globals.get_128(addr).to_le_bytes()),
        }
    }

    /// Validates and sets a global from its public value representation.
    pub(crate) fn set_global_wasmvalue(&mut self, addr: GlobalAddr, value: WasmValue) -> Result<()> {
        let ty = self.globals.ty(addr);
        if !ty.mutable {
            cold_path();
            return Err(Error::other("global is immutable"));
        }
        if !self.value_matches_type(value, ty.ty) {
            cold_path();
            return Err(Error::other("invalid global value type"));
        }
        match value {
            WasmValue::I32(value) => self.globals.set_32(addr, value as u32),
            WasmValue::I64(value) => self.globals.set_64(addr, value as u64),
            WasmValue::F32(value) => self.globals.set_32(addr, value.to_bits()),
            WasmValue::F64(value) => self.globals.set_64(addr, value.to_bits()),
            WasmValue::Ref(value) => self.globals.set_32(addr, ValueRef::from(value).raw()),
            WasmValue::V128(value) => self.globals.set_128(addr, value.into()),
        }
        Ok(())
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new(1024 * 1024)
    }
}
