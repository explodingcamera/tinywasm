use crate::{
    IndexKind, ParseLimitKind, ParserOptions, Result, TypeKind,
    conversion::{FunctionLoweringContext, convert_heap_type, value_lane},
    emitter::{Emitter, LabelId},
    macros::visit::*,
};
use alloc::{
    boxed::Box,
    collections::{BTreeMap, btree_map::Entry},
    vec::Vec,
};
use tinywasm_types::{
    AtomicWaitOp, BinOp, CmpOp, ExceptionHandler, Global, Import, ImportKind, Instruction, InstructionEffects,
    InstructionValueEffect, LocalDestination, MemoryOperand, MemoryType, Operand64, Operand64Idx, Operand128,
    Operand128Idx, StorageType, TableDefinition, TagType, TypeSection, ValueCounts, ValueLane, WasmFunctionData,
};
use wasmparser::{FunctionBody, OperatorsReader, OperatorsReaderAllocations, VisitSimdOperator};

#[cfg(feature = "validate")]
use wasmparser::{FuncValidator, FuncValidatorAllocations, ValidatorResources, VisitOperator};

#[derive(Debug, Clone, Copy)]
enum BlockKind {
    Function,
    Block,
    Loop,
    If,
    TryTable,
}

struct ControlFrame<'a> {
    kind: BlockKind,
    has_else: bool,
    loop_start: Option<LabelId>,
    end: Option<LabelId>,
    else_entry: Option<LabelId>,
    height: usize,
    base: ValueCounts,
    signature: BlockSignature<'a>,
    unreachable: bool,
    entry_unreachable: bool,
    end_reachable: bool,
}

struct BranchDestination {
    frame: usize,
    shape: Option<(ValueCounts, ValueCounts)>,
}

/// Module-owned physical lane signature, borrowed by control frames.
pub(crate) struct Signature {
    pub params: Vec<ValueLane>,
    results: Vec<ValueLane>,
}

#[derive(Clone, Copy)]
enum BlockSignature<'a> {
    Empty,
    Result(ValueLane),
    Function(&'a Signature),
}

impl BlockSignature<'_> {
    fn params(&self) -> &[ValueLane] {
        match self {
            Self::Function(signature) => &signature.params,
            _ => &[],
        }
    }

    fn results(&self) -> &[ValueLane] {
        match self {
            Self::Empty => &[],
            Self::Result(lane) => core::slice::from_ref(lane),
            Self::Function(signature) => &signature.results,
        }
    }
}

pub(crate) struct ModuleMetadata {
    signatures: Vec<Option<Signature>>,
    functions: Vec<u32>,
    globals: Vec<ValueLane>,
    memories: Vec<ValueLane>,
    tables: Vec<ValueLane>,
    tags: Vec<u32>,
    aggregate_fields: Vec<AggregateFields>,
    imported_memories: u32,
}

pub(crate) struct FunctionDataBuilder {
    pub(crate) operands64: Vec<Operand64>,
    pub(crate) operands128: Vec<Operand128>,
    pub(crate) branch_table_targets: Vec<u32>,
    pub(crate) exception_handlers: Vec<ExceptionHandler>,
    deduplicate64: Option<BTreeMap<Operand64, u32>>,
    deduplicate128: Option<BTreeMap<Operand128, u32>>,
}

impl FunctionDataBuilder {
    /// Creates empty operand pools with optional deduplication.
    pub(crate) fn new(deduplicate_operands: bool) -> Self {
        Self {
            operands64: Vec::new(),
            operands128: Vec::new(),
            branch_table_targets: Vec::new(),
            exception_handlers: Vec::new(),
            deduplicate64: deduplicate_operands.then(BTreeMap::new),
            deduplicate128: deduplicate_operands.then(BTreeMap::new),
        }
    }

    /// Allocates an immutable 64-bit operand, reusing it when deduplication is enabled.
    pub(crate) fn push64<T>(&mut self, operand: Operand64<T>) -> Result<Operand64Idx<T>> {
        push_operand(&mut self.operands64, self.deduplicate64.as_mut(), operand.cast()).map(Operand64Idx::new)
    }

    /// Allocates an immutable 128-bit operand, reusing it when deduplication is enabled.
    pub(crate) fn push128<T>(&mut self, operand: Operand128<T>) -> Result<Operand128Idx<T>> {
        push_operand(&mut self.operands128, self.deduplicate128.as_mut(), operand.cast()).map(Operand128Idx::new)
    }

    /// Allocates a private mutable target operand outside immutable deduplication.
    pub(crate) fn push_target64<T>(&mut self, operand: Operand64<T>) -> Result<Operand64Idx<T>> {
        push_operand(&mut self.operands64, None, operand.cast()).map(Operand64Idx::new)
    }

    /// Allocates a private mutable target operand outside immutable deduplication.
    pub(crate) fn push_target128<T>(&mut self, operand: Operand128<T>) -> Result<Operand128Idx<T>> {
        push_operand(&mut self.operands128, None, operand.cast()).map(Operand128Idx::new)
    }

    /// Reads a packed operand from the 64-bit pool.
    pub(crate) fn operand64<T>(&self, index: Operand64Idx<T>) -> Operand64<T> {
        self.operands64[index.index()].cast()
    }

    /// Reads a packed operand from the 128-bit pool.
    pub(crate) fn operand128<T>(&self, index: Operand128Idx<T>) -> Operand128<T> {
        self.operands128[index.index()].cast()
    }

    /// Resolves an instruction's declared inline or pooled destination local.
    pub(crate) fn local_destination(&self, destination: LocalDestination) -> u16 {
        match destination {
            LocalDestination::Inline(local) => local,
            LocalDestination::Third64(index) => self.operand64(index).c(),
            LocalDestination::Second64(index) => self.operand64(index).b(),
            LocalDestination::Second128(index) => self.operand128(index).b(),
            LocalDestination::Third128(index) => self.operand128(index).c(),
            LocalDestination::Third128Wide(index) => self.operand128(index).c(),
        }
    }

    /// Checks whether an instruction might overwrite a local.
    pub(crate) fn may_write_local(&self, instruction: Instruction, lane: ValueLane, local: u16) -> bool {
        match instruction.value_effect() {
            InstructionValueEffect::Unknown => true,
            InstructionValueEffect::LocalStore(written_lane, written_local) => {
                lane == written_lane && local == written_local
            }
            InstructionValueEffect::LocalWrite(written_lane, destination) => {
                lane == written_lane && local == self.local_destination(destination)
            }
            InstructionValueEffect::LocalResult(destination) => {
                matches!(instruction.effects(), InstructionEffects::Operands { output: Some(output), .. } if output == lane)
                    && local == self.local_destination(destination)
            }
            _ => false,
        }
    }

    /// Finalizes the operand pools, branch tables, and exception handlers.
    pub(crate) fn finish(self) -> WasmFunctionData {
        WasmFunctionData {
            operands64: self.operands64.into_boxed_slice(),
            operands128: self.operands128.into_boxed_slice(),
            branch_table_targets: self.branch_table_targets.into_boxed_slice(),
            exception_handlers: self.exception_handlers.into_boxed_slice(),
        }
    }
}

// Erase layout tags before allocation so typed callers share one body per pool width.
#[inline(never)]
fn push_operand<T: Copy + Ord>(
    operands: &mut Vec<T>,
    deduplicate: Option<&mut BTreeMap<T, u32>>,
    operand: T,
) -> Result<u32> {
    let entry = match deduplicate.map(|map| map.entry(operand)) {
        Some(Entry::Occupied(entry)) => return Ok(*entry.get()),
        Some(Entry::Vacant(entry)) => Some(entry),
        None => None,
    };
    let index =
        u32::try_from(operands.len()).map_err(|_| crate::ParseError::Other("operand pool is too large".into()))?;
    operands.push(operand);
    if let Some(entry) = entry {
        entry.insert(index);
    }
    Ok(index)
}

enum AggregateFields {
    Other,
    Struct(Box<[ValueLane]>),
    Array(ValueLane),
}

pub(crate) struct FunctionBuilder<'a> {
    emitter: Emitter,
    control_stack: Vec<ControlFrame<'a>>,
    operand_stack: Vec<ValueLane>,
    lane_counts: ValueCounts,
    max_lane_counts: ValueCounts,
    metadata: &'a ModuleMetadata,
    locals: Vec<(ValueLane, u16)>,
    uses_local_memory: bool,
}

impl<'a> FunctionBuilder<'a> {
    /// Creates lowering state with a borrowed function signature.
    pub(crate) fn new(
        metadata: &'a ModuleMetadata,
        signature: &'a Signature,
        locals: Vec<(ValueLane, u16)>,
        body_size: usize,
        context: FunctionLoweringContext,
        options: &ParserOptions,
    ) -> Self {
        Self {
            locals,
            metadata,
            emitter: Emitter::new(body_size, context, options),
            control_stack: alloc::vec![ControlFrame {
                kind: BlockKind::Function,
                has_else: false,
                loop_start: None,
                end: None,
                else_entry: None,
                height: 0,
                base: ValueCounts::default(),
                signature: BlockSignature::Function(signature),
                unreachable: false,
                entry_unreachable: false,
                end_reachable: false,
            }],
            operand_stack: Vec::new(),
            lane_counts: ValueCounts::default(),
            max_lane_counts: ValueCounts::default(),
            uses_local_memory: false,
        }
    }

    fn mark_memory(&mut self, memory: u32) {
        self.uses_local_memory |= memory >= self.metadata.imported_memories;
    }

    fn memory_address_lane(&mut self, memory: u32) -> Result<ValueLane> {
        let lane = self.metadata.memory_address_lane(memory)?;
        self.mark_memory(memory);
        Ok(lane)
    }

    fn memory_operand(&mut self, memarg: wasmparser::MemArg) -> Result<(ValueLane, Operand128Idx<MemoryOperand>)> {
        let wasmparser::MemArgTarget::Memory(memory) = memarg.target else {
            return Err(crate::ParseError::UnsupportedOperator("multibyte array access"));
        };
        let address = self.memory_address_lane(memory)?;
        let operand = self.push128(Operand128::<MemoryOperand>::new(memarg.offset, memory))?;
        Ok((address, operand))
    }

    fn atomic_wait(&mut self, memarg: wasmparser::MemArg, width: u8, op: AtomicWaitOp) -> Result<()> {
        if memarg.align != width.trailing_zeros() as u8 {
            return Err(crate::ParseError::Other("invalid atomic alignment".into()));
        }
        let (address, memory) = self.memory_operand(memarg)?;
        let inputs: &[ValueLane] = match op {
            AtomicWaitOp::Notify => &[address, ValueLane::S32],
            AtomicWaitOp::Wait32 => &[address, ValueLane::S32, ValueLane::S64],
            AtomicWaitOp::Wait64 => &[address, ValueLane::S64, ValueLane::S64],
        };
        self.emit(inputs, &[ValueLane::S32], Instruction::AtomicWait(memory, op))
    }

    fn visit_struct_get_impl(
        &mut self,
        type_index: u32,
        field_index: u32,
        instruction: fn(Operand64Idx<(u32, u32)>) -> Instruction,
    ) -> Result<()> {
        let size = self.metadata.struct_field(type_index, field_index)?;
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, field_index))?;
        self.emit(&[ValueLane::S32], &[size], instruction(operand))
    }

    fn push64<T>(&mut self, operand: Operand64<T>) -> Result<Operand64Idx<T>> {
        self.emitter.data.push64(operand)
    }

    fn push128<T>(&mut self, operand: Operand128<T>) -> Result<Operand128Idx<T>> {
        self.emitter.data.push128(operand)
    }
}

#[cfg(feature = "validate")]
struct ValidateThenVisit<'a, 'm> {
    validator: &'a mut FuncValidator<ValidatorResources>,
    builder: &'a mut FunctionBuilder<'m>,
    position: u64,
}

impl ModuleMetadata {
    fn address_lane(arch: tinywasm_types::MemoryArch) -> ValueLane {
        match arch {
            tinywasm_types::MemoryArch::I32 => ValueLane::S32,
            tinywasm_types::MemoryArch::I64 => ValueLane::S64,
        }
    }

    /// Collects the types and operand lanes needed for function lowering.
    pub(crate) fn new(
        types: &TypeSection,
        code_type_addrs: &[u32],
        imports: &[Import],
        globals: &[Global],
        memories: &[MemoryType],
        tables: &[TableDefinition],
        tags: &[TagType],
    ) -> Self {
        let mut functions = Vec::with_capacity(imports.len() + code_type_addrs.len());
        let mut global_sizes = Vec::with_capacity(imports.len() + globals.len());
        let mut memory_sizes = Vec::with_capacity(imports.len() + memories.len());
        let mut table_sizes = Vec::with_capacity(imports.len() + tables.len());
        let mut tag_types = Vec::with_capacity(imports.len() + tags.len());
        let imported_memories =
            imports.iter().filter(|import| matches!(import.kind, ImportKind::Memory(_))).count() as u32;

        for import in imports {
            match &import.kind {
                ImportKind::Function(ty) => functions.push(*ty),
                ImportKind::Global(ty) => global_sizes.push(ValueLane::from(&ty.ty)),
                ImportKind::Memory(ty) => memory_sizes.push(Self::address_lane(ty.arch())),
                ImportKind::Table(ty) => table_sizes.push(Self::address_lane(ty.arch())),
                ImportKind::Tag(ty) => tag_types.push(ty.type_idx),
            }
        }

        functions.extend_from_slice(code_type_addrs);
        global_sizes.extend(globals.iter().map(|global| ValueLane::from(&global.ty.ty)));
        memory_sizes.extend(memories.iter().map(|ty| Self::address_lane(ty.arch())));
        table_sizes.extend(tables.iter().map(|table| Self::address_lane(table.ty.arch())));
        tag_types.extend(tags.iter().map(|tag| tag.type_idx));

        let signatures = types
            .types
            .iter()
            .map(|ty| {
                ty.as_func().map(|ty| Signature {
                    params: ty.params().iter().map(ValueLane::from).collect(),
                    results: ty.results().iter().map(ValueLane::from).collect(),
                })
            })
            .collect();
        let aggregate_fields = types
            .types
            .iter()
            .map(|ty| {
                if let Some(ty) = ty.as_struct() {
                    AggregateFields::Struct(ty.fields.iter().map(|field| Self::storage_size(field.storage)).collect())
                } else if let Some(ty) = ty.as_array() {
                    AggregateFields::Array(Self::storage_size(ty.field.storage))
                } else {
                    AggregateFields::Other
                }
            })
            .collect();
        Self {
            signatures,
            functions,
            globals: global_sizes,
            memories: memory_sizes,
            tables: table_sizes,
            tags: tag_types,
            aggregate_fields,
            imported_memories,
        }
    }

    /// Looks up a function signature by type index.
    pub(crate) fn signature(&self, idx: u32) -> Result<&Signature> {
        self.signatures
            .get(idx as usize)
            .and_then(Option::as_ref)
            .ok_or(crate::ParseError::InvalidTypeReference { expected: TypeKind::Function, index: idx })
    }

    fn function_signature(&self, idx: u32) -> Result<&Signature> {
        let ty = *self
            .functions
            .get(idx as usize)
            .ok_or(crate::ParseError::IndexOutOfBounds { kind: IndexKind::Function, index: idx })?;
        self.signature(ty)
    }

    fn tag_signature(&self, idx: u32) -> Result<&Signature> {
        let ty = *self
            .tags
            .get(idx as usize)
            .ok_or(crate::ParseError::IndexOutOfBounds { kind: IndexKind::Tag, index: idx })?;
        self.signature(ty)
    }

    fn global_value_lane(&self, idx: u32) -> Result<ValueLane> {
        Self::indexed_lane(&self.globals, IndexKind::Global, idx)
    }

    fn memory_address_lane(&self, idx: u32) -> Result<ValueLane> {
        Self::indexed_lane(&self.memories, IndexKind::Memory, idx)
    }

    fn table_index_lane(&self, idx: u32) -> Result<ValueLane> {
        Self::indexed_lane(&self.tables, IndexKind::Table, idx)
    }

    fn indexed_lane(sizes: &[ValueLane], entity: IndexKind, idx: u32) -> Result<ValueLane> {
        sizes.get(idx as usize).copied().ok_or(crate::ParseError::IndexOutOfBounds { kind: entity, index: idx })
    }

    fn storage_size(storage: StorageType) -> ValueLane {
        match storage {
            StorageType::I8 | StorageType::I16 => ValueLane::S32,
            StorageType::Value(ref ty) => ValueLane::from(ty),
        }
    }

    fn struct_fields(&self, idx: u32) -> Result<&[ValueLane]> {
        self.aggregate_fields
            .get(idx as usize)
            .and_then(|fields| match fields {
                AggregateFields::Struct(fields) => Some(fields.as_ref()),
                AggregateFields::Other | AggregateFields::Array(_) => None,
            })
            .ok_or(crate::ParseError::InvalidTypeReference { expected: TypeKind::Struct, index: idx })
    }

    fn struct_field(&self, type_index: u32, field_index: u32) -> Result<ValueLane> {
        self.struct_fields(type_index)?
            .get(field_index as usize)
            .copied()
            .ok_or(crate::ParseError::IndexOutOfBounds { kind: IndexKind::StructField, index: field_index })
    }

    fn array_element_lane(&self, idx: u32) -> Result<ValueLane> {
        self.aggregate_fields
            .get(idx as usize)
            .and_then(|fields| match fields {
                AggregateFields::Array(field) => Some(*field),
                AggregateFields::Other | AggregateFields::Struct(_) => None,
            })
            .ok_or(crate::ParseError::InvalidTypeReference { expected: TypeKind::Array, index: idx })
    }
}

#[cfg(feature = "validate")]
impl<'a> VisitOperator<'a> for ValidateThenVisit<'_, '_> {
    type Output = Result<()>;

    #[cfg(not(rust_analyzer))] // rust analyzer gets confused and throws a bunch of errors when this macro is expanded
    wasmparser::for_each_operator!(visit_all_operators);

    #[cfg(not(rust_analyzer))] // rust analyzer gets confused and throws a bunch of errors when this macro is expanded
    wasmparser::for_each_visit_operator!(validate_then_visit);

    fn simd_visitor(&mut self) -> Option<&mut dyn VisitSimdOperator<'a, Output = Result<()>>> {
        Some(self)
    }
}

#[cfg(feature = "validate")]
impl VisitSimdOperator<'_> for ValidateThenVisit<'_, '_> {
    #[cfg(not(rust_analyzer))] // rust analyzer gets confused and throws a bunch of errors when this macro is expanded
    wasmparser::for_each_visit_simd_operator!(validate_then_visit_simd);
}

/// Lowers a function body without running the Wasm validator.
pub(crate) fn process_operators(
    body: FunctionBody<'_>,
    locals: Vec<(ValueLane, u16)>,
    metadata: &ModuleMetadata,
    context: FunctionLoweringContext,
    allocs: OperatorsReaderAllocations,
    options: &ParserOptions,
) -> Result<(Vec<Instruction>, FunctionDataBuilder, bool, ValueCounts, OperatorsReaderAllocations)> {
    let body_size = body.as_bytes().len();
    let reader = body.get_binary_reader_for_operators()?;
    let mut reader = OperatorsReader::new_with_allocs(reader, allocs);
    let signature = metadata.signature(context.ty_idx)?;
    let mut builder = FunctionBuilder::new(metadata, signature, locals, body_size, context, options);

    while !reader.eof() {
        let res =
            if options.limits.max_br_table_targets.is_some() || options.limits.max_array_new_fixed_elements.is_some() {
                let op = reader.read()?;
                match &op {
                    wasmparser::Operator::BrTable { targets } => {
                        options.limits.check(ParseLimitKind::BrTableTargets, targets.len() as usize)?;
                    }
                    wasmparser::Operator::ArrayNewFixed { array_size, .. } => {
                        options.limits.check(ParseLimitKind::ArrayNewFixedElements, *array_size as usize)?;
                    }
                    _ => {}
                }
                wasmparser::VisitOperator::visit_operator(&mut builder, &op)
            } else {
                reader.visit_operator(&mut builder)?
            };

        if let Err(e) = res {
            core::hint::cold_path();
            return Err(e);
        }
    }

    reader.finish()?;
    let (instructions, data) = builder.emitter.finish()?;
    Ok((instructions, data, builder.uses_local_memory, builder.max_lane_counts, reader.into_allocations()))
}

/// Validates and lowers a function body in one pass.
#[cfg(feature = "validate")]
pub(crate) fn process_operators_and_validate(
    mut validator: FuncValidator<ValidatorResources>,
    body: FunctionBody<'_>,
    locals: Vec<(ValueLane, u16)>,
    metadata: &ModuleMetadata,
    context: FunctionLoweringContext,
    allocs: OperatorsReaderAllocations,
    options: &ParserOptions,
) -> Result<(
    Vec<Instruction>,
    FunctionDataBuilder,
    bool,
    ValueCounts,
    FuncValidatorAllocations,
    OperatorsReaderAllocations,
)> {
    let body_size = body.as_bytes().len();
    let reader = body.get_binary_reader_for_operators()?;
    let mut reader = OperatorsReader::new_with_allocs(reader, allocs);
    let signature = metadata.signature(context.ty_idx)?;
    let mut builder = FunctionBuilder::new(metadata, signature, locals, body_size, context, options);

    while !reader.eof() {
        let position = reader.original_position();
        let mut visitor = ValidateThenVisit { validator: &mut validator, builder: &mut builder, position };
        let res =
            if options.limits.max_br_table_targets.is_some() || options.limits.max_array_new_fixed_elements.is_some() {
                let op = reader.read()?;
                match &op {
                    wasmparser::Operator::BrTable { targets } => {
                        options.limits.check(ParseLimitKind::BrTableTargets, targets.len() as usize)?;
                    }
                    wasmparser::Operator::ArrayNewFixed { array_size, .. } => {
                        options.limits.check(ParseLimitKind::ArrayNewFixedElements, *array_size as usize)?;
                    }
                    _ => {}
                }
                visitor.visit_operator(&op)
            } else {
                reader.visit_operator(&mut visitor)?
            };

        if let Err(e) = res {
            core::hint::cold_path();
            return Err(e);
        }
    }

    reader.finish()?;
    let (instructions, data) = builder.emitter.finish()?;
    Ok((
        instructions,
        data,
        builder.uses_local_memory,
        builder.max_lane_counts,
        validator.into_allocations(),
        reader.into_allocations(),
    ))
}

impl<'a> wasmparser::VisitOperator<'a> for FunctionBuilder<'_> {
    type Output = Result<()>;

    #[cfg(not(rust_analyzer))] // rust analyzer gets confused and throws a bunch of errors when this macro is expanded
    wasmparser::for_each_operator!(visit_all_operators);

    fn simd_visitor(&mut self) -> Option<&mut dyn VisitSimdOperator<'a, Output = Result<()>>> {
        Some(self)
    }

    wasmparser::for_each_visit_operator!(impl_visit_operator);

    fn visit_atomic_fence(&mut self, _ordering: wasmparser::Ordering) -> Self::Output {
        self.emit(&[], &[], Instruction::AtomicFence)
    }

    fn visit_memory_atomic_notify(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
        self.atomic_wait(memarg, 4, AtomicWaitOp::Notify)
    }
    fn visit_memory_atomic_wait32(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
        self.atomic_wait(memarg, 4, AtomicWaitOp::Wait32)
    }
    fn visit_memory_atomic_wait64(&mut self, memarg: wasmparser::MemArg) -> Self::Output {
        self.atomic_wait(memarg, 8, AtomicWaitOp::Wait64)
    }

    lowering_ops! {
        atomic Load [Addr] => [S32] {
            visit_i32_atomic_load => 4, visit_i32_atomic_load8_u => 1, visit_i32_atomic_load16_u => 2,
        }
        atomic Load [Addr] => [S64] {
            visit_i64_atomic_load => 8, visit_i64_atomic_load8_u => 1,
            visit_i64_atomic_load16_u => 2, visit_i64_atomic_load32_u => 4,
        }
        atomic Store [Addr, S32] => [] {
            visit_i32_atomic_store => 4, visit_i32_atomic_store8 => 1, visit_i32_atomic_store16 => 2,
        }
        atomic Store [Addr, S64] => [] {
            visit_i64_atomic_store => 8, visit_i64_atomic_store8 => 1,
            visit_i64_atomic_store16 => 2, visit_i64_atomic_store32 => 4,
        }
        atomic Add [Addr, S32] => [S32] {
            visit_i32_atomic_rmw_add => 4, visit_i32_atomic_rmw8_add_u => 1, visit_i32_atomic_rmw16_add_u => 2,
        }
        atomic Add [Addr, S64] => [S64] {
            visit_i64_atomic_rmw_add => 8, visit_i64_atomic_rmw8_add_u => 1,
            visit_i64_atomic_rmw16_add_u => 2, visit_i64_atomic_rmw32_add_u => 4,
        }
        atomic Sub [Addr, S32] => [S32] {
            visit_i32_atomic_rmw_sub => 4, visit_i32_atomic_rmw8_sub_u => 1, visit_i32_atomic_rmw16_sub_u => 2,
        }
        atomic Sub [Addr, S64] => [S64] {
            visit_i64_atomic_rmw_sub => 8, visit_i64_atomic_rmw8_sub_u => 1,
            visit_i64_atomic_rmw16_sub_u => 2, visit_i64_atomic_rmw32_sub_u => 4,
        }
        atomic And [Addr, S32] => [S32] {
            visit_i32_atomic_rmw_and => 4, visit_i32_atomic_rmw8_and_u => 1, visit_i32_atomic_rmw16_and_u => 2,
        }
        atomic And [Addr, S64] => [S64] {
            visit_i64_atomic_rmw_and => 8, visit_i64_atomic_rmw8_and_u => 1,
            visit_i64_atomic_rmw16_and_u => 2, visit_i64_atomic_rmw32_and_u => 4,
        }
        atomic Or [Addr, S32] => [S32] {
            visit_i32_atomic_rmw_or => 4, visit_i32_atomic_rmw8_or_u => 1, visit_i32_atomic_rmw16_or_u => 2,
        }
        atomic Or [Addr, S64] => [S64] {
            visit_i64_atomic_rmw_or => 8, visit_i64_atomic_rmw8_or_u => 1,
            visit_i64_atomic_rmw16_or_u => 2, visit_i64_atomic_rmw32_or_u => 4,
        }
        atomic Xor [Addr, S32] => [S32] {
            visit_i32_atomic_rmw_xor => 4, visit_i32_atomic_rmw8_xor_u => 1, visit_i32_atomic_rmw16_xor_u => 2,
        }
        atomic Xor [Addr, S64] => [S64] {
            visit_i64_atomic_rmw_xor => 8, visit_i64_atomic_rmw8_xor_u => 1,
            visit_i64_atomic_rmw16_xor_u => 2, visit_i64_atomic_rmw32_xor_u => 4,
        }
        atomic Xchg [Addr, S32] => [S32] {
            visit_i32_atomic_rmw_xchg => 4, visit_i32_atomic_rmw8_xchg_u => 1, visit_i32_atomic_rmw16_xchg_u => 2,
        }
        atomic Xchg [Addr, S64] => [S64] {
            visit_i64_atomic_rmw_xchg => 8, visit_i64_atomic_rmw8_xchg_u => 1,
            visit_i64_atomic_rmw16_xchg_u => 2, visit_i64_atomic_rmw32_xchg_u => 4,
        }
        atomic Cmpxchg [Addr, S32, S32] => [S32] {
            visit_i32_atomic_rmw_cmpxchg => 4, visit_i32_atomic_rmw8_cmpxchg_u => 1,
            visit_i32_atomic_rmw16_cmpxchg_u => 2,
        }
        atomic Cmpxchg [Addr, S64, S64] => [S64] {
            visit_i64_atomic_rmw_cmpxchg => 8, visit_i64_atomic_rmw8_cmpxchg_u => 1,
            visit_i64_atomic_rmw16_cmpxchg_u => 2, visit_i64_atomic_rmw32_cmpxchg_u => 4,
        }
        memory [Addr] => [S32] {
            visit_i32_load => I32Load [load(Instruction::I32LoadLocal, Instruction::I32LoadLocalAdd)],
            visit_f32_load => F32Load [load(Instruction::I32LoadLocal, Instruction::I32LoadLocalAdd)],
            visit_i32_load8_s => I32Load8S [load(Instruction::I32Load8SLocal, Instruction::I32Load8SLocalAdd)],
            visit_i32_load8_u => I32Load8U [load(Instruction::I32Load8ULocal, Instruction::I32Load8ULocalAdd)],
            visit_i32_load16_s => I32Load16S [load(Instruction::I32Load16SLocal, Instruction::I32Load16SLocalAdd)],
            visit_i32_load16_u => I32Load16U [load(Instruction::I32Load16ULocal, Instruction::I32Load16ULocalAdd)],
        }
        memory [Addr] => [S64] {
            visit_i64_load => I64Load [load(Instruction::I64LoadLocal, Instruction::I64LoadLocalAdd)],
            visit_f64_load => F64Load [load(Instruction::I64LoadLocal, Instruction::I64LoadLocalAdd)], visit_i64_load8_s => I64Load8S,
            visit_i64_load8_u => I64Load8U, visit_i64_load16_s => I64Load16S,
            visit_i64_load16_u => I64Load16U, visit_i64_load32_s => I64Load32S,
            visit_i64_load32_u => I64Load32U,
        }
        memory [Addr, S32] => [] {
            visit_f32_store => F32Store [store32()], visit_i32_store8 => I32Store8 [store_local(Instruction::I32Store8LocalLocal, false)],
            visit_i32_store16 => I32Store16 [store_local(Instruction::I32Store16LocalLocal, false)], visit_i32_store => I32Store [store32()],
        }
        memory [Addr, S64] => [] {
            visit_f64_store => F64Store [store64()], visit_i64_store8 => I64Store8 [store_local(Instruction::I64Store8LocalLocal, true)],
            visit_i64_store16 => I64Store16 [store_local(Instruction::I64Store16LocalLocal, true)],
            visit_i64_store32 => I64Store32 [store_local(Instruction::I64Store32LocalLocal, true)], visit_i64_store => I64Store [store64()],
        }
        fixed [] => [] { visit_data_drop(segment: u32) => DataDrop, visit_elem_drop(segment: u32) => ElemDrop }
        fixed [] => [S32] { visit_i32_const(value: i32) => Const32, visit_ref_func(function: u32) => RefFunc }
        heap false [] => [S32] { visit_ref_null => RefNull }
        heap false [S32] => [S32] {
            visit_ref_test_non_null => RefTest, visit_ref_cast_non_null => RefCast,
        }
        heap true [S32] => [S32] {
            visit_ref_test_nullable => RefTest, visit_ref_cast_nullable => RefCast,
        }
        fixed [S32] => [S32] {
            visit_ref_is_null => RefIsNull, visit_ref_as_non_null => RefAsNonNull, visit_ref_i31 => RefI31,
            visit_i31_get_s => I31GetS, visit_i31_get_u => I31GetU,
            visit_i32_eqz => I32Eqz [eqz32()], visit_i32_clz => I32Clz,
            visit_i32_ctz => I32Ctz, visit_i32_popcnt => I32Popcnt, visit_i32_extend8_s => I32Extend8S,
            visit_i32_extend16_s => I32Extend16S, visit_i32_trunc_f32_s => I32TruncF32S,
            visit_i32_trunc_f32_u => I32TruncF32U, visit_f32_convert_i32_s => F32ConvertI32S,
            visit_f32_convert_i32_u => F32ConvertI32U, visit_i32_trunc_sat_f32_s => I32TruncSatF32S,
            visit_i32_trunc_sat_f32_u => I32TruncSatF32U, visit_f32_abs => F32Abs, visit_f32_neg => F32Neg,
            visit_f32_ceil => F32Ceil, visit_f32_floor => F32Floor, visit_f32_trunc => F32Trunc,
            visit_f32_nearest => F32Nearest, visit_f32_sqrt => F32Sqrt,
        }
        effect [S32] => [S32] { visit_any_convert_extern, visit_extern_convert_any }
        fixed [S64] => [S64] {
            visit_i64_clz => I64Clz, visit_i64_ctz => I64Ctz, visit_i64_popcnt => I64Popcnt,
            visit_i64_extend8_s => I64Extend8S, visit_i64_extend16_s => I64Extend16S,
            visit_i64_extend32_s => I64Extend32S, visit_i64_trunc_f64_s => I64TruncF64S,
            visit_i64_trunc_f64_u => I64TruncF64U, visit_f64_convert_i64_s => F64ConvertI64S,
            visit_f64_convert_i64_u => F64ConvertI64U, visit_i64_trunc_sat_f64_s => I64TruncSatF64S,
            visit_i64_trunc_sat_f64_u => I64TruncSatF64U, visit_f64_abs => F64Abs, visit_f64_neg => F64Neg,
            visit_f64_ceil => F64Ceil, visit_f64_floor => F64Floor, visit_f64_trunc => F64Trunc,
            visit_f64_nearest => F64Nearest, visit_f64_sqrt => F64Sqrt,
        }
        fixed [S64] => [S32] {
            visit_i64_eqz => I64Eqz [eqz64()], visit_i32_wrap_i64 => I32WrapI64, visit_i32_trunc_f64_s => I32TruncF64S,
            visit_i32_trunc_f64_u => I32TruncF64U, visit_f32_convert_i64_s => F32ConvertI64S,
            visit_f32_convert_i64_u => F32ConvertI64U, visit_f32_demote_f64 => F32DemoteF64,
            visit_i32_trunc_sat_f64_s => I32TruncSatF64S, visit_i32_trunc_sat_f64_u => I32TruncSatF64U,
        }
        fixed [S32] => [S64] {
            visit_i64_extend_i32_s => I64ExtendI32S [extend_i32(true)],
            visit_i64_extend_i32_u => I64ExtendI32U [extend_i32(false)],
            visit_i64_trunc_f32_s => I64TruncF32S, visit_i64_trunc_f32_u => I64TruncF32U,
            visit_f64_convert_i32_s => F64ConvertI32S, visit_f64_convert_i32_u => F64ConvertI32U,
            visit_f64_promote_f32 => F64PromoteF32, visit_i64_trunc_sat_f32_s => I64TruncSatF32S,
            visit_i64_trunc_sat_f32_u => I64TruncSatF32U,
        }
        fixed [S32, S32] => [S32] {
            visit_ref_eq => RefEq,
            visit_i32_eq => I32Eq [compare(CmpOp::Eq)],
            visit_i32_ne => I32Ne [compare(CmpOp::Ne)],
            visit_i32_lt_s => I32LtS [compare(CmpOp::LtS)],
            visit_i32_lt_u => I32LtU [compare(CmpOp::LtU)],
            visit_i32_gt_s => I32GtS [compare(CmpOp::GtS)],
            visit_i32_gt_u => I32GtU [compare(CmpOp::GtU)],
            visit_i32_le_s => I32LeS [compare(CmpOp::LeS)],
            visit_i32_le_u => I32LeU [compare(CmpOp::LeU)],
            visit_i32_ge_s => I32GeS [compare(CmpOp::GeS)],
            visit_i32_ge_u => I32GeU [compare(CmpOp::GeU)],
            visit_f32_eq => F32Eq, visit_f32_ne => F32Ne, visit_f32_lt => F32Lt, visit_f32_gt => F32Gt,
            visit_f32_le => F32Le, visit_f32_ge => F32Ge,
            visit_i32_add => I32Add [binary32(BinOp::IAdd)],
            visit_i32_sub => I32Sub [binary32(BinOp::ISub)],
            visit_i32_mul => I32Mul [binary32(BinOp::IMul)],
            visit_i32_div_s => I32DivS, visit_i32_div_u => I32DivU,
            visit_i32_rem_s => I32RemS, visit_i32_rem_u => I32RemU,
            visit_i32_and => I32And [binary32(BinOp::IAnd)],
            visit_i32_or => I32Or [binary32(BinOp::IOr)],
            visit_i32_xor => I32Xor [binary32(BinOp::IXor)],
            visit_i32_shl => I32Shl [binary32(BinOp::IShl)],
            visit_i32_shr_s => I32ShrS [binary32(BinOp::IShrS)],
            visit_i32_shr_u => I32ShrU [binary32(BinOp::IShrU)],
            visit_i32_rotl => I32Rotl [binary32(BinOp::IRotl)],
            visit_i32_rotr => I32Rotr [binary32(BinOp::IRotr)],
            visit_f32_add => F32Add [binary32(BinOp::FAdd)],
            visit_f32_sub => F32Sub [binary32(BinOp::FSub)],
            visit_f32_mul => F32Mul [binary32(BinOp::FMul)],
            visit_f32_div => F32Div [binary32(BinOp::FDiv)],
            visit_f32_min => F32Min [binary32(BinOp::FMin)],
            visit_f32_max => F32Max [binary32(BinOp::FMax)],
            visit_f32_copysign => F32Copysign [binary32(BinOp::FCopysign)],
        }
        fixed [S64, S64] => [S32] {
            visit_i64_eq => I64Eq [compare(CmpOp::Eq)],
            visit_i64_ne => I64Ne [compare(CmpOp::Ne)],
            visit_i64_lt_s => I64LtS [compare(CmpOp::LtS)],
            visit_i64_lt_u => I64LtU [compare(CmpOp::LtU)],
            visit_i64_gt_s => I64GtS [compare(CmpOp::GtS)],
            visit_i64_gt_u => I64GtU [compare(CmpOp::GtU)],
            visit_i64_le_s => I64LeS [compare(CmpOp::LeS)],
            visit_i64_le_u => I64LeU [compare(CmpOp::LeU)],
            visit_i64_ge_s => I64GeS [compare(CmpOp::GeS)],
            visit_i64_ge_u => I64GeU [compare(CmpOp::GeU)],
            visit_f64_eq => F64Eq, visit_f64_ne => F64Ne, visit_f64_lt => F64Lt, visit_f64_gt => F64Gt,
            visit_f64_le => F64Le, visit_f64_ge => F64Ge,
        }
        fixed [S64, S64] => [S64] {
            visit_i64_add => I64Add [binary64(BinOp::IAdd)],
            visit_i64_sub => I64Sub [binary64(BinOp::ISub)],
            visit_i64_mul => I64Mul [binary64(BinOp::IMul)],
            visit_i64_div_s => I64DivS, visit_i64_div_u => I64DivU, visit_i64_rem_s => I64RemS,
            visit_i64_rem_u => I64RemU,
            visit_i64_and => I64And [binary64(BinOp::IAnd)],
            visit_i64_or => I64Or [binary64(BinOp::IOr)],
            visit_i64_xor => I64Xor [binary64(BinOp::IXor)],
            visit_i64_shl => I64Shl [binary64(BinOp::IShl)],
            visit_i64_shr_s => I64ShrS [binary64(BinOp::IShrS)],
            visit_i64_shr_u => I64ShrU [binary64(BinOp::IShrU)],
            visit_i64_rotl => I64Rotl [binary64(BinOp::IRotl)],
            visit_i64_rotr => I64Rotr [binary64(BinOp::IRotr)],
            visit_f64_add => F64Add [binary64(BinOp::FAdd)],
            visit_f64_sub => F64Sub [binary64(BinOp::FSub)],
            visit_f64_mul => F64Mul [binary64(BinOp::FMul)],
            visit_f64_div => F64Div [binary64(BinOp::FDiv)],
            visit_f64_min => F64Min [binary64(BinOp::FMin)],
            visit_f64_max => F64Max [binary64(BinOp::FMax)],
            visit_f64_copysign => F64Copysign [binary64(BinOp::FCopysign)],
        }
        fixed [S64, S64, S64, S64] => [S64, S64] { visit_i64_add128 => I64Add128, visit_i64_sub128 => I64Sub128 }
        fixed [S64, S64] => [S64, S64] { visit_i64_mul_wide_s => I64MulWideS, visit_i64_mul_wide_u => I64MulWideU }
        effect [] => [] { visit_nop }
        effect [S32] => [S32] { visit_f32_reinterpret_i32, visit_i32_reinterpret_f32 }
        effect [S64] => [S64] { visit_f64_reinterpret_i64, visit_i64_reinterpret_f64 }
        terminating [] => [] { visit_unreachable => Unreachable, visit_return => Return }
        indexed (memory_address_lane) [] => [Addr] { visit_memory_size(memory: u32) => MemorySize }
        indexed (memory_address_lane) [Addr] => [Addr] { visit_memory_grow(memory: u32) => MemoryGrow }
        indexed (memory_address_lane) [Addr, S32, Addr] => [] { visit_memory_fill(memory: u32) => MemoryFill [memory_fill()] }
        indexed (metadata.table_index_lane) [Addr] => [S32] { visit_table_get(table: u32) => TableGet }
        indexed (metadata.table_index_lane) [Addr, S32] => [] { visit_table_set(table: u32) => TableSet }
        indexed (metadata.table_index_lane) [] => [Addr] { visit_table_size(table: u32) => TableSize }
        indexed (metadata.table_index_lane) [S32, Addr] => [Addr] { visit_table_grow(table: u32) => TableGrow }
        indexed (metadata.table_index_lane) [Addr, S32, Addr] => [] { visit_table_fill(table: u32) => TableFill }
        fixed [] => [S32] { visit_struct_new_default(type_index: u32) => StructNewDefault }
        fixed [S32] => [S32] {
            visit_array_new_default(type_index: u32) => ArrayNewDefault, visit_array_len => ArrayLen,
        }
        indexed (metadata.array_element_lane) [Field, S32] => [S32] { visit_array_new(type_index: u32) => ArrayNew }
        indexed (metadata.array_element_lane) [S32, S32] => [Field] {
            visit_array_get(type_index: u32) => ArrayGet, visit_array_get_s(type_index: u32) => ArrayGetS,
            visit_array_get_u(type_index: u32) => ArrayGetU,
        }
        indexed (metadata.array_element_lane) [S32, S32, Field] => [] { visit_array_set(type_index: u32) => ArraySet }
        indexed (metadata.array_element_lane) [S32, S32, Field, S32] => [] { visit_array_fill(type_index: u32) => ArrayFill }
    }

    fn visit_struct_new(&mut self, type_index: u32) -> Result<()> {
        let field_count = self.metadata.struct_fields(type_index)?.len();
        for field_index in (0..field_count).rev() {
            let size = self.metadata.struct_field(type_index, field_index as u32)?;
            self.pop_expect(size)?;
        }
        self.push_sizes(&[ValueLane::S32])?;
        self.emitter.emit(Instruction::StructNew(type_index))?;
        Ok(())
    }

    fn visit_struct_get(&mut self, type_index: u32, field_index: u32) -> Result<()> {
        self.visit_struct_get_impl(type_index, field_index, Instruction::StructGet)
    }

    fn visit_struct_get_s(&mut self, type_index: u32, field_index: u32) -> Result<()> {
        self.visit_struct_get_impl(type_index, field_index, Instruction::StructGetS)
    }

    fn visit_struct_get_u(&mut self, type_index: u32, field_index: u32) -> Result<()> {
        self.visit_struct_get_impl(type_index, field_index, Instruction::StructGetU)
    }

    fn visit_struct_set(&mut self, type_index: u32, field_index: u32) -> Result<()> {
        let size = self.metadata.struct_field(type_index, field_index)?;
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, field_index))?;
        self.emit(&[ValueLane::S32, size], &[], Instruction::StructSet(operand))
    }

    fn visit_array_new_fixed(&mut self, type_index: u32, array_size: u32) -> Result<()> {
        let size = self.metadata.array_element_lane(type_index)?;
        for _ in 0..array_size {
            self.pop_expect(size)?;
        }
        self.push_sizes(&[ValueLane::S32])?;
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, array_size))?;
        self.emitter.emit(Instruction::ArrayNewFixed(operand))?;
        Ok(())
    }

    fn visit_call(&mut self, function_index: u32) -> Result<()> {
        let signature = self.metadata.function_signature(function_index)?;
        self.emit_call(&signature.params, &signature.results, Instruction::Call(function_index), None)
    }

    fn visit_call_indirect(&mut self, type_index: u32, table_index: u32) -> Result<()> {
        let index_lane = self.metadata.table_index_lane(table_index)?;
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, table_index))?;
        let signature = self.metadata.signature(type_index)?;
        self.emit_call(&signature.params, &signature.results, Instruction::CallIndirect(operand), Some(index_lane))
    }

    fn visit_call_ref(&mut self, type_index: u32) -> Result<()> {
        let signature = self.metadata.signature(type_index)?;
        self.emit_call(&signature.params, &signature.results, Instruction::CallRef(type_index), Some(ValueLane::S32))
    }

    fn visit_return_call(&mut self, function_index: u32) -> Result<()> {
        let signature = self.metadata.function_signature(function_index)?;
        self.emit_call(&signature.params, &[], Instruction::ReturnCall(function_index), None)?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_return_call_indirect(&mut self, type_index: u32, table_index: u32) -> Result<()> {
        let index_lane = self.metadata.table_index_lane(table_index)?;
        let signature = self.metadata.signature(type_index)?;
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, table_index))?;
        self.emit_call(&signature.params, &[], Instruction::ReturnCallIndirect(operand), Some(index_lane))?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_return_call_ref(&mut self, type_index: u32) -> Result<()> {
        let signature = self.metadata.signature(type_index)?;
        self.emit_call(&signature.params, &[], Instruction::ReturnCallRef(type_index), Some(ValueLane::S32))?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_global_set(&mut self, global_index: u32) -> Result<()> {
        let size = self.metadata.global_value_lane(global_index)?;
        let instruction = size.select(
            Instruction::GlobalSet32(global_index),
            Instruction::GlobalSet64(global_index),
            Instruction::GlobalSet128(global_index),
        );
        self.emit(&[size], &[], instruction)
    }

    fn visit_global_get(&mut self, global_index: u32) -> Result<()> {
        let size = self.metadata.global_value_lane(global_index)?;
        match size {
            ValueLane::S32 => emit_selected!(self, &[], &[size], GlobalGet32(global_index)[global_get32()]),
            ValueLane::S64 => emit_selected!(self, &[], &[size], GlobalGet64(global_index)[global_get64()]),
            ValueLane::S128 => emit_selected!(self, &[], &[size], GlobalGet128(global_index)[global_get128()]),
        }
    }

    fn visit_drop(&mut self) -> Result<()> {
        let size = self.operand_stack.last().copied().unwrap_or(ValueLane::S32);
        match size {
            ValueLane::S32 => emit_selected!(self, &[size], &[], Drop32[drop32()]),
            ValueLane::S64 => emit_selected!(self, &[size], &[], Drop64[drop64()]),
            ValueLane::S128 => emit_selected!(self, &[size], &[], Drop128[drop128()]),
        }
    }

    fn visit_select(&mut self) -> Result<()> {
        let size = self.operand_stack.iter().rev().nth(1).copied().unwrap_or(ValueLane::S32);
        let instruction = size.select(Instruction::Select32, Instruction::Select64, Instruction::Select128);
        self.emit(&[size, size, ValueLane::S32], &[size], instruction)
    }

    fn visit_local_get(&mut self, idx: u32) -> Result<()> {
        let (size, local_idx) = self.local(idx)?;
        match size {
            ValueLane::S32 => emit_selected!(self, &[], &[size], LocalGet32(local_idx)[local_get32()]),
            ValueLane::S64 => emit_selected!(self, &[], &[size], LocalGet64(local_idx)[local_get64()]),
            ValueLane::S128 => emit_selected!(self, &[], &[size], LocalGet128(local_idx)[local_get128()]),
        }
    }

    fn visit_local_set(&mut self, idx: u32) -> Result<()> {
        let (size, local_idx) = self.local(idx)?;
        match size {
            ValueLane::S32 => emit_selected!(self, &[size], &[], LocalSet32(local_idx)[local_set32()]),
            ValueLane::S64 => emit_selected!(self, &[size], &[], LocalSet64(local_idx)[local_set64()]),
            ValueLane::S128 => emit_selected!(self, &[size], &[], LocalSet128(local_idx)[local_set128()]),
        }
    }

    fn visit_local_tee(&mut self, idx: u32) -> Result<()> {
        let (size, local_idx) = self.local(idx)?;
        // Bound labels seal the tail, so these rules cannot fuse across a loop
        // entry or another branch destination.
        match size {
            ValueLane::S32 => emit_selected!(self, &[size], &[size], LocalTee32(local_idx)[local_tee32()]),
            ValueLane::S64 => emit_selected!(self, &[size], &[size], LocalTee64(local_idx)[local_tee64()]),
            ValueLane::S128 => emit_selected!(self, &[size], &[size], LocalTee128(local_idx)[local_tee128()]),
        }
    }

    fn visit_block(&mut self, blockty: wasmparser::BlockType) -> Result<()> {
        self.push_control(BlockKind::Block, blockty, None)
    }

    fn visit_loop(&mut self, ty: wasmparser::BlockType) -> Result<()> {
        self.push_control(BlockKind::Loop, ty, None)
    }

    fn visit_if(&mut self, ty: wasmparser::BlockType) -> Result<()> {
        self.pop_expect(ValueLane::S32)?;
        let else_entry = self.emitter.new_label();
        self.emitter.branch_if(true, else_entry)?;
        self.push_control(BlockKind::If, ty, Some(else_entry))
    }

    fn visit_try_table(&mut self, try_table: wasmparser::TryTable) -> Result<()> {
        let signature = self.block_signature(try_table.ty)?;
        for &size in signature.params().iter().rev() {
            self.pop_expect(size)?;
        }
        let height = self.operand_stack.len();
        let base = self.lane_counts;
        let entry_unreachable = self.is_unreachable();

        let body_start = self.emitter.new_label();
        let body_end = self.emitter.new_label();
        self.emitter.branch(Instruction::Jump(0), body_start)?;
        let mut catches = Vec::with_capacity(try_table.catches.len());
        for catch in try_table.catches {
            let (tag, depth, with_ref) = match catch {
                wasmparser::Catch::One { tag, label } => (Some(tag), label, false),
                wasmparser::Catch::OneRef { tag, label } => (Some(tag), label, true),
                wasmparser::Catch::All { label } => (None, label, false),
                wasmparser::Catch::AllRef { label } => (None, label, true),
            };
            let target_idx = self.get_ctx_idx(depth)?;
            let target_base = self.control_stack[target_idx].base;
            // The runtime truncates to the target base, then injects the exception payload and
            // optional reference before executing this landing pad. These pushes do not pass
            // through the ordinary logical operand stack, but still need function-entry capacity.
            let mut landing_counts = target_base;
            if let Some(tag) = tag {
                for &lane in &self.metadata.tag_signature(tag)?.params {
                    Self::increment_lane(&mut landing_counts, lane)?;
                }
            }
            if with_ref {
                Self::increment_lane(&mut landing_counts, ValueLane::S32)?;
            }
            self.max_lane_counts.c32 = self.max_lane_counts.c32.max(landing_counts.c32);
            self.max_lane_counts.c64 = self.max_lane_counts.c64.max(landing_counts.c64);
            self.max_lane_counts.c128 = self.max_lane_counts.c128.max(landing_counts.c128);
            let landing_label = self.emitter.new_label();
            self.emitter.bind(landing_label)?;
            self.emit_jump_or_return(target_idx)?;
            let landing_pad = 0;
            catches.push((
                match tag {
                    Some(tag) => tinywasm_types::ExceptionCatch::Tag { tag, landing_pad, base: target_base, with_ref },
                    None => tinywasm_types::ExceptionCatch::All { landing_pad, base: target_base, with_ref },
                },
                landing_label,
            ));
        }

        self.emitter.bind(body_start)?;
        self.emitter.exception_handler(body_start, body_end, catches);
        self.push_sizes(signature.params())?;
        self.control_stack.push(ControlFrame {
            kind: BlockKind::TryTable,
            has_else: false,
            loop_start: None,
            end: Some(body_end),
            else_entry: None,
            height,
            base,
            signature,
            unreachable: entry_unreachable,
            entry_unreachable,
            end_reachable: false,
        });
        Ok(())
    }

    fn visit_throw(&mut self, tag_index: u32) -> Result<()> {
        let signature = self.metadata.tag_signature(tag_index)?;
        self.apply_effect(&signature.params, &[])?;
        self.emitter.emit_boundary(Instruction::Throw(tag_index))?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_throw_ref(&mut self) -> Result<()> {
        self.apply_effect(&[ValueLane::S32], &[])?;
        self.emitter.emit_boundary(Instruction::ThrowRef)?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_else(&mut self) -> Result<()> {
        let (else_entry, end, height, base, signature, entry_unreachable) = {
            let ctx = self
                .control_stack
                .last_mut()
                .filter(|ctx| matches!(ctx.kind, BlockKind::If) && !ctx.has_else)
                .ok_or_else(|| crate::ParseError::Other("else without matching if".into()))?;
            ctx.end_reachable |= !ctx.unreachable;
            ctx.has_else = true;
            let end = *ctx.end.get_or_insert_with(|| self.emitter.new_label());
            (ctx.else_entry.unwrap(), end, ctx.height, ctx.base, ctx.signature, ctx.entry_unreachable)
        };
        self.emitter.branch(Instruction::Jump(0), end)?;
        self.emitter.bind(else_entry)?;
        self.reset_stack(height, base);
        self.push_sizes(signature.params())?;
        self.control_stack.last_mut().unwrap().unreachable = entry_unreachable;
        Ok(())
    }

    fn visit_end(&mut self) -> Result<()> {
        let ctx =
            self.control_stack.pop().ok_or_else(|| crate::ParseError::Other("end without control frame".into()))?;
        if let Some(end) = ctx.end {
            self.emitter.bind(end)?;
        }
        if !ctx.has_else
            && let Some(else_entry) = ctx.else_entry
        {
            self.emitter.bind(else_entry)?;
        }
        if matches!(ctx.kind, BlockKind::Function) {
            self.emitter.emit_boundary(Instruction::Return)?;
        } else {
            let reachable = !ctx.entry_unreachable
                && (!ctx.unreachable || ctx.end_reachable || matches!(ctx.kind, BlockKind::If) && !ctx.has_else);
            self.reset_stack(ctx.height, ctx.base);
            self.push_sizes(ctx.signature.results())?;
            if let Some(parent) = self.control_stack.last_mut() {
                parent.unreachable = !reachable;
            }
        }
        Ok(())
    }

    fn visit_br(&mut self, depth: u32) -> Result<()> {
        self.emit_branch_transfer(self.branch_destination(depth)?)?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_br_if(&mut self, depth: u32) -> Result<()> {
        self.pop_expect(ValueLane::S32)?;
        let destination = self.branch_destination(depth)?;
        if destination.shape.is_none()
            && let Some(target) = self.branch_label(destination.frame)
        {
            self.emitter.branch_if(false, target)?;
            // Preserve legacy reachability for direct br_if backedges only.
            self.control_stack[destination.frame].end_reachable = true;
            return Ok(());
        }
        let fallthrough = self.emitter.new_label();
        self.emitter.branch_if(true, fallthrough)?;
        self.emit_branch_transfer(destination)?;
        self.emitter.bind(fallthrough)?;
        Ok(())
    }

    fn visit_br_table(&mut self, targets: wasmparser::BrTable<'_>) -> Result<()> {
        let depths = targets.targets().collect::<Result<Vec<_>, wasmparser::Error>>()?;
        self.pop_expect(ValueLane::S32)?;

        let mut pads = Vec::new();
        let label_count =
            depths.len().checked_add(1).ok_or_else(|| crate::ParseError::Other("branch table is too large".into()))?;
        let mut labels = Vec::with_capacity(label_count);
        let reachable = !self.is_unreachable();
        let direct = reachable && self.emitter.optimizations_enabled();
        for depth in depths.into_iter().chain(core::iter::once(targets.default())) {
            if let Some((_, label, _)) = pads.iter().find(|(pad_depth, _, _)| *pad_depth == depth) {
                labels.push(*label);
                continue;
            }
            let destination = reachable.then(|| self.branch_destination(depth)).transpose()?;
            if direct
                && let Some(dest) = &destination
                && dest.shape.is_none()
                && let Some(label) = self.branch_label(dest.frame)
            {
                labels.push(label);
                continue;
            }
            let label = self.emitter.new_label();
            pads.push((depth, label, destination));
            labels.push(label);
        }
        let default = labels.pop().unwrap();
        self.emitter.branch_table(&labels, default)?;
        for (_, label, destination) in pads {
            self.emitter.bind(label)?;
            if let Some(destination) = destination {
                self.emit_branch_transfer(destination)?;
            } else {
                self.emitter.emit_boundary(Instruction::Return)?;
            }
        }
        self.mark_unreachable();
        Ok(())
    }

    fn visit_f32_const(&mut self, val: wasmparser::Ieee32) -> Result<()> {
        self.emit(&[], &[ValueLane::S32], Instruction::Const32(val.bits() as i32))
    }

    #[cfg(not(rust_analyzer))] // rust-analyzer thinks the return type is wrong
    fn visit_f64_const(&mut self, val: wasmparser::Ieee64) -> Result<()> {
        self.visit_i64_const(val.bits() as i64)
    }

    fn visit_i64_const(&mut self, value: i64) -> Result<()> {
        let instruction = if self.emitter.optimizations_enabled()
            && let Ok(value) = i32::try_from(value)
        {
            Instruction::Const64Imm(value)
        } else {
            Instruction::Const64(self.push64(Operand64::<i64>::new(value))?)
        };
        self.emit(&[], &[ValueLane::S64], instruction)
    }

    fn visit_table_copy(&mut self, dst_table: u32, src_table: u32) -> Result<()> {
        let dst = self.metadata.table_index_lane(dst_table)?;
        let src = self.metadata.table_index_lane(src_table)?;
        let len = if dst == ValueLane::S32 || src == ValueLane::S32 { ValueLane::S32 } else { ValueLane::S64 };
        let operand = self.push64(Operand64::<(u32, u32)>::new(dst_table, src_table))?;
        self.emit(&[dst, src, len], &[], Instruction::TableCopy(operand))
    }

    fn visit_memory_copy(&mut self, dst_mem: u32, src_mem: u32) -> Result<()> {
        let dst = self.metadata.memory_address_lane(dst_mem)?;
        let src = self.metadata.memory_address_lane(src_mem)?;
        self.mark_memory(dst_mem);
        self.mark_memory(src_mem);
        let len = if dst == ValueLane::S32 || src == ValueLane::S32 { ValueLane::S32 } else { ValueLane::S64 };
        let operand = self.push64(Operand64::<(u32, u32)>::new(dst_mem, src_mem))?;
        self.emit(&[dst, src, len], &[], Instruction::MemoryCopy(operand))
    }

    fn visit_memory_init(&mut self, data_index: u32, memory: u32) -> Result<()> {
        let dst = self.metadata.memory_address_lane(memory)?;
        self.mark_memory(memory);
        let operand = self.push64(Operand64::<(u32, u32)>::new(data_index, memory))?;
        self.emit(&[dst, ValueLane::S32, ValueLane::S32], &[], Instruction::MemoryInit(operand))
    }

    fn visit_table_init(&mut self, elem_index: u32, table: u32) -> Result<()> {
        let address = self.metadata.table_index_lane(table)?;
        let operand = self.push64(Operand64::<(u32, u32)>::new(elem_index, table))?;
        self.emit(&[address, ValueLane::S32, ValueLane::S32], &[], Instruction::TableInit(operand))
    }

    fn visit_array_new_data(&mut self, type_index: u32, data_index: u32) -> Result<()> {
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, data_index))?;
        self.emit(&[ValueLane::S32, ValueLane::S32], &[ValueLane::S32], Instruction::ArrayNewData(operand))
    }

    fn visit_array_new_elem(&mut self, type_index: u32, elem_index: u32) -> Result<()> {
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, elem_index))?;
        self.emit(&[ValueLane::S32, ValueLane::S32], &[ValueLane::S32], Instruction::ArrayNewElem(operand))
    }

    fn visit_array_init_data(&mut self, type_index: u32, data_index: u32) -> Result<()> {
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, data_index))?;
        self.emit(&[ValueLane::S32; 4], &[], Instruction::ArrayInitData(operand))
    }

    fn visit_array_init_elem(&mut self, type_index: u32, elem_index: u32) -> Result<()> {
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index, elem_index))?;
        self.emit(&[ValueLane::S32; 4], &[], Instruction::ArrayInitElem(operand))
    }

    fn visit_array_copy(&mut self, type_index_dst: u32, type_index_src: u32) -> Result<()> {
        let operand = self.push64(Operand64::<(u32, u32)>::new(type_index_dst, type_index_src))?;
        self.emit(&[ValueLane::S32; 5], &[], Instruction::ArrayCopy(operand))
    }

    fn visit_br_on_cast(
        &mut self,
        relative_depth: u32,
        _from_ref_type: wasmparser::RefType,
        to_ref_type: wasmparser::RefType,
    ) -> Result<()> {
        self.emit_cast_branch(relative_depth, to_ref_type, false)
    }

    fn visit_br_on_cast_fail(
        &mut self,
        relative_depth: u32,
        _from_ref_type: wasmparser::RefType,
        to_ref_type: wasmparser::RefType,
    ) -> Result<()> {
        self.emit_cast_branch(relative_depth, to_ref_type, true)
    }

    fn visit_br_on_null(&mut self, relative_depth: u32) -> Result<()> {
        self.pop_expect(ValueLane::S32)?;
        let fallthrough = self.emitter.new_label();
        self.emitter.branch(Instruction::JumpIfRefNonNull(0), fallthrough)?;
        self.emit_branch_transfer(self.branch_destination(relative_depth)?)?;
        self.emitter.bind(fallthrough)?;
        self.push_sizes(&[ValueLane::S32])
    }

    fn visit_br_on_non_null(&mut self, relative_depth: u32) -> Result<()> {
        self.pop_expect(ValueLane::S32)?;
        let fallthrough = self.emitter.new_label();
        self.emitter.branch(Instruction::JumpIfRefNull(0), fallthrough)?;
        self.push_sizes(&[ValueLane::S32])?;
        let destination = self.branch_destination(relative_depth)?;
        self.emit_branch_shape(&destination)?;
        self.pop_expect(ValueLane::S32)?;
        self.emit_jump_or_return(destination.frame)?;
        self.emitter.bind(fallthrough)?;
        Ok(())
    }

    fn visit_typed_select_multi(&mut self, tys: Vec<wasmparser::ValType>) -> Result<()> {
        let sizes: Vec<_> = tys.into_iter().map(value_lane).collect();
        let counts = Self::value_counts(&sizes);
        self.emit(
            &[sizes.as_slice(), sizes.as_slice(), &[ValueLane::S32]].concat(),
            &sizes,
            Instruction::SelectMulti(counts),
        )
    }

    fn visit_typed_select(&mut self, ty: wasmparser::ValType) -> Result<()> {
        let size = value_lane(ty);
        let instruction = size.select(Instruction::Select32, Instruction::Select64, Instruction::Select128);
        self.emit(&[size, size, ValueLane::S32], &[size], instruction)
    }
}

impl wasmparser::VisitSimdOperator<'_> for FunctionBuilder<'_> {
    wasmparser::for_each_visit_simd_operator!(impl_visit_operator);

    lowering_ops! {
        memory [Addr] => [S128] {
            visit_v128_load => V128Load, visit_v128_load8x8_s => V128Load8x8S,
            visit_v128_load8x8_u => V128Load8x8U, visit_v128_load16x4_s => V128Load16x4S,
            visit_v128_load16x4_u => V128Load16x4U, visit_v128_load32x2_s => V128Load32x2S,
            visit_v128_load32x2_u => V128Load32x2U, visit_v128_load8_splat => V128Load8Splat,
            visit_v128_load16_splat => V128Load16Splat, visit_v128_load32_splat => V128Load32Splat,
            visit_v128_load64_splat => V128Load64Splat, visit_v128_load32_zero => V128Load32Zero,
            visit_v128_load64_zero => V128Load64Zero,
        }
        memory [Addr, S128] => [] { visit_v128_store => V128Store [store128()] }
        memory [Addr, S128] => [S128] {
            visit_v128_load8_lane(lane: u8) => V128Load8Lane,
            visit_v128_load16_lane(lane: u8) => V128Load16Lane,
            visit_v128_load32_lane(lane: u8) => V128Load32Lane,
            visit_v128_load64_lane(lane: u8) => V128Load64Lane,
        }
        memory [Addr, S128] => [] {
            visit_v128_store8_lane(lane: u8) => V128Store8Lane,
            visit_v128_store16_lane(lane: u8) => V128Store16Lane,
            visit_v128_store32_lane(lane: u8) => V128Store32Lane,
            visit_v128_store64_lane(lane: u8) => V128Store64Lane,
        }
        fixed [S32] => [S128] {
            visit_i8x16_splat => I8x16Splat, visit_i16x8_splat => I16x8Splat,
            visit_i32x4_splat => I32x4Splat, visit_f32x4_splat => F32x4Splat,
        }
        fixed [S64] => [S128] { visit_i64x2_splat => I64x2Splat, visit_f64x2_splat => F64x2Splat }
        fixed [S128] => [S32] {
            visit_v128_any_true => V128AnyTrue, visit_i8x16_all_true => I8x16AllTrue,
            visit_i8x16_bitmask => I8x16Bitmask, visit_i16x8_all_true => I16x8AllTrue,
            visit_i16x8_bitmask => I16x8Bitmask, visit_i32x4_all_true => I32x4AllTrue,
            visit_i32x4_bitmask => I32x4Bitmask, visit_i64x2_all_true => I64x2AllTrue,
            visit_i64x2_bitmask => I64x2Bitmask, visit_i8x16_extract_lane_s(lane: u8) => I8x16ExtractLaneS,
            visit_i8x16_extract_lane_u(lane: u8) => I8x16ExtractLaneU,
            visit_i16x8_extract_lane_s(lane: u8) => I16x8ExtractLaneS,
            visit_i16x8_extract_lane_u(lane: u8) => I16x8ExtractLaneU,
            visit_i32x4_extract_lane(lane: u8) => I32x4ExtractLane,
            visit_f32x4_extract_lane(lane: u8) => F32x4ExtractLane,
        }
        fixed [S128] => [S64] {
            visit_i64x2_extract_lane(lane: u8) => I64x2ExtractLane,
            visit_f64x2_extract_lane(lane: u8) => F64x2ExtractLane,
        }
        fixed [S128, S32] => [S128] {
            visit_i8x16_shl => I8x16Shl, visit_i8x16_shr_s => I8x16ShrS,
            visit_i8x16_shr_u => I8x16ShrU, visit_i16x8_shl => I16x8Shl, visit_i16x8_shr_s => I16x8ShrS,
            visit_i16x8_shr_u => I16x8ShrU, visit_i32x4_shl => I32x4Shl, visit_i32x4_shr_s => I32x4ShrS,
            visit_i32x4_shr_u => I32x4ShrU, visit_i64x2_shl => I64x2Shl, visit_i64x2_shr_s => I64x2ShrS,
            visit_i64x2_shr_u => I64x2ShrU, visit_i8x16_replace_lane(lane: u8) => I8x16ReplaceLane,
            visit_i16x8_replace_lane(lane: u8) => I16x8ReplaceLane,
            visit_i32x4_replace_lane(lane: u8) => I32x4ReplaceLane,
            visit_f32x4_replace_lane(lane: u8) => F32x4ReplaceLane,
        }
        fixed [S128, S64] => [S128] {
            visit_i64x2_replace_lane(lane: u8) => I64x2ReplaceLane,
            visit_f64x2_replace_lane(lane: u8) => F64x2ReplaceLane,
        }
        fixed [S128] => [S128] {
            visit_v128_not => V128Not, visit_i8x16_abs => I8x16Abs, visit_i8x16_neg => I8x16Neg,
            visit_i16x8_abs => I16x8Abs, visit_i16x8_neg => I16x8Neg, visit_i32x4_abs => I32x4Abs,
            visit_i32x4_neg => I32x4Neg, visit_i64x2_abs => I64x2Abs, visit_i64x2_neg => I64x2Neg,
            visit_i16x8_extadd_pairwise_i8x16_s => I16x8ExtAddPairwiseI8x16S,
            visit_i16x8_extadd_pairwise_i8x16_u => I16x8ExtAddPairwiseI8x16U,
            visit_i32x4_extadd_pairwise_i16x8_s => I32x4ExtAddPairwiseI16x8S,
            visit_i32x4_extadd_pairwise_i16x8_u => I32x4ExtAddPairwiseI16x8U,
            visit_i16x8_extend_low_i8x16_s => I16x8ExtendLowI8x16S,
            visit_i16x8_extend_low_i8x16_u => I16x8ExtendLowI8x16U,
            visit_i16x8_extend_high_i8x16_s => I16x8ExtendHighI8x16S,
            visit_i16x8_extend_high_i8x16_u => I16x8ExtendHighI8x16U,
            visit_i32x4_extend_low_i16x8_s => I32x4ExtendLowI16x8S,
            visit_i32x4_extend_low_i16x8_u => I32x4ExtendLowI16x8U,
            visit_i32x4_extend_high_i16x8_s => I32x4ExtendHighI16x8S,
            visit_i32x4_extend_high_i16x8_u => I32x4ExtendHighI16x8U,
            visit_i64x2_extend_low_i32x4_s => I64x2ExtendLowI32x4S,
            visit_i64x2_extend_low_i32x4_u => I64x2ExtendLowI32x4U,
            visit_i64x2_extend_high_i32x4_s => I64x2ExtendHighI32x4S,
            visit_i64x2_extend_high_i32x4_u => I64x2ExtendHighI32x4U, visit_i8x16_popcnt => I8x16Popcnt,
            visit_f32x4_ceil => F32x4Ceil, visit_f32x4_floor => F32x4Floor, visit_f32x4_trunc => F32x4Trunc,
            visit_f32x4_nearest => F32x4Nearest, visit_f32x4_abs => F32x4Abs, visit_f32x4_neg => F32x4Neg,
            visit_f32x4_sqrt => F32x4Sqrt, visit_f64x2_ceil => F64x2Ceil, visit_f64x2_floor => F64x2Floor,
            visit_f64x2_trunc => F64x2Trunc, visit_f64x2_nearest => F64x2Nearest, visit_f64x2_abs => F64x2Abs,
            visit_f64x2_neg => F64x2Neg, visit_f64x2_sqrt => F64x2Sqrt,
            visit_i32x4_trunc_sat_f32x4_s => I32x4TruncSatF32x4S,
            visit_i32x4_trunc_sat_f32x4_u => I32x4TruncSatF32x4U,
            visit_f32x4_convert_i32x4_s => F32x4ConvertI32x4S,
            visit_f32x4_convert_i32x4_u => F32x4ConvertI32x4U,
            visit_i32x4_trunc_sat_f64x2_s_zero => I32x4TruncSatF64x2SZero,
            visit_i32x4_trunc_sat_f64x2_u_zero => I32x4TruncSatF64x2UZero,
            visit_f64x2_convert_low_i32x4_s => F64x2ConvertLowI32x4S,
            visit_f64x2_convert_low_i32x4_u => F64x2ConvertLowI32x4U,
            visit_f32x4_demote_f64x2_zero => F32x4DemoteF64x2Zero,
            visit_f64x2_promote_low_f32x4 => F64x2PromoteLowF32x4,
            visit_i32x4_relaxed_trunc_f32x4_s => I32x4RelaxedTruncF32x4S,
            visit_i32x4_relaxed_trunc_f32x4_u => I32x4RelaxedTruncF32x4U,
            visit_i32x4_relaxed_trunc_f64x2_s_zero => I32x4RelaxedTruncF64x2SZero,
            visit_i32x4_relaxed_trunc_f64x2_u_zero => I32x4RelaxedTruncF64x2UZero,
        }
        fixed [S128, S128] => [S128] {
            visit_v128_and => V128And,
            visit_v128_andnot => V128AndNot,
            visit_v128_or => V128Or [vector_or()],
            visit_v128_xor => V128Xor,
            visit_i8x16_swizzle => I8x16Swizzle, visit_i8x16_eq => I8x16Eq,
            visit_i8x16_ne => I8x16Ne, visit_i8x16_lt_s => I8x16LtS, visit_i8x16_lt_u => I8x16LtU,
            visit_i8x16_gt_s => I8x16GtS, visit_i8x16_gt_u => I8x16GtU, visit_i8x16_le_s => I8x16LeS,
            visit_i8x16_le_u => I8x16LeU, visit_i8x16_ge_s => I8x16GeS, visit_i8x16_ge_u => I8x16GeU,
            visit_i16x8_eq => I16x8Eq, visit_i16x8_ne => I16x8Ne, visit_i16x8_lt_s => I16x8LtS,
            visit_i16x8_lt_u => I16x8LtU, visit_i16x8_gt_s => I16x8GtS, visit_i16x8_gt_u => I16x8GtU,
            visit_i16x8_le_s => I16x8LeS, visit_i16x8_le_u => I16x8LeU, visit_i16x8_ge_s => I16x8GeS,
            visit_i16x8_ge_u => I16x8GeU, visit_i32x4_eq => I32x4Eq, visit_i32x4_ne => I32x4Ne,
            visit_i32x4_lt_s => I32x4LtS, visit_i32x4_lt_u => I32x4LtU, visit_i32x4_gt_s => I32x4GtS,
            visit_i32x4_gt_u => I32x4GtU, visit_i32x4_le_s => I32x4LeS, visit_i32x4_le_u => I32x4LeU,
            visit_i32x4_ge_s => I32x4GeS, visit_i32x4_ge_u => I32x4GeU, visit_i64x2_eq => I64x2Eq,
            visit_i64x2_ne => I64x2Ne, visit_i64x2_lt_s => I64x2LtS, visit_i64x2_gt_s => I64x2GtS,
            visit_i64x2_le_s => I64x2LeS, visit_i64x2_ge_s => I64x2GeS, visit_f32x4_eq => F32x4Eq,
            visit_f32x4_ne => F32x4Ne, visit_f32x4_lt => F32x4Lt, visit_f32x4_gt => F32x4Gt,
            visit_f32x4_le => F32x4Le, visit_f32x4_ge => F32x4Ge, visit_f64x2_eq => F64x2Eq,
            visit_f64x2_ne => F64x2Ne, visit_f64x2_lt => F64x2Lt, visit_f64x2_gt => F64x2Gt,
            visit_f64x2_le => F64x2Le, visit_f64x2_ge => F64x2Ge, visit_i8x16_add => I8x16Add,
            visit_i8x16_sub => I8x16Sub, visit_i8x16_min_s => I8x16MinS, visit_i8x16_min_u => I8x16MinU,
            visit_i8x16_max_s => I8x16MaxS, visit_i8x16_max_u => I8x16MaxU,
            visit_i8x16_narrow_i16x8_s => I8x16NarrowI16x8S,
            visit_i8x16_narrow_i16x8_u => I8x16NarrowI16x8U, visit_i8x16_add_sat_s => I8x16AddSatS,
            visit_i8x16_add_sat_u => I8x16AddSatU, visit_i8x16_sub_sat_s => I8x16SubSatS,
            visit_i8x16_sub_sat_u => I8x16SubSatU, visit_i8x16_avgr_u => I8x16AvgrU,
            visit_i16x8_add => I16x8Add, visit_i16x8_sub => I16x8Sub, visit_i16x8_min_s => I16x8MinS,
            visit_i16x8_min_u => I16x8MinU, visit_i16x8_max_s => I16x8MaxS, visit_i16x8_max_u => I16x8MaxU,
            visit_i16x8_narrow_i32x4_s => I16x8NarrowI32x4S,
            visit_i16x8_narrow_i32x4_u => I16x8NarrowI32x4U, visit_i16x8_add_sat_s => I16x8AddSatS,
            visit_i16x8_add_sat_u => I16x8AddSatU, visit_i16x8_sub_sat_s => I16x8SubSatS,
            visit_i16x8_sub_sat_u => I16x8SubSatU, visit_i16x8_avgr_u => I16x8AvgrU,
            visit_i16x8_mul => I16x8Mul, visit_i32x4_add => I32x4Add, visit_i32x4_sub => I32x4Sub,
            visit_i32x4_min_s => I32x4MinS, visit_i32x4_min_u => I32x4MinU, visit_i32x4_max_s => I32x4MaxS,
            visit_i32x4_max_u => I32x4MaxU, visit_i32x4_mul => I32x4Mul,
            visit_i64x2_add => I64x2Add,
            visit_i64x2_sub => I64x2Sub,
            visit_i64x2_mul => I64x2Mul,
            visit_i16x8_extmul_low_i8x16_s => I16x8ExtMulLowI8x16S,
            visit_i16x8_extmul_low_i8x16_u => I16x8ExtMulLowI8x16U,
            visit_i16x8_extmul_high_i8x16_s => I16x8ExtMulHighI8x16S,
            visit_i16x8_extmul_high_i8x16_u => I16x8ExtMulHighI8x16U,
            visit_i32x4_extmul_low_i16x8_s => I32x4ExtMulLowI16x8S,
            visit_i32x4_extmul_low_i16x8_u => I32x4ExtMulLowI16x8U,
            visit_i32x4_extmul_high_i16x8_s => I32x4ExtMulHighI16x8S,
            visit_i32x4_extmul_high_i16x8_u => I32x4ExtMulHighI16x8U,
            visit_i64x2_extmul_low_i32x4_s => I64x2ExtMulLowI32x4S,
            visit_i64x2_extmul_low_i32x4_u => I64x2ExtMulLowI32x4U,
            visit_i64x2_extmul_high_i32x4_s => I64x2ExtMulHighI32x4S,
            visit_i64x2_extmul_high_i32x4_u => I64x2ExtMulHighI32x4U,
            visit_i16x8_q15mulr_sat_s => I16x8Q15MulrSatS, visit_i32x4_dot_i16x8_s => I32x4DotI16x8S,
            visit_f32x4_add => F32x4Add, visit_f32x4_sub => F32x4Sub, visit_f32x4_mul => F32x4Mul,
            visit_f32x4_div => F32x4Div, visit_f32x4_min => F32x4Min, visit_f32x4_max => F32x4Max,
            visit_f32x4_pmin => F32x4PMin, visit_f32x4_pmax => F32x4PMax, visit_f64x2_add => F64x2Add,
            visit_f64x2_sub => F64x2Sub, visit_f64x2_mul => F64x2Mul, visit_f64x2_div => F64x2Div,
            visit_f64x2_min => F64x2Min, visit_f64x2_max => F64x2Max, visit_f64x2_pmin => F64x2PMin,
            visit_f64x2_pmax => F64x2PMax, visit_i8x16_relaxed_swizzle => I8x16RelaxedSwizzle,
            visit_f32x4_relaxed_min => F32x4RelaxedMin, visit_f32x4_relaxed_max => F32x4RelaxedMax,
            visit_f64x2_relaxed_min => F64x2RelaxedMin, visit_f64x2_relaxed_max => F64x2RelaxedMax,
            visit_i16x8_relaxed_q15mulr_s => I16x8RelaxedQ15mulrS,
            visit_i16x8_relaxed_dot_i8x16_i7x16_s => I16x8RelaxedDotI8x16I7x16S,
        }
        fixed [S128, S128, S128] => [S128] {
            visit_v128_bitselect => V128Bitselect,
            visit_f32x4_relaxed_madd => F32x4RelaxedMadd, visit_f32x4_relaxed_nmadd => F32x4RelaxedNmadd,
            visit_f64x2_relaxed_madd => F64x2RelaxedMadd, visit_f64x2_relaxed_nmadd => F64x2RelaxedNmadd,
            visit_i8x16_relaxed_laneselect => I8x16RelaxedLaneselect,
            visit_i16x8_relaxed_laneselect => I16x8RelaxedLaneselect,
            visit_i32x4_relaxed_laneselect => I32x4RelaxedLaneselect,
            visit_i64x2_relaxed_laneselect => I64x2RelaxedLaneselect,
            visit_i32x4_relaxed_dot_i8x16_i7x16_add_s => I32x4RelaxedDotI8x16I7x16AddS,
        }
    }

    fn visit_i8x16_shuffle(&mut self, lanes: [u8; 16]) -> Result<()> {
        let index = self.push128(Operand128::<[u8; 16]>::new(lanes))?;
        self.emit(&[ValueLane::S128, ValueLane::S128], &[ValueLane::S128], Instruction::I8x16Shuffle(index))
    }

    fn visit_v128_const(&mut self, value: wasmparser::V128) -> Result<()> {
        let instruction = if self.emitter.optimizations_enabled()
            && let Ok(value) = u32::try_from(u128::from_le_bytes(*value.bytes()))
        {
            Instruction::Const128Imm(value)
        } else {
            Instruction::Const128(self.push128(Operand128::<[u8; 16]>::new(*value.bytes()))?)
        };
        self.emit(&[], &[ValueLane::S128], instruction)
    }
}

impl<'a> FunctionBuilder<'a> {
    fn emit_cast_branch(
        &mut self,
        relative_depth: u32,
        target: wasmparser::RefType,
        branch_on_fail: bool,
    ) -> Result<()> {
        self.pop_expect(ValueLane::S32)?;
        let target = convert_heap_type(target.heap_type(), target.is_nullable())?;
        let fallthrough = self.emitter.new_label();
        let operand = self.emitter.data.push_target64(Operand64::<(u32, u32)>::new(0, target.to_bits()))?;
        self.emitter.branch(
            if branch_on_fail { Instruction::BrOnCastFail(operand) } else { Instruction::BrOnCast(operand) },
            fallthrough,
        )?;
        self.push_sizes(&[ValueLane::S32])?;
        self.emit_branch_transfer(self.branch_destination(relative_depth)?)?;
        self.emitter.bind(fallthrough)?;
        Ok(())
    }

    fn is_unreachable(&self) -> bool {
        self.control_stack.last().is_none_or(|frame| frame.unreachable)
    }

    fn get_ctx_idx(&self, depth: u32) -> Result<usize> {
        self.control_stack
            .len()
            .checked_sub(depth as usize)
            .and_then(|index| index.checked_sub(1))
            .ok_or(crate::ParseError::IndexOutOfBounds { kind: IndexKind::Label, index: depth })
    }

    fn local(&self, idx: u32) -> Result<(ValueLane, u16)> {
        self.locals
            .get(idx as usize)
            .copied()
            .ok_or(crate::ParseError::IndexOutOfBounds { kind: IndexKind::Local, index: idx })
    }

    /// Increments and returns a physical lane count, rejecting counts too large to encode.
    fn increment_lane(counts: &mut ValueCounts, lane: ValueLane) -> Result<u16> {
        let count = match lane {
            ValueLane::S32 => &mut counts.c32,
            ValueLane::S64 => &mut counts.c64,
            ValueLane::S128 => &mut counts.c128,
        };
        *count = count
            .checked_add(1)
            .ok_or_else(|| crate::ParseError::Other("logical operand lane count is too large".into()))?;
        Ok(*count)
    }

    /// Pushes logical operands while maintaining the lane counts used by `DropKeep` and their
    /// maximum, which the runtime reserves when it enters the function.
    fn push_sizes(&mut self, sizes: &[ValueLane]) -> Result<()> {
        for &size in sizes {
            let count = Self::increment_lane(&mut self.lane_counts, size)?;
            match size {
                ValueLane::S32 => self.max_lane_counts.c32 = self.max_lane_counts.c32.max(count),
                ValueLane::S64 => self.max_lane_counts.c64 = self.max_lane_counts.c64.max(count),
                ValueLane::S128 => self.max_lane_counts.c128 = self.max_lane_counts.c128.max(count),
            }
            self.operand_stack.push(size);
        }
        Ok(())
    }

    /// Pops an operand, allowing a polymorphic value at an unreachable frame base.
    fn pop_expect(&mut self, expected: ValueLane) -> Result<()> {
        let frame_height = self.control_stack.last().map_or(0, |frame| frame.height);
        if self.operand_stack.len() == frame_height && self.is_unreachable() {
            return Ok(());
        }
        let actual = self
            .operand_stack
            .pop()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        if actual != expected {
            return Err(crate::ParseError::Other("logical operand width mismatch".into()));
        }
        match actual {
            ValueLane::S32 => self.lane_counts.c32 -= 1,
            ValueLane::S64 => self.lane_counts.c64 -= 1,
            ValueLane::S128 => self.lane_counts.c128 -= 1,
        }
        Ok(())
    }

    /// Applies a declared logical stack effect in WebAssembly operand order.
    fn apply_effect(&mut self, inputs: &[ValueLane], outputs: &[ValueLane]) -> Result<()> {
        inputs.iter().rev().try_for_each(|&size| self.pop_expect(size))?;
        self.push_sizes(outputs)?;
        Ok(())
    }

    /// Applies an instruction's stack effect before adding it to the bytecode.
    fn emit(&mut self, inputs: &[ValueLane], outputs: &[ValueLane], instruction: Instruction) -> Result<()> {
        self.apply_effect(inputs, outputs)?;
        self.emitter.emit(instruction)
    }

    /// Applies a call's stack effects and records its accumulator results.
    fn emit_call(
        &mut self,
        inputs: &[ValueLane],
        outputs: &[ValueLane],
        instruction: Instruction,
        selector: Option<ValueLane>,
    ) -> Result<()> {
        if let Some(lane) = selector {
            self.pop_expect(lane)?;
        }
        self.apply_effect(inputs, outputs)?;
        let mut lanes = tinywasm_types::ValueLanes::EMPTY;
        for lane in inputs.iter().copied().chain(selector) {
            lanes = lanes.with(lane);
        }
        self.emitter.emit_call(instruction, lanes, outputs)
    }

    /// Applies stack effects and seals a helper's materialized-stack boundary.
    fn emit_boundary(&mut self, inputs: &[ValueLane], outputs: &[ValueLane], instruction: Instruction) -> Result<()> {
        self.apply_effect(inputs, outputs)?;
        self.emitter.emit_boundary(instruction)
    }

    /// Applies logical stack effects before invoking a statically selected rule family.
    #[inline]
    fn emit_with(
        &mut self,
        inputs: &[ValueLane],
        outputs: &[ValueLane],
        instruction: Instruction,
        rules: impl FnOnce(&mut crate::emitter::PendingTail, &mut FunctionDataBuilder) -> Result<()>,
    ) -> Result<()> {
        self.apply_effect(inputs, outputs)?;
        self.emitter.emit_with(instruction, rules)
    }

    /// Restores both logical operand order and lane counts to a control-frame base.
    fn reset_stack(&mut self, height: usize, base: ValueCounts) {
        self.operand_stack.truncate(height);
        self.lane_counts = base;
    }

    /// Marks the current path unreachable and restores its entry stack.
    fn mark_unreachable(&mut self) {
        if let Some(frame) = self.control_stack.last_mut() {
            frame.unreachable = true;
            let height = frame.height;
            let base = frame.base;
            self.reset_stack(height, base);
        }
    }

    fn block_signature(&self, ty: wasmparser::BlockType) -> Result<BlockSignature<'a>> {
        Ok(match ty {
            wasmparser::BlockType::Empty => BlockSignature::Empty,
            wasmparser::BlockType::Type(ty) => BlockSignature::Result(value_lane(ty)),
            wasmparser::BlockType::FuncType(idx) => BlockSignature::Function(self.metadata.signature(idx)?),
        })
    }

    /// Enters a control frame with its parameters restored above the saved base.
    fn push_control(&mut self, kind: BlockKind, ty: wasmparser::BlockType, else_entry: Option<LabelId>) -> Result<()> {
        let signature = self.block_signature(ty)?;
        for &size in signature.params().iter().rev() {
            self.pop_expect(size)?;
        }
        let height = self.operand_stack.len();
        let base = self.lane_counts;
        self.push_sizes(signature.params())?;
        let entry_unreachable = self.is_unreachable();
        // Backedges are not known yet, so loop headers always seal the tail.
        // Other constructs have no entry target and allocate exits on first use.
        let loop_start = if matches!(kind, BlockKind::Loop) {
            let start = self.emitter.new_label();
            self.emitter.bind(start)?;
            Some(start)
        } else {
            None
        };
        self.control_stack.push(ControlFrame {
            kind,
            has_else: false,
            loop_start,
            end: None,
            else_entry,
            height,
            base,
            signature,
            unreachable: entry_unreachable,
            entry_unreachable,
            end_reachable: false,
        });
        Ok(())
    }

    /// Emits the stack-shaping instruction required by a branch.
    fn emit_dropkeep(&mut self, base: ValueCounts, keep: ValueCounts) -> Result<()> {
        if Some(self.lane_counts.c32) != base.c32.checked_add(keep.c32) {
            self.emitter.emit(Instruction::DropKeep32 { base: base.c32, keep: keep.c32 })?;
        }
        if Some(self.lane_counts.c64) != base.c64.checked_add(keep.c64) {
            self.emitter.emit(Instruction::DropKeep64 { base: base.c64, keep: keep.c64 })?;
        }
        if Some(self.lane_counts.c128) != base.c128.checked_add(keep.c128) {
            self.emitter.emit(Instruction::DropKeep128 { base: base.c128, keep: keep.c128 })?;
        }
        Ok(())
    }

    fn needs_dropkeep(&self, base: ValueCounts, keep: ValueCounts) -> bool {
        Some(self.lane_counts.c32) != base.c32.checked_add(keep.c32)
            || Some(self.lane_counts.c64) != base.c64.checked_add(keep.c64)
            || Some(self.lane_counts.c128) != base.c128.checked_add(keep.c128)
    }

    fn value_counts(sizes: &[ValueLane]) -> ValueCounts {
        let mut counts = ValueCounts::default();
        for size in sizes {
            match size {
                ValueLane::S32 => counts.c32 += 1,
                ValueLane::S64 => counts.c64 += 1,
                ValueLane::S128 => counts.c128 += 1,
            }
        }
        counts
    }

    fn branch_destination(&self, depth: u32) -> Result<BranchDestination> {
        let frame = self.get_ctx_idx(depth)?;
        let shape = if self.is_unreachable() {
            None
        } else {
            let ctx = &self.control_stack[frame];
            let types =
                if matches!(ctx.kind, BlockKind::Loop) { ctx.signature.params() } else { ctx.signature.results() };
            let keep = Self::value_counts(types);
            self.needs_dropkeep(ctx.base, keep).then_some((ctx.base, keep))
        };
        Ok(BranchDestination { frame, shape })
    }

    fn branch_label(&mut self, frame: usize) -> Option<LabelId> {
        let ctx = &mut self.control_stack[frame];
        match ctx.kind {
            BlockKind::Function => None,
            BlockKind::Loop => Some(ctx.loop_start.unwrap()),
            BlockKind::Block | BlockKind::If | BlockKind::TryTable => {
                ctx.end_reachable = true;
                Some(*ctx.end.get_or_insert_with(|| self.emitter.new_label()))
            }
        }
    }

    fn emit_branch_shape(&mut self, destination: &BranchDestination) -> Result<()> {
        if let Some((base, keep)) = destination.shape {
            self.emit_dropkeep(base, keep)?;
        }
        Ok(())
    }

    fn emit_branch_transfer(&mut self, destination: BranchDestination) -> Result<()> {
        self.emit_branch_shape(&destination)?;
        self.emit_jump_or_return(destination.frame)
    }

    fn emit_jump_or_return(&mut self, frame: usize) -> Result<()> {
        if let Some(label) = self.branch_label(frame) {
            self.emitter.branch(Instruction::Jump(0), label)
        } else {
            self.emitter.emit_boundary(Instruction::Return)
        }
    }
}
