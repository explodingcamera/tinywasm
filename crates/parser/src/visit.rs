use crate::{
    Result,
    conversion::{convert_heap_type, value_lane},
    macros::visit::*,
};
use alloc::{boxed::Box, collections::BTreeMap, string::ToString, vec::Vec};
use tinywasm_types::{
    BinOp, BranchTableOperand, CmpOp, CompactMemoryArg, ConvertOp32To64, ConvertOp64To32, ExceptionHandler, Global,
    Import, ImportKind, Instruction, IntBinOp, LoadOp32, LoadOp64, LocalTripleArg, LocalUpdateCmpOperand,
    LocalUpdateOperand, MemoryLocalArg, MemoryOperand, MemoryType, Operand64, Operand64Idx, Operand128, Operand128Idx,
    PackedOp, StorageType, TableDefinition, TagType, TargetLocalArg, TypeSection, UnaryOp32, UnaryOp64, ValueCounts,
    ValueLane, WasmFunctionData,
};
use wasmparser::{FunctionBody, OperatorsReader, OperatorsReaderAllocations, VisitSimdOperator};

mod labels;
mod register;
mod selector;

use labels::{LabelId, LabelRegistry};
use selector::{I32BranchSelection, InstructionSelector};

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

struct ControlFrame {
    kind: BlockKind,
    has_else: bool,
    start_label: LabelId,
    end_label: LabelId,
    else_label: Option<LabelId>,
    height: usize,
    base: ValueCounts,
    params: Vec<ValueLane>,
    results: Vec<ValueLane>,
    unreachable: bool,
    entry_unreachable: bool,
    end_reachable: bool,
}

#[derive(Clone)]
pub(crate) struct Signature {
    pub params: Vec<ValueLane>,
    pub(crate) params_numeric32: Vec<bool>,
    results: Vec<ValueLane>,
    results_reference: Vec<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueLocation {
    Stack(ValueLane),
    Accumulator32,
    Accumulator64,
    AccumulatorRef,
    LocalNumeric32(u16),
    LocalNumeric64(u16),
    LocalRef(u16),
    ConstNumeric32(i32),
    ConstNumeric64(i64),
    DeferredBinLocalLocal32 { op: BinOp, left: u16, right: u16 },
    DeferredBinLocalConst32 { op: BinOp, local: u16, value: i32 },
    DeferredBinLocalLocal64 { op: BinOp, left: u16, right: u16 },
    DeferredBinLocalConst64 { op: BinOp, local: u16, value: i64 },
}

impl ValueLocation {
    fn lane(self) -> ValueLane {
        match self {
            Self::Stack(lane) => lane,
            Self::Accumulator32
            | Self::AccumulatorRef
            | Self::LocalNumeric32(_)
            | Self::LocalRef(_)
            | Self::ConstNumeric32(_)
            | Self::DeferredBinLocalLocal32 { .. }
            | Self::DeferredBinLocalConst32 { .. } => ValueLane::S32,
            Self::Accumulator64
            | Self::LocalNumeric64(_)
            | Self::ConstNumeric64(_)
            | Self::DeferredBinLocalLocal64 { .. }
            | Self::DeferredBinLocalConst64 { .. } => ValueLane::S64,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ValueType {
    Numeric32,
    Numeric64,
    Reference,
    Vector,
}

impl ValueType {
    fn lane(self) -> ValueLane {
        match self {
            Self::Numeric32 | Self::Reference => ValueLane::S32,
            Self::Numeric64 => ValueLane::S64,
            Self::Vector => ValueLane::S128,
        }
    }

    fn is_numeric(self) -> bool {
        matches!(self, Self::Numeric32 | Self::Numeric64)
    }

    fn is_reference(self) -> bool {
        self == Self::Reference
    }
}

impl From<&tinywasm_types::WasmType> for ValueType {
    fn from(value: &tinywasm_types::WasmType) -> Self {
        match value {
            tinywasm_types::WasmType::I32 | tinywasm_types::WasmType::F32 => Self::Numeric32,
            tinywasm_types::WasmType::I64 | tinywasm_types::WasmType::F64 => Self::Numeric64,
            tinywasm_types::WasmType::Ref(_) => Self::Reference,
            tinywasm_types::WasmType::V128 => Self::Vector,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegisterOp32 {
    Bin(tinywasm_types::BinOp),
    I32Cmp(tinywasm_types::CmpOp),
    F32Cmp(tinywasm_types::CmpOp),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RegisterOp64 {
    Bin(tinywasm_types::BinOp),
    I64Cmp(tinywasm_types::CmpOp),
    F64Cmp(tinywasm_types::CmpOp),
}

impl RegisterOp32 {
    fn is_commutative(self) -> bool {
        matches!(
            self,
            Self::Bin(BinOp::IAdd | BinOp::IMul | BinOp::IAnd | BinOp::IOr | BinOp::IXor)
                | Self::I32Cmp(tinywasm_types::CmpOp::Eq | tinywasm_types::CmpOp::Ne)
                | Self::F32Cmp(tinywasm_types::CmpOp::Eq | tinywasm_types::CmpOp::Ne)
        )
    }
}

impl RegisterOp64 {
    fn is_commutative(self) -> bool {
        matches!(
            self,
            Self::Bin(BinOp::IAdd | BinOp::IMul | BinOp::IAnd | BinOp::IOr | BinOp::IXor)
                | Self::I64Cmp(tinywasm_types::CmpOp::Eq | tinywasm_types::CmpOp::Ne)
                | Self::F64Cmp(tinywasm_types::CmpOp::Eq | tinywasm_types::CmpOp::Ne)
        )
    }

    fn result_lane(self) -> ValueLane {
        match self {
            Self::Bin(_) => ValueLane::S64,
            Self::I64Cmp(_) | Self::F64Cmp(_) => ValueLane::S32,
        }
    }
}

pub(crate) struct ModuleMetadata {
    signatures: Vec<Option<Signature>>,
    functions: Vec<u32>,
    globals: Vec<ValueType>,
    memories: Vec<ValueLane>,
    tables: Vec<ValueLane>,
    tags: Vec<u32>,
    aggregate_fields: Vec<AggregateFields>,
    imported_memories: u32,
}

struct FunctionDataBuilder {
    pub(crate) operands64: Vec<Operand64>,
    pub(crate) operands128: Vec<Operand128>,
    pub(crate) branch_table_targets: Vec<u32>,
    pub(crate) exception_handlers: Vec<ExceptionHandler>,
    deduplicate64: Option<BTreeMap<Operand64, u32>>,
    deduplicate128: Option<BTreeMap<Operand128, u32>>,
}

impl FunctionDataBuilder {
    fn new(deduplicate_operands: bool) -> Self {
        Self {
            operands64: Vec::new(),
            operands128: Vec::new(),
            branch_table_targets: Vec::new(),
            exception_handlers: Vec::new(),
            deduplicate64: deduplicate_operands.then(BTreeMap::new),
            deduplicate128: deduplicate_operands.then(BTreeMap::new),
        }
    }

    fn push_operand64<T>(&mut self, operand: Operand64<T>) -> Result<Operand64Idx<T>> {
        let operand = operand.cast();
        if let Some(index) = self.deduplicate64.as_ref().and_then(|map| map.get(&operand)) {
            return Ok(Operand64Idx::new(*index));
        }
        let index = self.operands64.len() as u32;
        self.operands64.push(operand);
        if let Some(map) = &mut self.deduplicate64 {
            map.insert(operand, index);
        }
        Ok(Operand64Idx::new(index))
    }

    fn push_operand128<T>(&mut self, operand: Operand128<T>) -> Result<Operand128Idx<T>> {
        let operand = operand.cast();
        if let Some(index) = self.deduplicate128.as_ref().and_then(|map| map.get(&operand)) {
            return Ok(Operand128Idx::new(*index));
        }
        let index = self.operands128.len() as u32;
        self.operands128.push(operand);
        if let Some(map) = &mut self.deduplicate128 {
            map.insert(operand, index);
        }
        Ok(Operand128Idx::new(index))
    }

    fn push_target_operand64<T>(&mut self, operand: Operand64<T>) -> Result<Operand64Idx<T>> {
        self.operands64.push(operand.cast());
        Ok(Operand64Idx::new(self.operands64.len() as u32 - 1))
    }

    fn push_target_operand128<T>(&mut self, operand: Operand128<T>) -> Result<Operand128Idx<T>> {
        self.operands128.push(operand.cast());
        Ok(Operand128Idx::new(self.operands128.len() as u32 - 1))
    }

    fn operand128<T>(&self, index: Operand128Idx<T>) -> Operand128<T> {
        self.operands128[index.index()].cast()
    }

    fn operand64<T>(&self, index: Operand64Idx<T>) -> Operand64<T> {
        self.operands64[index.index()].cast()
    }

    fn finish(self) -> WasmFunctionData {
        WasmFunctionData {
            operands64: self.operands64.into_boxed_slice(),
            operands128: self.operands128.into_boxed_slice(),
            branch_table_targets: self.branch_table_targets.into_boxed_slice(),
            exception_handlers: self.exception_handlers.into_boxed_slice(),
        }
    }
}

enum AggregateFields {
    Other,
    Struct(Box<[ValueType]>),
    Array(ValueType),
}

pub(crate) struct FunctionBuilder<'a> {
    instructions: InstructionSelector,
    data: FunctionDataBuilder,
    labels: LabelRegistry,
    control_stack: Vec<ControlFrame>,
    operand_stack: Vec<ValueLocation>,
    accumulator32_owner: Option<usize>,
    accumulator64_owner: Option<usize>,
    accumulator_ref_owner: Option<usize>,
    lane_counts: ValueCounts,
    function_result_reference: bool,
    function_index: u32,
    metadata: &'a ModuleMetadata,
    local_types: Vec<ValueLane>,
    local_numeric32: Vec<bool>,
    local_addr_map: Vec<u16>,
    uses_local_memory: bool,
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn new(
        metadata: &'a ModuleMetadata,
        function_index: u32,
        signature: &Signature,
        local_types: Vec<ValueLane>,
        local_numeric32: Vec<bool>,
        local_addr_map: Vec<u16>,
        body_size: usize,
        deduplicate_operands: bool,
    ) -> Self {
        let mut labels = LabelRegistry::default();
        let start_label = labels.new_label();
        labels.pin(start_label, 0).expect("initial function label is valid");
        let end_label = labels.new_label();
        Self {
            local_types,
            local_numeric32,
            local_addr_map,
            metadata,
            function_index,
            instructions: InstructionSelector::with_capacity(body_size.min(1024)),
            data: FunctionDataBuilder::new(deduplicate_operands),
            labels,
            control_stack: alloc::vec![ControlFrame {
                kind: BlockKind::Function,
                has_else: false,
                start_label,
                end_label,
                else_label: None,
                height: 0,
                base: ValueCounts::default(),
                params: Vec::new(),
                results: signature.results.clone(),
                unreachable: false,
                entry_unreachable: false,
                end_reachable: false,
            }],
            operand_stack: Vec::new(),
            accumulator32_owner: None,
            accumulator64_owner: None,
            accumulator_ref_owner: None,
            lane_counts: ValueCounts::default(),
            function_result_reference: signature.results_reference.as_slice() == [true],
            uses_local_memory: false,
        }
    }

    fn mark_memory(&mut self, memory: u32) {
        self.uses_local_memory |= memory >= self.metadata.imported_memories;
    }

    fn finish(mut self) -> Result<(Vec<Instruction>, WasmFunctionData, bool)> {
        let mut instructions = self.instructions.finish();
        self.labels.resolve(&mut instructions, &mut self.data)?;
        Ok((instructions, self.data.finish(), self.uses_local_memory))
    }

    fn visit_struct_get_impl(
        &mut self,
        type_index: u32,
        field_index: u32,
        instruction: fn(Operand64Idx<(u32, u32)>) -> Instruction,
    ) -> Result<()> {
        let size = self.metadata.struct_field(type_index, field_index)?;
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, field_index))?;
        self.emit(&[ValueLane::S32], &[size], instruction(operand))
    }

    fn push_operand64<T>(&mut self, operand: Operand64<T>) -> Result<Operand64Idx<T>> {
        self.data.push_operand64(operand)
    }

    fn push_operand128<T>(&mut self, operand: Operand128<T>) -> Result<Operand128Idx<T>> {
        self.data.push_operand128(operand)
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
        let mut global_types = Vec::with_capacity(imports.len() + globals.len());
        let mut memory_sizes = Vec::with_capacity(imports.len() + memories.len());
        let mut table_sizes = Vec::with_capacity(imports.len() + tables.len());
        let mut tag_types = Vec::with_capacity(imports.len() + tags.len());
        let imported_memories =
            imports.iter().filter(|import| matches!(import.kind, ImportKind::Memory(_))).count() as u32;

        for import in imports {
            match &import.kind {
                ImportKind::Function(ty) => functions.push(*ty),
                ImportKind::Global(ty) => global_types.push(ValueType::from(&ty.ty)),
                ImportKind::Memory(ty) => memory_sizes.push(Self::address_lane(ty.arch())),
                ImportKind::Table(ty) => table_sizes.push(Self::address_lane(ty.arch())),
                ImportKind::Tag(ty) => tag_types.push(ty.type_idx),
            }
        }

        functions.extend_from_slice(code_type_addrs);
        global_types.extend(globals.iter().map(|global| ValueType::from(&global.ty.ty)));
        memory_sizes.extend(memories.iter().map(|ty| Self::address_lane(ty.arch())));
        table_sizes.extend(tables.iter().map(|table| Self::address_lane(table.ty.arch())));
        tag_types.extend(tags.iter().map(|tag| tag.type_idx));

        let signatures = types
            .types
            .iter()
            .map(|ty| {
                ty.as_func().map(|ty| Signature {
                    params: ty.params().iter().map(ValueLane::from).collect(),
                    params_numeric32: ty
                        .params()
                        .iter()
                        .map(|ty| matches!(ty, tinywasm_types::WasmType::I32 | tinywasm_types::WasmType::F32))
                        .collect(),
                    results: ty.results().iter().map(ValueLane::from).collect(),
                    results_reference: ty
                        .results()
                        .iter()
                        .map(|ty| matches!(ty, tinywasm_types::WasmType::Ref(_)))
                        .collect(),
                })
            })
            .collect();
        let aggregate_fields = types
            .types
            .iter()
            .map(|ty| {
                if let Some(ty) = ty.as_struct() {
                    AggregateFields::Struct(ty.fields.iter().map(|field| Self::storage_type(field.storage)).collect())
                } else if let Some(ty) = ty.as_array() {
                    AggregateFields::Array(Self::storage_type(ty.field.storage))
                } else {
                    AggregateFields::Other
                }
            })
            .collect();
        Self {
            signatures,
            functions,
            globals: global_types,
            memories: memory_sizes,
            tables: table_sizes,
            tags: tag_types,
            aggregate_fields,
            imported_memories,
        }
    }

    pub(crate) fn signature(&self, idx: u32) -> Result<&Signature> {
        self.signatures
            .get(idx as usize)
            .and_then(Option::as_ref)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("type index is not a function type: {idx}")))
    }

    fn function_signature(&self, idx: u32) -> Result<&Signature> {
        let ty = *self
            .functions
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("function index out of bounds: {idx}")))?;
        self.signature(ty)
    }

    fn tag_signature(&self, idx: u32) -> Result<&Signature> {
        let ty = *self
            .tags
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("tag index out of bounds: {idx}")))?;
        self.signature(ty)
    }

    fn global_size(&self, idx: u32) -> Result<ValueLane> {
        Ok(self.global_type(idx)?.lane())
    }

    fn global_is_numeric(&self, idx: u32) -> Result<bool> {
        Ok(self.global_type(idx)?.is_numeric())
    }

    fn global_is_reference(&self, idx: u32) -> Result<bool> {
        Ok(self.global_type(idx)?.is_reference())
    }

    fn global_type(&self, idx: u32) -> Result<ValueType> {
        self.globals
            .get(idx as usize)
            .copied()
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("global index out of bounds: {idx}")))
    }

    fn memory_size(&self, idx: u32) -> Result<ValueLane> {
        Self::indexed_size(&self.memories, "memory", idx)
    }

    fn table_size(&self, idx: u32) -> Result<ValueLane> {
        Self::indexed_size(&self.tables, "table", idx)
    }

    fn indexed_size(sizes: &[ValueLane], entity: &str, idx: u32) -> Result<ValueLane> {
        sizes
            .get(idx as usize)
            .copied()
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("{entity} index out of bounds: {idx}")))
    }

    fn storage_type(storage: StorageType) -> ValueType {
        match storage {
            StorageType::I8 | StorageType::I16 => ValueType::Numeric32,
            StorageType::Value(ref ty) => ValueType::from(ty),
        }
    }

    fn struct_fields(&self, idx: u32) -> Result<&[ValueType]> {
        self.aggregate_fields
            .get(idx as usize)
            .and_then(|fields| match fields {
                AggregateFields::Struct(fields) => Some(fields.as_ref()),
                AggregateFields::Other | AggregateFields::Array(_) => None,
            })
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("type index is not a struct type: {idx}")))
    }

    fn struct_field(&self, type_index: u32, field_index: u32) -> Result<ValueLane> {
        self.struct_fields(type_index)?
            .get(field_index as usize)
            .map(|ty| ty.lane())
            .ok_or_else(|| crate::ParseError::Other("struct field index out of bounds".into()))
    }

    fn array_field(&self, idx: u32) -> Result<ValueLane> {
        self.aggregate_fields
            .get(idx as usize)
            .and_then(|fields| match fields {
                AggregateFields::Array(field) => Some(field.lane()),
                AggregateFields::Other | AggregateFields::Struct(_) => None,
            })
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("type index is not an array type: {idx}")))
    }
}

#[cfg(feature = "validate")]
impl<'a> VisitOperator<'a> for ValidateThenVisit<'_, '_> {
    type Output = Result<()>;

    #[cfg(not(rust_analyzer))] // rust analyzer gets confused and throws a bunch of errors when this macro is expanded
    wasmparser::for_each_visit_operator!(validate_then_visit);

    fn simd_visitor(&mut self) -> Option<&mut dyn VisitSimdOperator<'a, Output = Self::Output>> {
        Some(self)
    }
}

#[cfg(feature = "validate")]
impl VisitSimdOperator<'_> for ValidateThenVisit<'_, '_> {
    #[cfg(not(rust_analyzer))] // rust analyzer gets confused and throws a bunch of errors when this macro is expanded
    wasmparser::for_each_visit_simd_operator!(validate_then_visit_simd);
}

pub(crate) fn process_operators(
    body: FunctionBody<'_>,
    locals: (Vec<ValueLane>, Vec<bool>, Vec<u16>),
    metadata: &ModuleMetadata,
    function_index: u32,
    ty_idx: u32,
    allocs: OperatorsReaderAllocations,
    deduplicate_operands: bool,
) -> Result<(Vec<Instruction>, WasmFunctionData, bool, OperatorsReaderAllocations)> {
    let (local_types, local_numeric32, local_addr_map) = locals;
    let body_size = body.as_bytes().len();
    let reader = body.get_binary_reader_for_operators()?;
    let mut reader = OperatorsReader::new_with_allocs(reader, allocs);
    let signature = metadata.signature(ty_idx)?;
    let mut builder = FunctionBuilder::new(
        metadata,
        function_index,
        signature,
        local_types,
        local_numeric32,
        local_addr_map,
        body_size,
        deduplicate_operands,
    );

    while !reader.eof() {
        if let Err(error) = reader.visit_operator(&mut builder).map_err(crate::ParseError::from).flatten() {
            core::hint::cold_path();
            return Err(error);
        }
    }

    reader.finish()?;
    let (instructions, data, uses_local_memory) = builder.finish()?;
    Ok((instructions, data, uses_local_memory, reader.into_allocations()))
}

#[cfg(feature = "validate")]
pub(crate) fn process_operators_and_validate(
    mut validator: FuncValidator<ValidatorResources>,
    body: FunctionBody<'_>,
    locals: (Vec<ValueLane>, Vec<bool>, Vec<u16>),
    metadata: &ModuleMetadata,
    function_index: u32,
    ty_idx: u32,
    allocs: OperatorsReaderAllocations,
    deduplicate_operands: bool,
) -> Result<(Vec<Instruction>, WasmFunctionData, bool, FuncValidatorAllocations, OperatorsReaderAllocations)> {
    let (local_types, local_numeric32, local_addr_map) = locals;
    let body_size = body.as_bytes().len();
    let reader = body.get_binary_reader_for_operators()?;
    let mut reader = OperatorsReader::new_with_allocs(reader, allocs);
    let signature = metadata.signature(ty_idx)?;
    let mut builder = FunctionBuilder::new(
        metadata,
        function_index,
        signature,
        local_types,
        local_numeric32,
        local_addr_map,
        body_size,
        deduplicate_operands,
    );

    while !reader.eof() {
        let position = reader.original_position();
        let result = reader
            .visit_operator(&mut ValidateThenVisit { validator: &mut validator, builder: &mut builder, position })
            .map_err(crate::ParseError::from)
            .flatten();

        if let Err(error) = result {
            core::hint::cold_path();
            return Err(error);
        }
    }

    reader.finish()?;
    let (instructions, data, uses_local_memory) = builder.finish()?;
    Ok((instructions, data, uses_local_memory, validator.into_allocations(), reader.into_allocations()))
}

impl<'a> wasmparser::VisitOperator<'a> for FunctionBuilder<'_> {
    type Output = Result<()>;

    fn simd_visitor(&mut self) -> Option<&mut dyn VisitSimdOperator<'a, Output = Self::Output>> {
        Some(self)
    }

    wasmparser::for_each_visit_operator!(impl_visit_operator);

    lowering_ops! {
        memory [Addr] => [S32] {
            visit_i32_load => I32Load, visit_f32_load => F32Load, visit_i32_load8_s => I32Load8S,
            visit_i32_load8_u => I32Load8U, visit_i32_load16_s => I32Load16S,
            visit_i32_load16_u => I32Load16U,
        }
        memory [Addr] => [S64] {
            visit_i64_load => I64Load, visit_f64_load => F64Load, visit_i64_load8_s => I64Load8S,
            visit_i64_load8_u => I64Load8U, visit_i64_load16_s => I64Load16S,
            visit_i64_load16_u => I64Load16U, visit_i64_load32_s => I64Load32S,
            visit_i64_load32_u => I64Load32U,
        }
        memory [Addr, S32] => [] {
            visit_f32_store => F32Store, visit_i32_store8 => I32Store8,
            visit_i32_store16 => I32Store16, visit_i32_store => I32Store,
        }
        memory [Addr, S64] => [] {
            visit_f64_store => F64Store, visit_i64_store8 => I64Store8,
            visit_i64_store16 => I64Store16, visit_i64_store32 => I64Store32, visit_i64_store => I64Store,
        }
        fixed [] => [] { visit_data_drop(segment: u32) => DataDrop, visit_elem_drop(segment: u32) => ElemDrop }
        heap false [S32] => [S32] { visit_ref_cast_non_null => RefCast }
        heap true [S32] => [S32] { visit_ref_cast_nullable => RefCast }
        heap_acc32 false { visit_ref_test_non_null => AccRefTest }
        heap_acc32 true { visit_ref_test_nullable => AccRefTest }
        fixed [S32] => [S32] {
            visit_ref_as_non_null => RefAsNonNull, visit_ref_i31 => RefI31,
        }
        acc_reference32 [S32] {
            visit_i31_get_s => AccI31GetS,
            visit_i31_get_u => AccI31GetU, visit_array_len => AccArrayLen,
        }
        acc_unary32 {
            visit_i32_eqz => AccI32Eqz, I32Eqz,
            visit_i32_clz => AccI32Clz, I32Clz,
            visit_i32_ctz => AccI32Ctz, I32Ctz,
            visit_i32_popcnt => AccI32Popcnt, I32Popcnt,
            visit_i32_extend8_s => AccI32Extend8S, I32Extend8S,
            visit_i32_extend16_s => AccI32Extend16S, I32Extend16S,
            visit_i32_trunc_f32_s => AccI32TruncF32S, I32TruncF32S,
            visit_i32_trunc_f32_u => AccI32TruncF32U, I32TruncF32U,
            visit_i32_trunc_sat_f32_s => AccI32TruncSatF32S, I32TruncSatF32S,
            visit_i32_trunc_sat_f32_u => AccI32TruncSatF32U, I32TruncSatF32U,
            visit_f32_convert_i32_s => AccF32ConvertI32S, F32ConvertI32S,
            visit_f32_convert_i32_u => AccF32ConvertI32U, F32ConvertI32U,
            visit_f32_abs => AccF32Abs, F32Abs,
            visit_f32_neg => AccF32Neg, F32Neg,
            visit_f32_ceil => AccF32Ceil, F32Ceil,
            visit_f32_floor => AccF32Floor, F32Floor,
            visit_f32_trunc => AccF32Trunc, F32Trunc,
            visit_f32_nearest => AccF32Nearest, F32Nearest,
            visit_f32_sqrt => AccF32Sqrt, F32Sqrt,
        }
        effect [S32] => [S32] { visit_any_convert_extern, visit_extern_convert_any }
        acc_unary64 {
            visit_i64_clz => AccI64Clz, I64Clz,
            visit_i64_ctz => AccI64Ctz, I64Ctz,
            visit_i64_popcnt => AccI64Popcnt, I64Popcnt,
            visit_i64_extend8_s => AccI64Extend8S, I64Extend8S,
            visit_i64_extend16_s => AccI64Extend16S, I64Extend16S,
            visit_i64_extend32_s => AccI64Extend32S, I64Extend32S,
            visit_i64_trunc_f64_s => AccI64TruncF64S, I64TruncF64S,
            visit_i64_trunc_f64_u => AccI64TruncF64U, I64TruncF64U,
            visit_i64_trunc_sat_f64_s => AccI64TruncSatF64S, I64TruncSatF64S,
            visit_i64_trunc_sat_f64_u => AccI64TruncSatF64U, I64TruncSatF64U,
            visit_f64_convert_i64_s => AccF64ConvertI64S, F64ConvertI64S,
            visit_f64_convert_i64_u => AccF64ConvertI64U, F64ConvertI64U,
            visit_f64_abs => AccF64Abs, F64Abs,
            visit_f64_neg => AccF64Neg, F64Neg,
            visit_f64_ceil => AccF64Ceil, F64Ceil,
            visit_f64_floor => AccF64Floor, F64Floor,
            visit_f64_trunc => AccF64Trunc, F64Trunc,
            visit_f64_nearest => AccF64Nearest, F64Nearest,
            visit_f64_sqrt => AccF64Sqrt, F64Sqrt,
        }
        acc_cross S64 => S32 {
            visit_i64_eqz => AccI64Eqz, Instruction::AccConvertStack64To32(ConvertOp64To32::I64Eqz),
            visit_i32_wrap_i64 => AccI32WrapI64, Instruction::AccConvertStack64To32(ConvertOp64To32::I32WrapI64),
            visit_i32_trunc_f64_s => AccI32TruncF64S, Instruction::AccConvertStack64To32(ConvertOp64To32::I32TruncF64S),
            visit_i32_trunc_f64_u => AccI32TruncF64U, Instruction::AccConvertStack64To32(ConvertOp64To32::I32TruncF64U),
            visit_f32_convert_i64_s => AccF32ConvertI64S, Instruction::AccConvertStack64To32(ConvertOp64To32::F32ConvertI64S),
            visit_f32_convert_i64_u => AccF32ConvertI64U, Instruction::AccConvertStack64To32(ConvertOp64To32::F32ConvertI64U),
            visit_f32_demote_f64 => AccF32DemoteF64, Instruction::AccConvertStack64To32(ConvertOp64To32::F32DemoteF64),
            visit_i32_trunc_sat_f64_s => AccI32TruncSatF64S, Instruction::AccConvertStack64To32(ConvertOp64To32::I32TruncSatF64S),
            visit_i32_trunc_sat_f64_u => AccI32TruncSatF64U, Instruction::AccConvertStack64To32(ConvertOp64To32::I32TruncSatF64U),
        }
        acc_cross S32 => S64 {
            visit_i64_extend_i32_s => AccI64ExtendI32S, Instruction::AccConvertStack32To64(ConvertOp32To64::I64ExtendI32S),
            visit_i64_extend_i32_u => AccI64ExtendI32U, Instruction::AccConvertStack32To64(ConvertOp32To64::I64ExtendI32U),
            visit_i64_trunc_f32_s => AccI64TruncF32S, Instruction::AccConvertStack32To64(ConvertOp32To64::I64TruncF32S),
            visit_i64_trunc_f32_u => AccI64TruncF32U, Instruction::AccConvertStack32To64(ConvertOp32To64::I64TruncF32U),
            visit_f64_convert_i32_s => AccF64ConvertI32S, Instruction::AccConvertStack32To64(ConvertOp32To64::F64ConvertI32S),
            visit_f64_convert_i32_u => AccF64ConvertI32U, Instruction::AccConvertStack32To64(ConvertOp32To64::F64ConvertI32U),
            visit_f64_promote_f32 => AccF64PromoteF32, Instruction::AccConvertStack32To64(ConvertOp32To64::F64PromoteF32),
            visit_i64_trunc_sat_f32_s => AccI64TruncSatF32S, Instruction::AccConvertStack32To64(ConvertOp32To64::I64TruncSatF32S),
            visit_i64_trunc_sat_f32_u => AccI64TruncSatF32U, Instruction::AccConvertStack32To64(ConvertOp32To64::I64TruncSatF32U),
        }
        acc_reference32 [S32, S32] { visit_ref_eq => AccRefEq }
        acc_int_binary32 {
            visit_i32_div_s => DivS, visit_i32_div_u => DivU,
            visit_i32_rem_s => RemS, visit_i32_rem_u => RemU,
        }
        acc_binary32 {
            visit_i32_eq => RegisterOp32::I32Cmp(CmpOp::Eq);
            visit_i32_ne => RegisterOp32::I32Cmp(CmpOp::Ne);
            visit_i32_lt_s => RegisterOp32::I32Cmp(CmpOp::LtS);
            visit_i32_lt_u => RegisterOp32::I32Cmp(CmpOp::LtU);
            visit_i32_gt_s => RegisterOp32::I32Cmp(CmpOp::GtS);
            visit_i32_gt_u => RegisterOp32::I32Cmp(CmpOp::GtU);
            visit_i32_le_s => RegisterOp32::I32Cmp(CmpOp::LeS);
            visit_i32_le_u => RegisterOp32::I32Cmp(CmpOp::LeU);
            visit_i32_ge_s => RegisterOp32::I32Cmp(CmpOp::GeS);
            visit_i32_ge_u => RegisterOp32::I32Cmp(CmpOp::GeU);
            visit_f32_eq => RegisterOp32::F32Cmp(CmpOp::Eq);
            visit_f32_ne => RegisterOp32::F32Cmp(CmpOp::Ne);
            visit_f32_lt => RegisterOp32::F32Cmp(CmpOp::LtS);
            visit_f32_gt => RegisterOp32::F32Cmp(CmpOp::GtS);
            visit_f32_le => RegisterOp32::F32Cmp(CmpOp::LeS);
            visit_f32_ge => RegisterOp32::F32Cmp(CmpOp::GeS);
            visit_i32_add => RegisterOp32::Bin(BinOp::IAdd);
            visit_i32_sub => RegisterOp32::Bin(BinOp::ISub);
            visit_i32_mul => RegisterOp32::Bin(BinOp::IMul);
            visit_i32_and => RegisterOp32::Bin(BinOp::IAnd);
            visit_i32_or => RegisterOp32::Bin(BinOp::IOr);
            visit_i32_xor => RegisterOp32::Bin(BinOp::IXor);
            visit_i32_shl => RegisterOp32::Bin(BinOp::IShl);
            visit_i32_shr_s => RegisterOp32::Bin(BinOp::IShrS);
            visit_i32_shr_u => RegisterOp32::Bin(BinOp::IShrU);
            visit_i32_rotl => RegisterOp32::Bin(BinOp::IRotl);
            visit_i32_rotr => RegisterOp32::Bin(BinOp::IRotr);
            visit_f32_add => RegisterOp32::Bin(BinOp::FAdd);
            visit_f32_sub => RegisterOp32::Bin(BinOp::FSub);
            visit_f32_mul => RegisterOp32::Bin(BinOp::FMul);
            visit_f32_div => RegisterOp32::Bin(BinOp::FDiv);
            visit_f32_min => RegisterOp32::Bin(BinOp::FMin);
            visit_f32_max => RegisterOp32::Bin(BinOp::FMax);
            visit_f32_copysign => RegisterOp32::Bin(BinOp::FCopysign);
        }
        acc_binary64 {
            visit_i64_eq => RegisterOp64::I64Cmp(CmpOp::Eq);
            visit_i64_ne => RegisterOp64::I64Cmp(CmpOp::Ne);
            visit_i64_lt_s => RegisterOp64::I64Cmp(CmpOp::LtS);
            visit_i64_lt_u => RegisterOp64::I64Cmp(CmpOp::LtU);
            visit_i64_gt_s => RegisterOp64::I64Cmp(CmpOp::GtS);
            visit_i64_gt_u => RegisterOp64::I64Cmp(CmpOp::GtU);
            visit_i64_le_s => RegisterOp64::I64Cmp(CmpOp::LeS);
            visit_i64_le_u => RegisterOp64::I64Cmp(CmpOp::LeU);
            visit_i64_ge_s => RegisterOp64::I64Cmp(CmpOp::GeS);
            visit_i64_ge_u => RegisterOp64::I64Cmp(CmpOp::GeU);
            visit_f64_eq => RegisterOp64::F64Cmp(CmpOp::Eq);
            visit_f64_ne => RegisterOp64::F64Cmp(CmpOp::Ne);
            visit_f64_lt => RegisterOp64::F64Cmp(CmpOp::LtS);
            visit_f64_gt => RegisterOp64::F64Cmp(CmpOp::GtS);
            visit_f64_le => RegisterOp64::F64Cmp(CmpOp::LeS);
            visit_f64_ge => RegisterOp64::F64Cmp(CmpOp::GeS);
        }
        acc_int_binary64 {
            visit_i64_div_s => DivS, visit_i64_div_u => DivU,
            visit_i64_rem_s => RemS, visit_i64_rem_u => RemU,
        }
        acc_binary64 {
            visit_i64_add => RegisterOp64::Bin(BinOp::IAdd);
            visit_i64_sub => RegisterOp64::Bin(BinOp::ISub);
            visit_i64_mul => RegisterOp64::Bin(BinOp::IMul);
            visit_i64_and => RegisterOp64::Bin(BinOp::IAnd);
            visit_i64_or => RegisterOp64::Bin(BinOp::IOr);
            visit_i64_xor => RegisterOp64::Bin(BinOp::IXor);
            visit_i64_shl => RegisterOp64::Bin(BinOp::IShl);
            visit_i64_shr_s => RegisterOp64::Bin(BinOp::IShrS);
            visit_i64_shr_u => RegisterOp64::Bin(BinOp::IShrU);
            visit_i64_rotl => RegisterOp64::Bin(BinOp::IRotl);
            visit_i64_rotr => RegisterOp64::Bin(BinOp::IRotr);
            visit_f64_add => RegisterOp64::Bin(BinOp::FAdd);
            visit_f64_sub => RegisterOp64::Bin(BinOp::FSub);
            visit_f64_mul => RegisterOp64::Bin(BinOp::FMul);
            visit_f64_div => RegisterOp64::Bin(BinOp::FDiv);
            visit_f64_min => RegisterOp64::Bin(BinOp::FMin);
            visit_f64_max => RegisterOp64::Bin(BinOp::FMax);
            visit_f64_copysign => RegisterOp64::Bin(BinOp::FCopysign);
        }
        fixed [S64, S64, S64, S64] => [S64, S64] { visit_i64_add128 => I64Add128, visit_i64_sub128 => I64Sub128 }
        fixed [S64, S64] => [S64, S64] { visit_i64_mul_wide_s => I64MulWideS, visit_i64_mul_wide_u => I64MulWideU }
        effect [] => [] { visit_nop }
        effect [S32] => [S32] { visit_f32_reinterpret_i32, visit_i32_reinterpret_f32 }
        effect [S64] => [S64] { visit_f64_reinterpret_i64, visit_i64_reinterpret_f64 }
        terminating [] => [] { visit_unreachable => Unreachable }
        memory_index [Addr, S32, Addr] => [] { visit_memory_fill(memory: u32) => MemoryFill }
        table [Addr] => [S32] { visit_table_get(table: u32) => TableGet }
        table [Addr, S32] => [] { visit_table_set(table: u32) => TableSet }
        table [Addr, S32, Addr] => [] { visit_table_fill(table: u32) => TableFill }
        fixed [] => [S32] { visit_struct_new_default(type_index: u32) => StructNewDefault }
        fixed [S32] => [S32] { visit_array_new_default(type_index: u32) => ArrayNewDefault }
        array_field [Field, S32] => [S32] { visit_array_new(type_index: u32) => ArrayNew }
        array_field [S32, S32] => [Field] {
            visit_array_get(type_index: u32) => ArrayGet, visit_array_get_s(type_index: u32) => ArrayGetS,
            visit_array_get_u(type_index: u32) => ArrayGetU,
        }
        array_field [S32, S32, Field] => [] { visit_array_set(type_index: u32) => ArraySet }
        array_field [S32, S32, Field, S32] => [] { visit_array_fill(type_index: u32) => ArrayFill }
    }

    fn visit_struct_new(&mut self, type_index: u32) -> Self::Output {
        self.materialize_all_scalars();
        let field_count = self.metadata.struct_fields(type_index)?.len();
        for field_index in (0..field_count).rev() {
            let size = self.metadata.struct_field(type_index, field_index as u32)?;
            self.pop_expect(size)?;
        }
        self.push_sizes(&[ValueLane::S32])?;
        self.instructions.push(Instruction::StructNew(type_index));
        Ok(())
    }

    fn visit_memory_size(&mut self, memory: u32) -> Self::Output {
        let lane = self.metadata.memory_size(memory)?;
        self.mark_memory(memory);
        self.materialize_lane(lane);
        match lane {
            ValueLane::S32 => {
                self.push_accumulator32(lane)?;
                self.instructions.push(Instruction::AccMemorySize32(memory));
            }
            ValueLane::S64 => {
                self.push_accumulator64(lane)?;
                self.instructions.push(Instruction::AccMemorySize64(memory));
            }
            ValueLane::S128 => unreachable!(),
        }
        Ok(())
    }

    fn visit_memory_grow(&mut self, memory: u32) -> Self::Output {
        let lane = self.metadata.memory_size(memory)?;
        self.mark_memory(memory);
        match lane {
            ValueLane::S32 if self.top_is_accumulator32() => {
                self.pop_accumulator32(lane)?;
                self.push_accumulator32(lane)?;
                self.instructions.push(Instruction::AccMemoryGrow32(memory));
            }
            ValueLane::S64 if self.top_is_accumulator64() => {
                self.pop_accumulator64(lane)?;
                self.push_accumulator64(lane)?;
                self.instructions.push(Instruction::AccMemoryGrow64(memory));
            }
            ValueLane::S32 => {
                self.materialize_lane(lane);
                self.pop_stack_value(lane)?;
                self.push_accumulator32(lane)?;
                self.instructions.push(Instruction::AccMemoryGrowStack32(memory));
            }
            ValueLane::S64 => {
                self.materialize_lane(lane);
                self.pop_stack_value(lane)?;
                self.push_accumulator64(lane)?;
                self.instructions.push(Instruction::AccMemoryGrowStack64(memory));
            }
            ValueLane::S128 => unreachable!(),
        }
        Ok(())
    }

    fn visit_table_size(&mut self, table: u32) -> Self::Output {
        let lane = self.metadata.table_size(table)?;
        self.materialize_lane(lane);
        match lane {
            ValueLane::S32 => {
                self.push_accumulator32(lane)?;
                self.instructions.push(Instruction::AccTableSize32(table));
            }
            ValueLane::S64 => {
                self.push_accumulator64(lane)?;
                self.instructions.push(Instruction::AccTableSize64(table));
            }
            ValueLane::S128 => unreachable!(),
        }
        Ok(())
    }

    fn visit_table_grow(&mut self, table: u32) -> Self::Output {
        let lane = self.metadata.table_size(table)?;
        let reference_end = self.operand_stack.len().saturating_sub(usize::from(lane == ValueLane::S32));
        self.materialize_lane_prefix(ValueLane::S32, reference_end);
        match lane {
            ValueLane::S32 if self.top_is_accumulator32() => {
                self.pop_accumulator32(lane)?;
                self.materialize_lane(lane);
                self.pop_stack_value(ValueLane::S32)?;
                self.push_accumulator32(lane)?;
                self.instructions.push(Instruction::AccTableGrow32(table));
            }
            ValueLane::S64 if self.top_is_accumulator64() => {
                self.pop_accumulator64(lane)?;
                self.materialize_lane(lane);
                self.pop_stack_value(ValueLane::S32)?;
                self.push_accumulator64(lane)?;
                self.instructions.push(Instruction::AccTableGrow64(table));
            }
            ValueLane::S32 => {
                self.materialize_lane(lane);
                self.pop_stack_value(lane)?;
                self.pop_stack_value(ValueLane::S32)?;
                self.push_accumulator32(lane)?;
                self.instructions.push(Instruction::AccTableGrowStack32(table));
            }
            ValueLane::S64 => {
                self.materialize_lane(lane);
                self.pop_stack_value(lane)?;
                self.pop_stack_value(ValueLane::S32)?;
                self.push_accumulator64(lane)?;
                self.instructions.push(Instruction::AccTableGrowStack64(table));
            }
            ValueLane::S128 => unreachable!(),
        }
        Ok(())
    }

    fn visit_struct_get(&mut self, type_index: u32, field_index: u32) -> Self::Output {
        self.visit_struct_get_impl(type_index, field_index, Instruction::StructGet)
    }

    fn visit_struct_get_s(&mut self, type_index: u32, field_index: u32) -> Self::Output {
        self.visit_struct_get_impl(type_index, field_index, Instruction::StructGetS)
    }

    fn visit_struct_get_u(&mut self, type_index: u32, field_index: u32) -> Self::Output {
        self.visit_struct_get_impl(type_index, field_index, Instruction::StructGetU)
    }

    fn visit_struct_set(&mut self, type_index: u32, field_index: u32) -> Self::Output {
        let size = self.metadata.struct_field(type_index, field_index)?;
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, field_index))?;
        self.emit(&[ValueLane::S32, size], &[], Instruction::StructSet(operand))
    }

    fn visit_array_new_fixed(&mut self, type_index: u32, array_size: u32) -> Self::Output {
        self.materialize_all_scalars();
        let size = self.metadata.array_field(type_index)?;
        for _ in 0..array_size {
            self.pop_expect(size)?;
        }
        self.push_sizes(&[ValueLane::S32])?;
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, array_size))?;
        self.instructions.push(Instruction::ArrayNewFixed(operand));
        Ok(())
    }

    fn visit_call(&mut self, function_index: u32) -> Self::Output {
        let signature = self.metadata.function_signature(function_index)?;
        let instruction = if function_index == self.function_index {
            Instruction::CallSelf
        } else {
            Instruction::Call(function_index)
        };
        self.emit(&signature.params, &signature.results, instruction)
    }

    fn visit_call_indirect(&mut self, type_index: u32, table_index: u32) -> Self::Output {
        let table_size = self.metadata.table_size(table_index)?;
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, table_index))?;
        let signature = self.metadata.signature(type_index)?;
        self.pop_expect(table_size)?;
        self.emit(&signature.params, &signature.results, Instruction::CallIndirect(operand))
    }

    fn visit_call_ref(&mut self, type_index: u32) -> Self::Output {
        let signature = self.metadata.signature(type_index)?;
        self.pop_expect(ValueLane::S32)?;
        self.emit(&signature.params, &signature.results, Instruction::CallRef(type_index))
    }

    fn visit_return_call(&mut self, function_index: u32) -> Self::Output {
        self.materialize_all_scalars();
        let signature = self.metadata.function_signature(function_index)?;
        self.apply_effect(&signature.params, &[])?;
        self.mark_unreachable();
        self.instructions.push(if function_index == self.function_index {
            Instruction::ReturnCallSelf
        } else {
            Instruction::ReturnCall(function_index)
        });
        Ok(())
    }

    fn visit_return_call_indirect(&mut self, type_index: u32, table_index: u32) -> Self::Output {
        self.materialize_all_scalars();
        let table_size = self.metadata.table_size(table_index)?;
        let signature = self.metadata.signature(type_index)?;
        self.pop_expect(table_size)?;
        self.apply_effect(&signature.params, &[])?;
        self.mark_unreachable();
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, table_index))?;
        self.instructions.push(Instruction::ReturnCallIndirect(operand));
        Ok(())
    }

    fn visit_return_call_ref(&mut self, type_index: u32) -> Self::Output {
        self.materialize_all_scalars();
        let signature = self.metadata.signature(type_index)?;
        self.pop_expect(ValueLane::S32)?;
        self.apply_effect(&signature.params, &[])?;
        self.mark_unreachable();
        self.instructions.push(Instruction::ReturnCallRef(type_index));
        Ok(())
    }

    fn visit_ref_func(&mut self, function: u32) -> Self::Output {
        self.materialize_accumulator_ref();
        self.push_accumulator_ref()?;
        self.instructions.push(Instruction::AccRefFunc(function));
        Ok(())
    }

    fn visit_ref_null(&mut self, heap_type: wasmparser::HeapType) -> Self::Output {
        _ = convert_heap_type(heap_type, false)?;
        self.materialize_accumulator_ref();
        self.push_accumulator_ref()?;
        self.instructions.push(Instruction::AccRefNull);
        Ok(())
    }

    fn visit_ref_is_null(&mut self) -> Self::Output {
        self.materialize_accumulator32();
        if self.load_top_reference_into_accumulator()? {
            self.pop_accumulator_ref()?;
            self.push_accumulator32(ValueLane::S32)?;
            self.instructions.push(Instruction::AccRefIsNull);
            return Ok(());
        }
        self.emit_accumulator_reference32(&[ValueLane::S32], Instruction::AccRefIsNullStack)
    }

    fn visit_global_set(&mut self, global_index: u32) -> Self::Output {
        let size = self.metadata.global_size(global_index)?;
        if self.metadata.global_is_reference(global_index)? && self.top_is_accumulator_ref() {
            self.pop_accumulator_ref()?;
            self.instructions.push(Instruction::AccRefGlobalSet(global_index));
            return Ok(());
        }
        if size == ValueLane::S32 && self.top_is_accumulator32() {
            self.pop_accumulator32(ValueLane::S32)?;
            self.instructions.push(Instruction::AccGlobalSet32(global_index));
            return Ok(());
        }
        if size == ValueLane::S64 && self.top_is_accumulator64() {
            self.pop_accumulator64(ValueLane::S64)?;
            self.instructions.push(Instruction::AccGlobalSet64(global_index));
            return Ok(());
        }
        let instruction = size.select(
            Instruction::GlobalSet32(global_index),
            Instruction::GlobalSet64(global_index),
            Instruction::GlobalSet128(global_index),
        );
        self.emit(&[size], &[], instruction)
    }

    fn visit_global_get(&mut self, global_index: u32) -> Self::Output {
        let size = self.metadata.global_size(global_index)?;
        if self.metadata.global_is_reference(global_index)? {
            self.materialize_accumulator_ref();
            self.push_accumulator_ref()?;
            self.instructions.push(Instruction::AccRefGlobalGet(global_index));
            return Ok(());
        }
        if self.metadata.global_is_numeric(global_index)? {
            self.materialize_lane(size);
            match size {
                ValueLane::S32 => {
                    self.push_accumulator32(size)?;
                    self.instructions.push(Instruction::AccGlobalGet32(global_index));
                }
                ValueLane::S64 => {
                    self.push_accumulator64(size)?;
                    self.instructions.push(Instruction::AccGlobalGet64(global_index));
                }
                ValueLane::S128 => unreachable!(),
            }
            return Ok(());
        }
        let instruction = size.select(
            Instruction::GlobalGet32(global_index),
            Instruction::GlobalGet64(global_index),
            Instruction::GlobalGet128(global_index),
        );
        self.emit(&[], &[size], instruction)
    }

    fn visit_drop(&mut self) -> Self::Output {
        if self.top_is_accumulator_ref() {
            self.pop_accumulator_ref()?;
            self.instructions.push(Instruction::ClearAccRef);
            return Ok(());
        }
        if self.top_is_accumulator32() {
            self.pop_accumulator32(ValueLane::S32)?;
            return Ok(());
        }
        if self.top_is_accumulator64() {
            self.pop_accumulator64(ValueLane::S64)?;
            return Ok(());
        }
        let size = self.operand_stack.last().map(|value| value.lane()).unwrap_or(ValueLane::S32);
        if self.operand_stack.last().is_some_and(|value| !matches!(value, ValueLocation::Stack(_))) {
            return self.pop_deferred_value(size);
        }
        let instruction = size.select(Instruction::Drop32, Instruction::Drop64, Instruction::Drop128);
        self.emit(&[size], &[], instruction)
    }

    fn visit_select(&mut self) -> Self::Output {
        let size = self.operand_stack.iter().rev().nth(1).map(|value| value.lane()).unwrap_or(ValueLane::S32);
        if size == ValueLane::S32 && self.top_is_accumulator32() {
            self.materialize_lane_prefix(ValueLane::S32, self.operand_stack.len().saturating_sub(1));
            self.pop_accumulator32(ValueLane::S32)?;
            self.pop_stack_value(ValueLane::S32)?;
            self.pop_stack_value(ValueLane::S32)?;
            self.push_accumulator32(ValueLane::S32)?;
            self.instructions.push(Instruction::AccSelect32);
            return Ok(());
        }
        if size == ValueLane::S64 && self.top_is_accumulator32() && !self.accumulator64_is_live() {
            self.materialize_lane_prefix(ValueLane::S64, self.operand_stack.len().saturating_sub(1));
            self.pop_accumulator32(ValueLane::S32)?;
            self.pop_stack_value(ValueLane::S64)?;
            self.pop_stack_value(ValueLane::S64)?;
            self.push_accumulator64(ValueLane::S64)?;
            self.instructions.push(Instruction::AccSelect64);
            return Ok(());
        }
        let instruction = size.select(Instruction::Select32, Instruction::Select64, Instruction::Select128);
        self.emit(&[size, size, ValueLane::S32], &[size], instruction)
    }

    fn visit_local_get(&mut self, idx: u32) -> Self::Output {
        let (size, numeric32, local_idx) = self.local(idx)?;
        if numeric32 {
            return self.emit_numeric32_source(ValueLocation::LocalNumeric32(local_idx));
        }
        if size == ValueLane::S64 {
            return self.emit_numeric64_source(ValueLocation::LocalNumeric64(local_idx));
        }
        if size == ValueLane::S32 {
            return self.push_location(ValueLocation::LocalRef(local_idx));
        }
        let instruction = size.select(
            Instruction::LocalGet32(local_idx),
            Instruction::LocalGet64(local_idx),
            Instruction::LocalGet128(local_idx),
        );
        self.emit(&[], &[size], instruction)
    }

    fn visit_local_set(&mut self, idx: u32) -> Self::Output {
        let (size, numeric32, local_idx) = self.local(idx)?;
        if !numeric32 && size == ValueLane::S32 && self.top_is_accumulator_ref() {
            self.materialize_deferred_operands_before_local_write(ValueLane::S32, local_idx);
            self.pop_accumulator_ref()?;
            self.instructions.push(Instruction::AccRefLocalSet(local_idx));
            return Ok(());
        }
        let source = self.operand_stack.last().copied();
        if numeric32
            && let Some(source @ (ValueLocation::LocalNumeric32(_) | ValueLocation::ConstNumeric32(_))) = source
        {
            self.materialize_deferred_operands_before_local_write(ValueLane::S32, local_idx);
            self.pop_deferred_value(ValueLane::S32)?;
            let instruction = match source {
                ValueLocation::LocalNumeric32(source) => Instruction::LocalCopy32(source, local_idx),
                ValueLocation::ConstNumeric32(value) => {
                    Instruction::SetLocalConst32(tinywasm_types::I32LocalArg { value, local: local_idx })
                }
                _ => unreachable!(),
            };
            self.instructions.push(instruction);
            return Ok(());
        }
        if size == ValueLane::S64
            && let Some(source @ (ValueLocation::LocalNumeric64(_) | ValueLocation::ConstNumeric64(_))) = source
        {
            self.materialize_deferred_operands_before_local_write(ValueLane::S64, local_idx);
            self.pop_deferred_value(ValueLane::S64)?;
            let instruction = match source {
                ValueLocation::LocalNumeric64(source) => Instruction::LocalCopy64(source, local_idx),
                ValueLocation::ConstNumeric64(value) => {
                    let value = self.push_operand64(Operand64::<i64>::new(value))?;
                    Instruction::SetLocalConst64(PackedOp::new(local_idx, value))
                }
                _ => unreachable!(),
            };
            self.instructions.push(instruction);
            return Ok(());
        }
        if numeric32 && self.top_is_accumulator32() {
            self.materialize_deferred_operands_before_local_write(ValueLane::S32, local_idx);
            self.pop_accumulator32(ValueLane::S32)?;
            let fused = match self.instructions.select_acc_local_set32(local_idx) {
                Some(selector::Acc32LocalSetSelection::Load(memory_arg_idx)) => {
                    Some(Instruction::AccLoadTee32(tinywasm_types::AccMemoryLocalArg {
                        memory_arg_idx,
                        local: local_idx,
                    }))
                }
                Some(selector::Acc32LocalSetSelection::LocalLoad { op, memory, address }) => {
                    let compact_memory = CompactMemoryArg::try_from(self.data.operand128(memory)).ok();
                    let compact_address = u8::try_from(address).ok();
                    let destination = u8::try_from(local_idx).ok();
                    match (compact_memory, compact_address, destination) {
                        (Some(memory), Some(address), Some(destination)) => {
                            let memory_arg_idx = self.push_operand64(Operand64::from(memory))?;
                            let argument = MemoryLocalArg { memory_arg_idx, local1: address, local2: destination };
                            Some(match op {
                                LoadOp32::Full => Instruction::LoadLocalSet32(argument),
                                LoadOp32::I8S => Instruction::LoadLocalSet8S32(argument),
                                LoadOp32::I8U => Instruction::LoadLocalSet8U32(argument),
                                LoadOp32::I16S => Instruction::LoadLocalSet16S32(argument),
                                LoadOp32::I16U => Instruction::LoadLocalSet16U32(argument),
                            })
                        }
                        _ => {
                            self.instructions.push(Instruction::AccLocalGet32(address));
                            self.instructions.push(match op {
                                LoadOp32::Full => Instruction::AccLoad32(memory),
                                LoadOp32::I8S => Instruction::AccLoad8S32(memory),
                                LoadOp32::I8U => Instruction::AccLoad8U32(memory),
                                LoadOp32::I16S => Instruction::AccLoad16S32(memory),
                                LoadOp32::I16U => Instruction::AccLoad16U32(memory),
                            });
                            None
                        }
                    }
                }
                Some(selector::Acc32LocalSetSelection::AddLocalConst { local, value }) => {
                    if local == local_idx {
                        Some(Instruction::IncLocal32(tinywasm_types::I32LocalArg { value, local }))
                    } else {
                        let operand =
                            self.push_operand64(Operand64::<(u16, u16, u32)>::new(local, local_idx, value as u32))?;
                        Some(Instruction::BinOpLocalConstSet32(PackedOp::new(BinOp::IAdd, operand)))
                    }
                }
                Some(selector::Acc32LocalSetSelection::LocalConst(packed)) => {
                    let source = self.data.operand64(packed.index);
                    let operand =
                        self.push_operand64(Operand64::<(u16, u16, u32)>::new(source.a(), local_idx, source.b()))?;
                    Some(Instruction::BinOpLocalConstSet32(PackedOp::new(packed.op, operand)))
                }
                Some(selector::Acc32LocalSetSelection::LocalLocal { op, left, right }) => {
                    if op == BinOp::IAdd {
                        Some(Instruction::AddLocalLocalSet32(LocalTripleArg { left, right, dst: local_idx }))
                    } else {
                        let operand = self.push_operand64(Operand64::<(u16, u16, u16)>::new(left, right, local_idx))?;
                        Some(Instruction::BinOpLocalLocalSet32(PackedOp::new(op, operand)))
                    }
                }
                Some(selector::Acc32LocalSetSelection::MultiplyAccumulate { floating }) => Some(if floating {
                    Instruction::FMulAccLocal32(local_idx)
                } else {
                    Instruction::MulAccLocal32(local_idx)
                }),
                None => None,
            };
            self.instructions.push(fused.unwrap_or(Instruction::AccLocalSet32(local_idx)));
            return Ok(());
        }
        if size == ValueLane::S64 && self.top_is_accumulator64() {
            self.materialize_deferred_operands_before_local_write(ValueLane::S64, local_idx);
            self.pop_accumulator64(ValueLane::S64)?;
            self.instructions.push(Instruction::AccLocalSet64(local_idx));
            return Ok(());
        }
        let instruction = size.select(
            Instruction::LocalSet32(local_idx),
            Instruction::LocalSet64(local_idx),
            Instruction::LocalSet128(local_idx),
        );
        self.emit(&[size], &[], instruction)
    }

    fn visit_local_tee(&mut self, idx: u32) -> Self::Output {
        let (size, numeric32, local_idx) = self.local(idx)?;
        if !numeric32 && size == ValueLane::S32 && self.top_is_accumulator_ref() {
            self.materialize_deferred_operands_before_local_write(ValueLane::S32, local_idx);
            self.instructions.push(Instruction::AccRefLocalTee(local_idx));
            return Ok(());
        }
        if numeric32 && self.top_is_accumulator32() {
            self.materialize_deferred_operands_before_local_write(ValueLane::S32, local_idx);
            match self.instructions.select_acc_local_tee32() {
                Some(selector::Acc32LocalTeeSelection::Load(memory_arg_idx)) => {
                    self.instructions.push(Instruction::AccLoadTee32(tinywasm_types::AccMemoryLocalArg {
                        memory_arg_idx,
                        local: local_idx,
                    }));
                }
                Some(selector::Acc32LocalTeeSelection::AddLocalConst { local, value }) => {
                    let operand =
                        self.push_operand64(Operand64::<(u16, u16, u32)>::new(local, local_idx, value as u32))?;
                    self.instructions.stage_i32_add_local_const_tee(local, value, local_idx, operand);
                }
                Some(selector::Acc32LocalTeeSelection::LocalConst(packed)) => {
                    let source = self.data.operand64(packed.index);
                    let operand =
                        self.push_operand64(Operand64::<(u16, u16, u32)>::new(source.a(), local_idx, source.b()))?;
                    self.instructions.push(Instruction::AccBinOpLocalConstTee32(PackedOp::new(packed.op, operand)));
                }
                Some(selector::Acc32LocalTeeSelection::LocalLocal { op, left, right }) => {
                    let operand = self.push_operand64(Operand64::<(u16, u16, u16)>::new(left, right, local_idx))?;
                    self.instructions.push(Instruction::AccBinOpLocalLocalTee32(PackedOp::new(op, operand)));
                }
                Some(selector::Acc32LocalTeeSelection::Const { op, value }) => {
                    let operand = self.push_operand64(Operand64::<(u16, u32)>::new(local_idx, value as u32))?;
                    self.instructions.push(Instruction::AccBinOpConstTee32(PackedOp::new(op, operand)));
                }
                None => self.instructions.push(Instruction::AccLocalTee32(local_idx)),
            }
            return Ok(());
        }
        if size == ValueLane::S64 && self.top_is_accumulator64() {
            self.materialize_deferred_operands_before_local_write(ValueLane::S64, local_idx);
            let fused = match self.instructions.select_acc_local_tee64() {
                Some(selector::Acc64LocalTeeSelection::Stack(op)) => {
                    Some(Instruction::AccBinOpStackTee64(op, local_idx))
                }
                Some(selector::Acc64LocalTeeSelection::StackStack(op)) => {
                    Some(Instruction::AccBinOpStackStackTee64(op, local_idx))
                }
                Some(selector::Acc64LocalTeeSelection::Local { op, local }) => {
                    Some(Instruction::AccBinOpLocalTee64(op, local, local_idx))
                }
                Some(selector::Acc64LocalTeeSelection::Const(packed)) => {
                    let value = self.data.operand64(packed.index).value();
                    let operand = self.push_operand128(Operand128::<(u16, u64)>::new(local_idx, value as u64))?;
                    Some(Instruction::AccBinOpConstTee64(PackedOp::new(packed.op, operand)))
                }
                Some(selector::Acc64LocalTeeSelection::NestedLocalLocal(packed)) => {
                    let source = self.data.operand64(packed.index);
                    let operand =
                        self.push_operand64(Operand64::<(u16, u16, u16)>::new(source.a(), source.b(), local_idx))?;
                    Some(Instruction::AccBinOpNestedLocalLocalTee64(PackedOp::new(packed.op, operand)))
                }
                None => None,
            };
            self.instructions.push(fused.unwrap_or(Instruction::AccLocalTee64(local_idx)));
            return Ok(());
        }
        self.apply_effect(&[size], &[size])?;
        let src = match (size, self.instructions.last()) {
            (ValueLane::S32, Some(Instruction::LocalGet32(src))) => Some(*src),
            (ValueLane::S64, Some(Instruction::LocalGet64(src))) => Some(*src),
            (ValueLane::S128, Some(Instruction::LocalGet128(src))) => Some(*src),
            _ => None,
        };
        if let Some(src) = src {
            self.instructions.pop();
            let instructions = match size {
                ValueLane::S32 => [Instruction::LocalCopy32(src, local_idx), Instruction::LocalGet32(local_idx)],
                ValueLane::S64 => [Instruction::LocalCopy64(src, local_idx), Instruction::LocalGet64(local_idx)],
                ValueLane::S128 => [Instruction::LocalCopy128(src, local_idx), Instruction::LocalGet128(local_idx)],
            };
            self.instructions.extend(instructions);
        } else {
            self.instructions.push(size.select(
                Instruction::LocalTee32(local_idx),
                Instruction::LocalTee64(local_idx),
                Instruction::LocalTee128(local_idx),
            ));
        }
        Ok(())
    }

    fn visit_block(&mut self, blockty: wasmparser::BlockType) -> Self::Output {
        self.push_control(BlockKind::Block, blockty, None)
    }

    fn visit_loop(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.push_control(BlockKind::Loop, ty, None)
    }

    fn visit_if(&mut self, ty: wasmparser::BlockType) -> Self::Output {
        self.materialize_deferred_prefix(self.operand_stack.len().saturating_sub(1));
        let location = self.operand_stack.last().copied();
        let instruction = if self.top_is_accumulator32() {
            self.pop_accumulator32(ValueLane::S32)?;
            Instruction::JumpIfAccZero32(0)
        } else if let Some(ValueLocation::LocalNumeric32(local)) = location {
            self.pop_deferred_value(ValueLane::S32)?;
            Instruction::JumpIfLocalZero32(TargetLocalArg { target_ip: 0, local })
        } else if let Some(location @ (ValueLocation::LocalNumeric32(_) | ValueLocation::ConstNumeric32(_))) = location
        {
            self.pop_deferred_value(ValueLane::S32)?;
            self.instructions.push(Self::numeric32_source_instruction(location));
            Instruction::JumpIfAccZero32(0)
        } else {
            self.pop_expect(ValueLane::S32)?;
            Instruction::JumpIfZero32(0)
        };
        let else_label = self.labels.new_label();
        if !self.try_emit_selected_i32_branch(else_label, false)? {
            self.emit_labeled_instruction(instruction, else_label);
        }
        self.push_control(BlockKind::If, ty, Some(else_label))
    }

    fn visit_try_table(&mut self, try_table: wasmparser::TryTable) -> Self::Output {
        self.materialize_all_scalars();
        let signature = self.block_signature(try_table.ty)?;
        for &size in signature.params.iter().rev() {
            self.pop_expect(size)?;
        }
        let height = self.operand_stack.len();
        let base = self.lane_counts;
        let entry_unreachable = self.is_unreachable();

        let body_label = self.labels.new_label();
        self.emit_labeled_instruction(Instruction::Jump(0), body_label);
        let mut catches = Vec::with_capacity(try_table.catches.len());
        let mut catch_labels = Vec::with_capacity(try_table.catches.len());
        for catch in try_table.catches {
            let (tag, depth, with_ref) = match catch {
                wasmparser::Catch::One { tag, label } => (Some(tag), label, false),
                wasmparser::Catch::OneRef { tag, label } => (Some(tag), label, true),
                wasmparser::Catch::All { label } => (None, label, false),
                wasmparser::Catch::AllRef { label } => (None, label, true),
            };
            if let Some(tag) = tag {
                self.metadata.tag_signature(tag)?;
            }
            let target_idx = self.get_ctx_idx(depth)?;
            let target_base = self.control_stack[target_idx].base;
            let landing_label = self.labels.new_label();
            self.labels.pin(landing_label, self.instructions.len())?;
            catch_labels.push(landing_label);
            let (target_kind, start_label, end_label) = {
                let target = &self.control_stack[target_idx];
                (target.kind, target.start_label, target.end_label)
            };
            match target_kind {
                BlockKind::Function => self.instructions.push(Instruction::Return),
                BlockKind::Loop => {
                    self.emit_labeled_instruction(Instruction::Jump(0), start_label);
                }
                BlockKind::Block | BlockKind::If | BlockKind::TryTable => {
                    self.control_stack[target_idx].end_reachable = true;
                    self.emit_labeled_instruction(Instruction::Jump(0), end_label);
                }
            }
            catches.push(match tag {
                Some(tag) => tinywasm_types::ExceptionCatch::Tag { tag, landing_pad: 0, base: target_base, with_ref },
                None => tinywasm_types::ExceptionCatch::All { landing_pad: 0, base: target_base, with_ref },
            });
        }

        self.labels.pin(body_label, self.instructions.len())?;
        let end_label = self.labels.new_label();
        let handler_idx = self.data.exception_handlers.len();
        self.data.exception_handlers.push(tinywasm_types::ExceptionHandler {
            start_ip: 0,
            end_ip: 0,
            catches: catches.into_boxed_slice(),
        });
        self.labels.use_handler_start(body_label, handler_idx);
        self.labels.use_handler_end(end_label, handler_idx);
        for (catch, label) in catch_labels.into_iter().enumerate() {
            self.labels.use_catch_landing(label, handler_idx, catch);
        }
        self.push_sizes(&signature.params)?;
        self.control_stack.push(ControlFrame {
            kind: BlockKind::TryTable,
            has_else: false,
            start_label: body_label,
            end_label,
            else_label: None,
            height,
            base,
            params: signature.params,
            results: signature.results,
            unreachable: entry_unreachable,
            entry_unreachable,
            end_reachable: false,
        });
        Ok(())
    }

    fn visit_throw(&mut self, tag_index: u32) -> Self::Output {
        self.materialize_all_scalars();
        let signature = self.metadata.tag_signature(tag_index)?;
        self.apply_effect(&signature.params, &[])?;
        self.instructions.push(Instruction::Throw(tag_index));
        self.mark_unreachable();
        Ok(())
    }

    fn visit_throw_ref(&mut self) -> Self::Output {
        self.materialize_all_scalars();
        self.apply_effect(&[ValueLane::S32], &[])?;
        self.instructions.push(Instruction::ThrowRef);
        self.mark_unreachable();
        Ok(())
    }

    fn visit_return(&mut self) -> Self::Output {
        self.emit_return_instruction()?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_else(&mut self) -> Self::Output {
        self.materialize_all_scalars();
        let (else_label, end_label, height, base, params, entry_unreachable) = {
            let ctx = self
                .control_stack
                .last_mut()
                .filter(|ctx| matches!(ctx.kind, BlockKind::If))
                .ok_or_else(|| crate::ParseError::Other("else without matching if".into()))?;
            ctx.end_reachable |= !ctx.unreachable;
            ctx.has_else = true;
            (
                ctx.else_label.ok_or_else(|| crate::ParseError::Other("if frame has no else label".into()))?,
                ctx.end_label,
                ctx.height,
                ctx.base,
                ctx.params.clone(),
                ctx.entry_unreachable,
            )
        };
        self.emit_labeled_instruction(Instruction::Jump(0), end_label);
        self.labels.pin(else_label, self.instructions.len())?;
        self.reset_stack(height, base);
        self.push_sizes(&params)?;
        self.control_stack.last_mut().unwrap().unreachable = entry_unreachable;
        Ok(())
    }

    fn visit_end(&mut self) -> Self::Output {
        let function_end = self.control_stack.last().is_some_and(|ctx| matches!(ctx.kind, BlockKind::Function));
        if function_end {
            self.emit_return_instruction()?;
        } else {
            self.materialize_all_scalars();
        }
        let ctx =
            self.control_stack.pop().ok_or_else(|| crate::ParseError::Other("end without control frame".into()))?;
        if matches!(ctx.kind, BlockKind::If)
            && !ctx.has_else
            && let Some(else_label) = ctx.else_label
        {
            self.labels.pin(else_label, self.instructions.len())?;
        }
        self.labels.pin(ctx.end_label, self.instructions.len())?;
        if !matches!(ctx.kind, BlockKind::Function) {
            let reachable = !ctx.entry_unreachable
                && (!ctx.unreachable || ctx.end_reachable || matches!(ctx.kind, BlockKind::If) && !ctx.has_else);
            self.reset_stack(ctx.height, ctx.base);
            self.push_sizes(&ctx.results)?;
            if let Some(parent) = self.control_stack.last_mut() {
                parent.unreachable = !reachable;
            }
        }
        Ok(())
    }

    fn visit_br(&mut self, depth: u32) -> Self::Output {
        self.emit_dropkeep_to_label(depth)?;
        self.emit_branch_jump_or_return(depth)?;
        self.mark_unreachable();
        Ok(())
    }

    fn visit_br_if(&mut self, depth: u32) -> Self::Output {
        self.materialize_deferred_prefix(self.operand_stack.len().saturating_sub(1));
        let location = self.operand_stack.last().copied();
        let local_condition = match location {
            Some(ValueLocation::LocalNumeric32(local)) => Some(local),
            _ => None,
        };
        let register_condition = self.top_is_accumulator32()
            || local_condition.is_some()
            || matches!(location, Some(ValueLocation::ConstNumeric32(_)));
        if register_condition {
            if self.top_is_accumulator32() {
                self.pop_accumulator32(ValueLane::S32)?;
            } else if local_condition.is_some() {
                self.pop_deferred_value(ValueLane::S32)?;
            } else {
                let location = location.unwrap();
                self.pop_deferred_value(ValueLane::S32)?;
                self.instructions.push(Self::numeric32_source_instruction(location));
            }
        } else {
            self.pop_expect(ValueLane::S32)?;
        }
        if !self.branch_needs_dropkeep(depth)?
            && let Some(target) = self.branch_target(depth)?
        {
            let instruction = if let Some(local) = local_condition {
                Instruction::JumpIfLocalNonZero32(TargetLocalArg { target_ip: 0, local })
            } else if register_condition {
                Instruction::JumpIfAccNonZero32(0)
            } else {
                Instruction::JumpIfNonZero32(0)
            };
            if !self.try_emit_selected_i32_branch(target, true)? {
                self.emit_labeled_instruction(instruction, target);
            }
            return Ok(());
        }

        let fallthrough = self.labels.new_label();
        let instruction = if let Some(local) = local_condition {
            Instruction::JumpIfLocalZero32(TargetLocalArg { target_ip: 0, local })
        } else if register_condition {
            Instruction::JumpIfAccZero32(0)
        } else {
            Instruction::JumpIfZero32(0)
        };
        if !self.try_emit_selected_i32_branch(fallthrough, false)? {
            self.emit_labeled_instruction(instruction, fallthrough);
        }
        self.emit_dropkeep_to_label(depth)?;
        self.emit_branch_jump_or_return(depth)?;
        self.labels.pin(fallthrough, self.instructions.len())?;
        Ok(())
    }

    fn visit_br_table(&mut self, targets: wasmparser::BrTable<'_>) -> Self::Output {
        let ts = targets.targets().collect::<Result<Vec<_>, wasmparser::Error>>()?;
        self.pop_expect(ValueLane::S32)?;

        let default_depth = targets.default();
        let len = ts.len() as u32;
        let target_depths: Vec<u32> = ts;

        struct PadInfo {
            depth: u32,
            label: LabelId,
        }
        let mut pads: Vec<PadInfo> = Vec::new();

        for &depth in target_depths.iter().chain(core::iter::once(&default_depth)) {
            if pads.iter().any(|pad| pad.depth == depth) {
                continue;
            }
            pads.push(PadInfo { depth, label: self.labels.new_label() });
        }

        let branch_table_start = self.data.branch_table_targets.len() as u32;
        for &depth in &target_depths {
            let pad = pads
                .iter()
                .find(|pad| pad.depth == depth)
                .ok_or_else(|| crate::ParseError::Other("missing branch table target".into()))?;
            let target = self.data.branch_table_targets.len();
            self.data.branch_table_targets.push(0);
            self.labels.use_branch_table_target(pad.label, target);
        }

        let default_pad = pads
            .iter()
            .find(|pad| pad.depth == default_depth)
            .ok_or_else(|| crate::ParseError::Other("missing default branch table target".into()))?;
        let branch_operand =
            self.data.push_target_operand128(Operand128::<BranchTableOperand>::new(0, branch_table_start, len))?;
        self.labels.use_operand128(default_pad.label, branch_operand.index());
        self.instructions.push(Instruction::BranchTable(branch_operand));

        for pad in pads {
            self.labels.pin(pad.label, self.instructions.len())?;
            if self.is_unreachable() {
                self.instructions.push(Instruction::Return);
                continue;
            }
            self.emit_dropkeep_to_label(pad.depth)?;
            self.emit_branch_jump_or_return(pad.depth)?;
        }
        self.mark_unreachable();
        Ok(())
    }

    fn visit_f32_const(&mut self, val: wasmparser::Ieee32) -> Self::Output {
        self.emit_numeric32_source(ValueLocation::ConstNumeric32(val.bits() as i32))
    }

    fn visit_i32_const(&mut self, value: i32) -> Self::Output {
        self.emit_numeric32_source(ValueLocation::ConstNumeric32(value))
    }

    fn visit_f64_const(&mut self, val: wasmparser::Ieee64) -> Self::Output {
        self.emit_numeric64_source(ValueLocation::ConstNumeric64(val.bits() as i64))
    }

    fn visit_i64_const(&mut self, value: i64) -> Self::Output {
        self.emit_numeric64_source(ValueLocation::ConstNumeric64(value))
    }

    fn visit_table_copy(&mut self, dst_table: u32, src_table: u32) -> Self::Output {
        let dst = self.metadata.table_size(dst_table)?;
        let src = self.metadata.table_size(src_table)?;
        let len = if dst == ValueLane::S32 || src == ValueLane::S32 { ValueLane::S32 } else { ValueLane::S64 };
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(dst_table, src_table))?;
        self.emit(&[dst, src, len], &[], Instruction::TableCopy(operand))
    }

    fn visit_memory_copy(&mut self, dst_mem: u32, src_mem: u32) -> Self::Output {
        let dst = self.metadata.memory_size(dst_mem)?;
        let src = self.metadata.memory_size(src_mem)?;
        self.mark_memory(dst_mem);
        self.mark_memory(src_mem);
        let len = if dst == ValueLane::S32 || src == ValueLane::S32 { ValueLane::S32 } else { ValueLane::S64 };
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(dst_mem, src_mem))?;
        self.emit(&[dst, src, len], &[], Instruction::MemoryCopy(operand))
    }

    fn visit_memory_init(&mut self, data_index: u32, memory: u32) -> Self::Output {
        let dst = self.metadata.memory_size(memory)?;
        self.mark_memory(memory);
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(data_index, memory))?;
        self.emit(&[dst, ValueLane::S32, ValueLane::S32], &[], Instruction::MemoryInit(operand))
    }

    fn visit_table_init(&mut self, elem_index: u32, table: u32) -> Self::Output {
        let address = self.metadata.table_size(table)?;
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(elem_index, table))?;
        self.emit(&[address, ValueLane::S32, ValueLane::S32], &[], Instruction::TableInit(operand))
    }

    fn visit_array_new_data(&mut self, type_index: u32, data_index: u32) -> Self::Output {
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, data_index))?;
        self.emit(&[ValueLane::S32, ValueLane::S32], &[ValueLane::S32], Instruction::ArrayNewData(operand))
    }

    fn visit_array_new_elem(&mut self, type_index: u32, elem_index: u32) -> Self::Output {
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, elem_index))?;
        self.emit(&[ValueLane::S32, ValueLane::S32], &[ValueLane::S32], Instruction::ArrayNewElem(operand))
    }

    fn visit_array_init_data(&mut self, type_index: u32, data_index: u32) -> Self::Output {
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, data_index))?;
        self.emit(&[ValueLane::S32; 4], &[], Instruction::ArrayInitData(operand))
    }

    fn visit_array_init_elem(&mut self, type_index: u32, elem_index: u32) -> Self::Output {
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index, elem_index))?;
        self.emit(&[ValueLane::S32; 4], &[], Instruction::ArrayInitElem(operand))
    }

    fn visit_array_copy(&mut self, type_index_dst: u32, type_index_src: u32) -> Self::Output {
        let operand = self.push_operand64(Operand64::<(u32, u32)>::new(type_index_dst, type_index_src))?;
        self.emit(&[ValueLane::S32; 5], &[], Instruction::ArrayCopy(operand))
    }

    fn visit_br_on_cast(
        &mut self,
        relative_depth: u32,
        _from_ref_type: wasmparser::RefType,
        to_ref_type: wasmparser::RefType,
    ) -> Self::Output {
        self.emit_cast_branch(relative_depth, to_ref_type, false)
    }

    fn visit_br_on_cast_fail(
        &mut self,
        relative_depth: u32,
        _from_ref_type: wasmparser::RefType,
        to_ref_type: wasmparser::RefType,
    ) -> Self::Output {
        self.emit_cast_branch(relative_depth, to_ref_type, true)
    }

    fn visit_br_on_null(&mut self, relative_depth: u32) -> Self::Output {
        self.pop_expect(ValueLane::S32)?;
        let fallthrough = self.labels.new_label();
        self.emit_labeled_instruction(Instruction::JumpIfRefNonNull(0), fallthrough);
        self.emit_dropkeep_to_label(relative_depth)?;
        self.emit_branch_jump_or_return(relative_depth)?;
        self.labels.pin(fallthrough, self.instructions.len())?;
        self.push_sizes(&[ValueLane::S32])
    }

    fn visit_br_on_non_null(&mut self, relative_depth: u32) -> Self::Output {
        self.pop_expect(ValueLane::S32)?;
        let fallthrough = self.labels.new_label();
        self.emit_labeled_instruction(Instruction::JumpIfRefNull(0), fallthrough);
        self.push_sizes(&[ValueLane::S32])?;
        self.emit_dropkeep_to_label(relative_depth)?;
        self.pop_expect(ValueLane::S32)?;
        self.emit_branch_jump_or_return(relative_depth)?;
        self.labels.pin(fallthrough, self.instructions.len())?;
        Ok(())
    }

    fn visit_typed_select_multi(&mut self, tys: Vec<wasmparser::ValType>) -> Self::Output {
        let sizes: Vec<_> = tys.into_iter().map(value_lane).collect();
        let counts = Self::value_counts(&sizes);
        self.emit(
            &[sizes.as_slice(), sizes.as_slice(), &[ValueLane::S32]].concat(),
            &sizes,
            Instruction::SelectMulti(counts),
        )
    }

    fn visit_typed_select(&mut self, ty: wasmparser::ValType) -> Self::Output {
        if matches!(
            ty,
            wasmparser::ValType::I32 | wasmparser::ValType::F32 | wasmparser::ValType::I64 | wasmparser::ValType::F64
        ) {
            return self.visit_select();
        }
        let size = value_lane(ty);
        let instruction = size.select(Instruction::Select32, Instruction::Select64, Instruction::Select128);
        self.emit(&[size, size, ValueLane::S32], &[size], instruction)
    }
}

macro_rules! impl_visit_simd_operator {
    ($(@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*))*) => {
        $(impl_visit_operator!(@@$proposal $op $({ $($arg: $argty),* })? => $visit ($($ann:tt)*));)*
    };

    (@@simd $($rest:tt)* ) => {};
    (@@relaxed_simd $($rest:tt)* ) => {};
    (@@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*)) => {
        fn $visit(&mut self $($(,$arg: $argty)*)?) -> Self::Output {
            Err(crate::ParseError::UnsupportedOperator(stringify!($visit).to_string()))
        }
    };
}

impl wasmparser::VisitSimdOperator<'_> for FunctionBuilder<'_> {
    wasmparser::for_each_visit_simd_operator!(impl_visit_simd_operator);

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
        memory [Addr, S128] => [] { visit_v128_store => V128Store }
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
            visit_v128_and => V128And, visit_v128_andnot => V128AndNot, visit_v128_or => V128Or,
            visit_v128_xor => V128Xor, visit_i8x16_swizzle => I8x16Swizzle, visit_i8x16_eq => I8x16Eq,
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
            visit_i32x4_max_u => I32x4MaxU, visit_i32x4_mul => I32x4Mul, visit_i64x2_add => I64x2Add,
            visit_i64x2_sub => I64x2Sub, visit_i64x2_mul => I64x2Mul,
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

    fn visit_i8x16_shuffle(&mut self, lanes: [u8; 16]) -> Self::Output {
        let index = self.push_operand128(Operand128::<[u8; 16]>::new(lanes))?;
        self.emit(&[ValueLane::S128, ValueLane::S128], &[ValueLane::S128], Instruction::I8x16Shuffle(index))
    }

    fn visit_v128_const(&mut self, value: wasmparser::V128) -> Self::Output {
        let index = self.push_operand128(Operand128::<[u8; 16]>::new(*value.bytes()))?;
        self.emit(&[], &[ValueLane::S128], Instruction::Const128(index))
    }
}

impl FunctionBuilder<'_> {
    fn emit_cast_branch(
        &mut self,
        relative_depth: u32,
        target: wasmparser::RefType,
        branch_on_fail: bool,
    ) -> Result<()> {
        self.pop_expect(ValueLane::S32)?;
        let target = convert_heap_type(target.heap_type(), target.is_nullable())?;
        let fallthrough = self.labels.new_label();
        let operand = self.data.push_target_operand64(Operand64::<(u32, u32)>::new(0, target.to_bits()))?;
        self.labels.use_operand64(fallthrough, operand.index());
        self.instructions.push(if branch_on_fail {
            Instruction::BrOnCastFail(operand)
        } else {
            Instruction::BrOnCast(operand)
        });
        self.push_sizes(&[ValueLane::S32])?;
        self.emit_dropkeep_to_label(relative_depth)?;
        self.emit_branch_jump_or_return(relative_depth)?;
        self.labels.pin(fallthrough, self.instructions.len())?;
        Ok(())
    }

    fn is_unreachable(&self) -> bool {
        self.control_stack.last().is_none_or(|frame| frame.unreachable)
    }

    fn has_polymorphic_inputs(&self, input_count: usize) -> bool {
        self.control_stack.last().is_none_or(|frame| {
            frame.unreachable && self.operand_stack.len().saturating_sub(frame.height) < input_count
        })
    }

    fn get_ctx_idx(&self, depth: u32) -> Result<usize> {
        self.control_stack
            .len()
            .checked_sub(depth as usize + 1)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("branch depth out of bounds: {depth}")))
    }

    fn accumulator_load_instruction(instruction: Instruction) -> Option<Instruction> {
        Some(match instruction {
            Instruction::I32Load(index) | Instruction::F32Load(index) => Instruction::AccLoad32(index),
            Instruction::I32Load8S(index) => Instruction::AccLoad8S32(index),
            Instruction::I32Load8U(index) => Instruction::AccLoad8U32(index),
            Instruction::I32Load16S(index) => Instruction::AccLoad16S32(index),
            Instruction::I32Load16U(index) => Instruction::AccLoad16U32(index),
            _ => return None,
        })
    }

    fn accumulator_load32_addr64_instruction(instruction: Instruction) -> Option<Instruction> {
        Some(match instruction {
            Instruction::I32Load(index) | Instruction::F32Load(index) => Instruction::AccLoad32Addr64(index),
            Instruction::I32Load8S(index) => Instruction::AccLoad8S32Addr64(index),
            Instruction::I32Load8U(index) => Instruction::AccLoad8U32Addr64(index),
            Instruction::I32Load16S(index) => Instruction::AccLoad16S32Addr64(index),
            Instruction::I32Load16U(index) => Instruction::AccLoad16U32Addr64(index),
            _ => return None,
        })
    }

    fn accumulator_load64_instruction(instruction: Instruction, address: ValueLane) -> Option<Instruction> {
        Some(match (instruction, address) {
            (Instruction::I64Load(index) | Instruction::F64Load(index), ValueLane::S32) => {
                Instruction::AccLoad64Addr32(index)
            }
            (Instruction::I64Load8S(index), ValueLane::S32) => Instruction::AccLoad8S64Addr32(index),
            (Instruction::I64Load8U(index), ValueLane::S32) => Instruction::AccLoad8U64Addr32(index),
            (Instruction::I64Load16S(index), ValueLane::S32) => Instruction::AccLoad16S64Addr32(index),
            (Instruction::I64Load16U(index), ValueLane::S32) => Instruction::AccLoad16U64Addr32(index),
            (Instruction::I64Load32S(index), ValueLane::S32) => Instruction::AccLoad32S64Addr32(index),
            (Instruction::I64Load32U(index), ValueLane::S32) => Instruction::AccLoad32U64Addr32(index),
            (Instruction::I64Load(index) | Instruction::F64Load(index), ValueLane::S64) => {
                Instruction::AccLoad64Addr64(index)
            }
            (Instruction::I64Load8S(index), ValueLane::S64) => Instruction::AccLoad8S64Addr64(index),
            (Instruction::I64Load8U(index), ValueLane::S64) => Instruction::AccLoad8U64Addr64(index),
            (Instruction::I64Load16S(index), ValueLane::S64) => Instruction::AccLoad16S64Addr64(index),
            (Instruction::I64Load16U(index), ValueLane::S64) => Instruction::AccLoad16U64Addr64(index),
            (Instruction::I64Load32S(index), ValueLane::S64) => Instruction::AccLoad32S64Addr64(index),
            (Instruction::I64Load32U(index), ValueLane::S64) => Instruction::AccLoad32U64Addr64(index),
            _ => return None,
        })
    }

    fn accumulator_store_instruction(instruction: Instruction) -> Option<(Instruction, Operand128Idx<MemoryOperand>)> {
        Some(match instruction {
            Instruction::I32Store(index) | Instruction::F32Store(index) => (Instruction::AccStore32(index), index),
            Instruction::I32Store8(index) => (Instruction::AccStore8_32(index), index),
            Instruction::I32Store16(index) => (Instruction::AccStore16_32(index), index),
            _ => return None,
        })
    }

    fn accumulator_store64_instruction(
        instruction: Instruction,
    ) -> Option<(Instruction, Operand128Idx<MemoryOperand>)> {
        Some(match instruction {
            Instruction::I64Store(index) | Instruction::F64Store(index) => (Instruction::AccStore64(index), index),
            Instruction::I64Store8(index) => (Instruction::AccStore8_64(index), index),
            Instruction::I64Store16(index) => (Instruction::AccStore16_64(index), index),
            Instruction::I64Store32(index) => (Instruction::AccStore32_64(index), index),
            _ => return None,
        })
    }

    fn register_stack_binary_instruction(op: RegisterOp32) -> Instruction {
        match op {
            RegisterOp32::Bin(BinOp::IAdd) => Instruction::AccI32AddStack,
            RegisterOp32::Bin(op) => Instruction::AccBinOpStack32(op),
            RegisterOp32::I32Cmp(op) => Instruction::AccI32CmpStack(op),
            RegisterOp32::F32Cmp(op) => Instruction::AccF32CmpStack(op),
        }
    }

    fn register_stack_stack_binary_instruction(op: RegisterOp32) -> Instruction {
        match op {
            RegisterOp32::Bin(op) => Instruction::AccBinOpStackStack32(op),
            RegisterOp32::I32Cmp(op) => Instruction::AccI32CmpStackStack(op),
            RegisterOp32::F32Cmp(op) => Instruction::AccF32CmpStackStack(op),
        }
    }

    fn top_is_accumulator32(&self) -> bool {
        self.operand_stack.last() == Some(&ValueLocation::Accumulator32)
    }

    fn top_is_accumulator64(&self) -> bool {
        self.operand_stack.last() == Some(&ValueLocation::Accumulator64)
    }

    fn top_is_accumulator_ref(&self) -> bool {
        self.operand_stack.last() == Some(&ValueLocation::AccumulatorRef)
    }

    fn accumulator32_is_live(&self) -> bool {
        self.accumulator32_owner.is_some()
    }

    fn accumulator64_is_live(&self) -> bool {
        self.accumulator64_owner.is_some()
    }

    fn accumulator_ref_is_live(&self) -> bool {
        self.accumulator_ref_owner.is_some()
    }

    fn load_top_reference_into_accumulator(&mut self) -> Result<bool> {
        match self.operand_stack.last().copied() {
            Some(ValueLocation::AccumulatorRef) => Ok(true),
            Some(ValueLocation::LocalRef(local)) => {
                self.materialize_accumulator_ref();
                self.replace_top_location(ValueLocation::AccumulatorRef)?;
                self.instructions.push(Instruction::AccRefLocalGet(local));
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn emit_numeric32_source(&mut self, location: ValueLocation) -> Result<()> {
        debug_assert!(location.lane() == ValueLane::S32);
        self.push_location(location)
    }

    fn emit_numeric64_source(&mut self, location: ValueLocation) -> Result<()> {
        debug_assert!(location.lane() == ValueLane::S64);
        self.push_location(location)
    }

    fn numeric32_source_instruction(location: ValueLocation) -> Instruction {
        match location {
            ValueLocation::LocalNumeric32(local) => Instruction::AccLocalGet32(local),
            ValueLocation::ConstNumeric32(value) => Instruction::AccConst32(value),
            _ => unreachable!(),
        }
    }

    fn numeric64_source_instruction(&mut self, location: ValueLocation) -> Result<Instruction> {
        Ok(match location {
            ValueLocation::LocalNumeric64(local) => Instruction::AccLocalGet64(local),
            ValueLocation::ConstNumeric64(value) => {
                Instruction::AccConst64(self.push_operand64(Operand64::<i64>::new(value))?)
            }
            _ => unreachable!(),
        })
    }

    fn numeric32_direct_instruction(op: RegisterOp32, location: ValueLocation) -> Instruction {
        register::direct_instruction(op, location)
    }

    fn materialize_all_scalars(&mut self) {
        let end = self.operand_stack.len();
        self.materialize_lane_prefix(ValueLane::S32, end);
        self.materialize_lane_prefix(ValueLane::S64, end);
    }

    fn materialize_lane(&mut self, lane: ValueLane) {
        self.materialize_lane_prefix(lane, self.operand_stack.len());
    }

    fn materialize_accumulator32(&mut self) {
        if let Some(owner) = self.accumulator32_owner {
            self.materialize_lane_prefix(ValueLane::S32, owner + 1);
        }
    }

    fn materialize_accumulator_ref(&mut self) {
        if let Some(owner) = self.accumulator_ref_owner {
            self.materialize_lane_prefix(ValueLane::S32, owner + 1);
        }
    }

    fn materialize_lane_prefix(&mut self, lane: ValueLane, end: usize) {
        if lane == ValueLane::S128 {
            return;
        }
        for index in 0..end {
            let value = self.operand_stack[index];
            if value.lane() != lane {
                continue;
            }
            match value {
                ValueLocation::Accumulator32 => {
                    let local_load = self.instructions.select_acc_local_load32();
                    let fused = local_load.and_then(|(op, memory, address)| {
                        let memory = CompactMemoryArg::try_from(self.data.operand128(memory)).ok()?;
                        let address = u8::try_from(address).ok()?;
                        let memory_arg_idx = self.push_operand64(Operand64::from(memory)).ok()?;
                        let argument = MemoryLocalArg { memory_arg_idx, local1: address, local2: 0 };
                        Some(match op {
                            LoadOp32::Full => Instruction::LoadLocal32(argument),
                            LoadOp32::I8S => Instruction::LoadLocal8S32(argument),
                            LoadOp32::I8U => Instruction::LoadLocal8U32(argument),
                            LoadOp32::I16S => Instruction::LoadLocal16S32(argument),
                            LoadOp32::I16U => Instruction::LoadLocal16U32(argument),
                        })
                    });
                    if let Some(instruction) = fused {
                        self.instructions.push(instruction);
                    } else {
                        if let Some((op, memory, address)) = local_load {
                            self.instructions.push(Instruction::AccLocalGet32(address));
                            self.instructions.push(match op {
                                LoadOp32::Full => Instruction::AccLoad32(memory),
                                LoadOp32::I8S => Instruction::AccLoad8S32(memory),
                                LoadOp32::I8U => Instruction::AccLoad8U32(memory),
                                LoadOp32::I16S => Instruction::AccLoad16S32(memory),
                                LoadOp32::I16U => Instruction::AccLoad16U32(memory),
                            });
                        }
                        self.instructions.push(Instruction::PushAcc32);
                    }
                    self.accumulator32_owner = None;
                }
                ValueLocation::LocalNumeric32(local) => self.instructions.push(Instruction::LocalGet32(local)),
                ValueLocation::ConstNumeric32(value) => self.instructions.push(Instruction::Const32(value)),
                ValueLocation::DeferredBinLocalLocal32 { op, left, right } => {
                    self.instructions.push(Instruction::BinOpLocalLocal32(op, left, right));
                }
                ValueLocation::DeferredBinLocalConst32 { op, local, value } => {
                    let argument = tinywasm_types::I32LocalArg { value, local };
                    let instruction = match op {
                        BinOp::IAdd => Instruction::AddLocalConst32(argument),
                        BinOp::ISub => Instruction::SubLocalConst32(argument),
                        BinOp::IMul => Instruction::MulLocalConst32(argument),
                        BinOp::IAnd => Instruction::AndLocalConst32(argument),
                        BinOp::IShrU => Instruction::ShrULocalConst32(argument),
                        _ => {
                            let operand = self
                                .push_operand64(Operand64::<(u16, u32)>::new(local, value as u32))
                                .expect("operand index overflow");
                            Instruction::BinOpLocalConst32(PackedOp::new(op, operand))
                        }
                    };
                    self.instructions.push(instruction);
                }
                ValueLocation::Accumulator64 => {
                    self.instructions.push(Instruction::PushAcc64);
                    self.accumulator64_owner = None;
                }
                ValueLocation::AccumulatorRef => {
                    self.instructions.push(Instruction::PushAccRef);
                    self.accumulator_ref_owner = None;
                }
                ValueLocation::LocalNumeric64(local) => self.instructions.push(Instruction::LocalGet64(local)),
                ValueLocation::LocalRef(local) => self.instructions.push(Instruction::LocalGet32(local)),
                ValueLocation::ConstNumeric64(value) => {
                    let operand = self.push_operand64(Operand64::<i64>::new(value)).expect("operand index overflow");
                    self.instructions.push(Instruction::Const64(operand));
                }
                ValueLocation::DeferredBinLocalLocal64 { op, left, right } => {
                    self.instructions.push(Instruction::BinOpLocalLocal64(op, left, right));
                }
                ValueLocation::DeferredBinLocalConst64 { op, local, value } => {
                    let operand = self
                        .push_operand128(Operand128::<(u16, u64)>::new(local, value as u64))
                        .expect("operand index overflow");
                    self.instructions.push(Instruction::BinOpLocalConst64(PackedOp::new(op, operand)));
                }
                ValueLocation::Stack(_) => continue,
            }
            self.operand_stack[index] = ValueLocation::Stack(lane);
        }
    }

    fn materialize_deferred_operands_before_local_write(&mut self, lane: ValueLane, local: u16) {
        if !self.operand_stack.iter().any(|value| match (lane, *value) {
            (ValueLane::S32, ValueLocation::LocalNumeric32(value)) => value == local,
            (ValueLane::S32, ValueLocation::LocalRef(value)) => value == local,
            (ValueLane::S64, ValueLocation::LocalNumeric64(value)) => value == local,
            (ValueLane::S32, ValueLocation::DeferredBinLocalLocal32 { left, right, .. }) => {
                left == local || right == local
            }
            (ValueLane::S32, ValueLocation::DeferredBinLocalConst32 { local: source, .. }) => source == local,
            (ValueLane::S64, ValueLocation::DeferredBinLocalLocal64 { left, right, .. }) => {
                left == local || right == local
            }
            (ValueLane::S64, ValueLocation::DeferredBinLocalConst64 { local: source, .. }) => source == local,
            _ => false,
        }) {
            return;
        }
        self.materialize_lane_prefix(lane, self.operand_stack.len().saturating_sub(1));
    }

    fn materialize_deferred_prefix(&mut self, end: usize) {
        self.materialize_lane_prefix(ValueLane::S32, end);
        self.materialize_lane_prefix(ValueLane::S64, end);
    }

    fn push_accumulator32(&mut self, lane: ValueLane) -> Result<()> {
        debug_assert!(lane == ValueLane::S32);
        self.push_location(ValueLocation::Accumulator32)
    }

    fn push_accumulator64(&mut self, lane: ValueLane) -> Result<()> {
        debug_assert!(lane == ValueLane::S64);
        self.push_location(ValueLocation::Accumulator64)
    }

    fn push_accumulator_ref(&mut self) -> Result<()> {
        self.push_location(ValueLocation::AccumulatorRef)
    }

    fn pop_accumulator32(&mut self, expected: ValueLane) -> Result<()> {
        let value = self
            .operand_stack
            .pop()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        if value.lane() != expected || value != ValueLocation::Accumulator32 {
            return Err(crate::ParseError::Other("logical register operand mismatch".into()));
        }
        debug_assert_eq!(self.accumulator32_owner, Some(self.operand_stack.len()));
        self.accumulator32_owner = None;
        self.decrement_lane_count(value.lane());
        Ok(())
    }

    fn pop_accumulator64(&mut self, expected: ValueLane) -> Result<()> {
        let value = self
            .operand_stack
            .pop()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        if value.lane() != expected || value != ValueLocation::Accumulator64 {
            return Err(crate::ParseError::Other("logical register operand mismatch".into()));
        }
        debug_assert_eq!(self.accumulator64_owner, Some(self.operand_stack.len()));
        self.accumulator64_owner = None;
        self.decrement_lane_count(value.lane());
        Ok(())
    }

    fn pop_accumulator_ref(&mut self) -> Result<()> {
        let value = self
            .operand_stack
            .pop()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        if value != ValueLocation::AccumulatorRef {
            return Err(crate::ParseError::Other("logical reference register operand mismatch".into()));
        }
        debug_assert_eq!(self.accumulator_ref_owner, Some(self.operand_stack.len()));
        self.accumulator_ref_owner = None;
        self.decrement_lane_count(ValueLane::S32);
        Ok(())
    }

    fn pop_deferred_value(&mut self, expected: ValueLane) -> Result<()> {
        let value = self
            .operand_stack
            .pop()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        let is_deferred = matches!(
            (expected, value),
            (ValueLane::S32, ValueLocation::LocalNumeric32(_) | ValueLocation::ConstNumeric32(_))
                | (ValueLane::S32, ValueLocation::LocalRef(_))
                | (ValueLane::S64, ValueLocation::LocalNumeric64(_) | ValueLocation::ConstNumeric64(_))
                | (
                    ValueLane::S32,
                    ValueLocation::DeferredBinLocalLocal32 { .. } | ValueLocation::DeferredBinLocalConst32 { .. },
                )
                | (
                    ValueLane::S64,
                    ValueLocation::DeferredBinLocalLocal64 { .. } | ValueLocation::DeferredBinLocalConst64 { .. },
                )
        );
        if value.lane() != expected || !is_deferred {
            return Err(crate::ParseError::Other("logical deferred operand mismatch".into()));
        }
        self.decrement_lane_count(value.lane());
        Ok(())
    }

    fn pop_stack_value(&mut self, expected: ValueLane) -> Result<()> {
        let frame_height = self.control_stack.last().map_or(0, |frame| frame.height);
        if self.operand_stack.len() == frame_height && self.is_unreachable() {
            return Ok(());
        }
        let value = self
            .operand_stack
            .pop()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        if value.lane() != expected || value != ValueLocation::Stack(expected) {
            return Err(crate::ParseError::Other("logical stack operand mismatch".into()));
        }
        self.decrement_lane_count(value.lane());
        Ok(())
    }

    fn decrement_lane_count(&mut self, lane: ValueLane) {
        match lane {
            ValueLane::S32 => self.lane_counts.c32 -= 1,
            ValueLane::S64 => self.lane_counts.c64 -= 1,
            ValueLane::S128 => self.lane_counts.c128 -= 1,
        }
    }

    fn local(&self, idx: u32) -> Result<(ValueLane, bool, u16)> {
        let size = *self
            .local_types
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("local index out of bounds: {idx}")))?;
        let addr = *self
            .local_addr_map
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("local address missing: {idx}")))?;
        let numeric32 = *self
            .local_numeric32
            .get(idx as usize)
            .ok_or_else(|| crate::ParseError::Other(alloc::format!("local type missing: {idx}")))?;
        Ok((size, numeric32, addr))
    }

    /// Pushes logical operands while maintaining the lane counts used by `DropKeep`.
    fn push_sizes(&mut self, sizes: &[ValueLane]) -> Result<()> {
        for &size in sizes {
            self.push_location(ValueLocation::Stack(size))?;
        }
        Ok(())
    }

    fn push_location(&mut self, location: ValueLocation) -> Result<()> {
        match location {
            ValueLocation::Accumulator32 if self.accumulator32_owner.is_some() => {
                return Err(crate::ParseError::Other("acc32 already contains a logical operand".into()));
            }
            ValueLocation::Accumulator64 if self.accumulator64_owner.is_some() => {
                return Err(crate::ParseError::Other("acc64 already contains a logical operand".into()));
            }
            ValueLocation::AccumulatorRef if self.accumulator_ref_owner.is_some() => {
                return Err(crate::ParseError::Other("acc_ref already contains a logical operand".into()));
            }
            _ => {}
        }
        let count = match location.lane() {
            ValueLane::S32 => &mut self.lane_counts.c32,
            ValueLane::S64 => &mut self.lane_counts.c64,
            ValueLane::S128 => &mut self.lane_counts.c128,
        };
        *count = count
            .checked_add(1)
            .ok_or_else(|| crate::ParseError::Other("logical operand lane count is too large".into()))?;
        let owner = self.operand_stack.len();
        self.operand_stack.push(location);
        match location {
            ValueLocation::Accumulator32 => self.accumulator32_owner = Some(owner),
            ValueLocation::Accumulator64 => self.accumulator64_owner = Some(owner),
            ValueLocation::AccumulatorRef => self.accumulator_ref_owner = Some(owner),
            _ => {}
        }
        Ok(())
    }

    fn replace_top_location(&mut self, location: ValueLocation) -> Result<()> {
        let previous = *self
            .operand_stack
            .last()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        if previous.lane() != location.lane() {
            return Err(crate::ParseError::Other("logical operand width mismatch".into()));
        }
        let owner = self.operand_stack.len() - 1;
        match location {
            ValueLocation::Accumulator32 if self.accumulator32_owner.is_some_and(|current| current != owner) => {
                return Err(crate::ParseError::Other("acc32 already contains a logical operand".into()));
            }
            ValueLocation::Accumulator64 if self.accumulator64_owner.is_some_and(|current| current != owner) => {
                return Err(crate::ParseError::Other("acc64 already contains a logical operand".into()));
            }
            ValueLocation::AccumulatorRef if self.accumulator_ref_owner.is_some_and(|current| current != owner) => {
                return Err(crate::ParseError::Other("acc_ref already contains a logical operand".into()));
            }
            _ => {}
        }
        match previous {
            ValueLocation::Accumulator32 => self.accumulator32_owner = None,
            ValueLocation::Accumulator64 => self.accumulator64_owner = None,
            ValueLocation::AccumulatorRef => self.accumulator_ref_owner = None,
            _ => {}
        }
        *self.operand_stack.last_mut().unwrap() = location;
        match location {
            ValueLocation::Accumulator32 => self.accumulator32_owner = Some(owner),
            ValueLocation::Accumulator64 => self.accumulator64_owner = Some(owner),
            ValueLocation::AccumulatorRef => self.accumulator_ref_owner = Some(owner),
            _ => {}
        }
        Ok(())
    }

    /// Pops an operand, allowing a polymorphic value at an unreachable frame base.
    fn pop_expect(&mut self, expected: ValueLane) -> Result<()> {
        self.materialize_lane(expected);
        let frame_height = self.control_stack.last().map_or(0, |frame| frame.height);
        if self.operand_stack.len() == frame_height && self.is_unreachable() {
            return Ok(());
        }
        let actual = self
            .operand_stack
            .pop()
            .ok_or_else(|| crate::ParseError::Other("logical operand stack underflow".into()))?;
        if actual.lane() != expected {
            return Err(crate::ParseError::Other("logical operand width mismatch".into()));
        }
        self.decrement_lane_count(actual.lane());
        Ok(())
    }

    /// Applies a declared logical stack effect in WebAssembly operand order.
    fn apply_effect(&mut self, inputs: &[ValueLane], outputs: &[ValueLane]) -> Result<()> {
        inputs.iter().rev().try_for_each(|&size| self.pop_expect(size))?;
        self.push_sizes(outputs)?;
        Ok(())
    }

    fn preserve_effect(&self, inputs: &[ValueLane], outputs: &[ValueLane]) -> Result<()> {
        if inputs != outputs {
            return Err(crate::ParseError::Other("preserved stack effect changes operand widths".into()));
        }
        let frame_height = self.control_stack.last().map_or(0, |frame| frame.height);
        for (offset, expected) in inputs.iter().rev().enumerate() {
            let Some(index) = self.operand_stack.len().checked_sub(offset + 1) else {
                if self.is_unreachable() {
                    return Ok(());
                }
                return Err(crate::ParseError::Other("logical operand stack underflow".into()));
            };
            if index < frame_height && self.is_unreachable() {
                return Ok(());
            }
            if self.operand_stack[index].lane() != *expected {
                return Err(crate::ParseError::Other("logical operand width mismatch".into()));
            }
        }
        Ok(())
    }

    fn emit_return_instruction(&mut self) -> Result<()> {
        let single_32 = self.control_stack[0].results.as_slice() == [ValueLane::S32];
        let single_64 = self.control_stack[0].results.as_slice() == [ValueLane::S64];
        let instruction =
            if single_32 && self.function_result_reference && self.load_top_reference_into_accumulator()? {
                Instruction::ReturnAccRef
            } else {
                match (single_32, single_64, self.operand_stack.last()) {
                    (true, _, Some(ValueLocation::Accumulator32)) => Instruction::ReturnAcc32,
                    (_, true, Some(ValueLocation::Accumulator64)) => Instruction::ReturnAcc64,
                    _ => {
                        self.materialize_all_scalars();
                        Instruction::Return
                    }
                }
            };
        match instruction {
            Instruction::ReturnAcc32 => self.pop_accumulator32(ValueLane::S32)?,
            Instruction::ReturnAcc64 => self.pop_accumulator64(ValueLane::S64)?,
            Instruction::ReturnAccRef => self.pop_accumulator_ref()?,
            _ => {}
        }
        if self.accumulator_ref_is_live() {
            self.instructions.push(Instruction::ClearAccRef);
        }
        self.discard_accumulator_owners();
        self.instructions.push(instruction);
        Ok(())
    }

    /// Applies an instruction's stack effect before adding it to the bytecode.
    fn emit(&mut self, inputs: &[ValueLane], outputs: &[ValueLane], instruction: Instruction) -> Result<()> {
        self.emit_stack(inputs, outputs, instruction)
    }

    fn emit_memory(&mut self, inputs: &[ValueLane], outputs: &[ValueLane], instruction: Instruction) -> Result<()> {
        if self.has_polymorphic_inputs(inputs.len()) {
            return self.apply_effect(inputs, outputs);
        }
        if self.try_emit_register_memory(instruction)? {
            return Ok(());
        }
        self.emit_stack(inputs, outputs, instruction)
    }

    fn emit_stack(&mut self, inputs: &[ValueLane], outputs: &[ValueLane], instruction: Instruction) -> Result<()> {
        if matches!(
            instruction,
            Instruction::Return
                | Instruction::Call(_)
                | Instruction::CallSelf
                | Instruction::CallIndirect(_)
                | Instruction::CallRef(_)
                | Instruction::ReturnCall(_)
                | Instruction::ReturnCallSelf
                | Instruction::ReturnCallIndirect(_)
                | Instruction::ReturnCallRef(_)
        ) {
            self.materialize_all_scalars();
        } else {
            for &lane in inputs.iter().chain(outputs) {
                self.materialize_lane(lane);
            }
        }
        self.apply_effect(inputs, outputs)?;
        self.instructions.push(instruction);
        Ok(())
    }

    /// Restores both logical operand order and lane counts to a control-frame base.
    fn reset_stack(&mut self, height: usize, base: ValueCounts) {
        debug_assert!(!self.accumulator32_is_live());
        debug_assert!(!self.accumulator64_is_live());
        debug_assert!(!self.accumulator_ref_is_live());
        self.operand_stack.truncate(height);
        self.lane_counts = base;
    }

    fn discard_accumulator_owners(&mut self) {
        self.accumulator32_owner = None;
        self.accumulator64_owner = None;
        self.accumulator_ref_owner = None;
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

    /// Enters a control frame with its parameters restored above the saved base.
    fn push_control(&mut self, kind: BlockKind, ty: wasmparser::BlockType, else_label: Option<LabelId>) -> Result<()> {
        let signature = self.block_signature(ty)?;
        self.push_control_signature(kind, signature, else_label)
    }

    fn block_signature(&self, ty: wasmparser::BlockType) -> Result<Signature> {
        Ok(match ty {
            wasmparser::BlockType::Empty => Signature {
                params: Vec::new(),
                params_numeric32: Vec::new(),
                results: Vec::new(),
                results_reference: Vec::new(),
            },
            wasmparser::BlockType::Type(ty) => Signature {
                params: Vec::new(),
                params_numeric32: Vec::new(),
                results: alloc::vec![value_lane(ty)],
                results_reference: alloc::vec![matches!(ty, wasmparser::ValType::Ref(_))],
            },
            wasmparser::BlockType::FuncType(idx) => self.metadata.signature(idx)?.clone(),
        })
    }

    fn push_control_signature(
        &mut self,
        kind: BlockKind,
        signature: Signature,
        else_label: Option<LabelId>,
    ) -> Result<()> {
        self.materialize_all_scalars();
        for &size in signature.params.iter().rev() {
            self.pop_expect(size)?;
        }
        let height = self.operand_stack.len();
        let base = self.lane_counts;
        self.push_sizes(&signature.params)?;
        let entry_unreachable = self.is_unreachable();
        let start_label = self.labels.new_label();
        self.labels.pin(start_label, self.instructions.len())?;
        let end_label = self.labels.new_label();
        self.control_stack.push(ControlFrame {
            kind,
            has_else: false,
            start_label,
            end_label,
            else_label,
            height,
            base,
            params: signature.params,
            results: signature.results,
            unreachable: entry_unreachable,
            entry_unreachable,
            end_reachable: false,
        });
        Ok(())
    }

    /// Emits the stack-shaping instruction required by a branch.
    fn emit_dropkeep(&mut self, base: ValueCounts, keep: ValueCounts) {
        self.materialize_all_scalars();
        let target = ValueCounts { c32: base.c32 + keep.c32, c64: base.c64 + keep.c64, c128: base.c128 + keep.c128 };
        if self.lane_counts.c32 != target.c32 {
            self.instructions.push(Instruction::DropKeep32 { base: base.c32, keep: keep.c32 });
        }
        if self.lane_counts.c64 != target.c64 {
            self.instructions.push(Instruction::DropKeep64 { base: base.c64, keep: keep.c64 });
        }
        if self.lane_counts.c128 != target.c128 {
            self.instructions.push(Instruction::DropKeep128 { base: base.c128, keep: keep.c128 });
        }
    }

    fn emit_labeled_instruction(&mut self, instruction: Instruction, target: LabelId) {
        let index = self.instructions.len();
        self.instructions.push(instruction);
        self.labels.use_instruction(target, index);
    }

    fn try_emit_selected_i32_branch(&mut self, target: LabelId, branch_on_nonzero: bool) -> Result<bool> {
        let Some(selection) = self.instructions.select_i32_branch(branch_on_nonzero) else {
            return Ok(false);
        };
        let instruction = match selection {
            I32BranchSelection::AccEqz => {
                let instruction = if branch_on_nonzero {
                    Instruction::JumpIfAccZero32(0)
                } else {
                    Instruction::JumpIfAccNonZero32(0)
                };
                self.emit_labeled_instruction(instruction, target);
                return Ok(true);
            }
            I32BranchSelection::Local(local) => {
                let instruction = if branch_on_nonzero {
                    Instruction::JumpIfLocalNonZero32(TargetLocalArg { target_ip: 0, local })
                } else {
                    Instruction::JumpIfLocalZero32(TargetLocalArg { target_ip: 0, local })
                };
                self.emit_labeled_instruction(instruction, target);
                return Ok(true);
            }
            I32BranchSelection::UpdateLocal { bin_op, value, local, on_zero } => {
                let operand = self
                    .data
                    .push_target_operand128(Operand128::<LocalUpdateOperand>::new(0, value, local, on_zero))?;
                self.labels.use_operand128(target, operand.index());
                if bin_op == BinOp::IAdd {
                    Instruction::IncLocalJump32(operand)
                } else {
                    Instruction::BinOpLocalConstJump32(PackedOp::new(bin_op, operand))
                }
            }
            I32BranchSelection::UpdateCompareLocal { bin_op, value, local, right, cmp_op } => {
                let operand = self
                    .data
                    .push_target_operand128(Operand128::<LocalUpdateCmpOperand>::new(0, value, local, right))?;
                self.labels.use_operand128(target, operand.index());
                if bin_op == BinOp::IAdd {
                    Instruction::IncLocalJumpCmpLocal32(PackedOp::new(cmp_op, operand))
                } else {
                    Instruction::BinOpLocalConstJumpCmpLocal32(PackedOp::new((bin_op, cmp_op), operand))
                }
            }
            I32BranchSelection::CompareLocalLocal { left, right, cmp_op } => {
                let operand = self.data.push_target_operand64(Operand64::<(u32, u16, u16)>::new(0, left, right))?;
                self.labels.use_operand64(target, operand.index());
                Instruction::JumpCmpLocalLocal32(PackedOp::new(cmp_op, operand))
            }
            I32BranchSelection::CompareLocalConst { operand, cmp_op } => {
                let source = self.data.operand64(operand);
                let operand = self.data.push_target_operand128(Operand128::<(u32, i32, u16)>::new(
                    0,
                    source.b() as i32,
                    source.a(),
                ))?;
                self.labels.use_operand128(target, operand.index());
                Instruction::JumpCmpLocalConst32(PackedOp::new(cmp_op, operand))
            }
            I32BranchSelection::CompareStackLocal { local, cmp_op } => {
                let operand = self.data.push_target_operand64(Operand64::<(u32, u16)>::new(0, local))?;
                self.labels.use_operand64(target, operand.index());
                Instruction::JumpCmpStackLocal32(PackedOp::new(cmp_op, operand))
            }
            I32BranchSelection::CompareStackConst { value, cmp_op } => {
                let operand = self.data.push_target_operand64(Operand64::<(u32, i32)>::new(0, value))?;
                self.labels.use_operand64(target, operand.index());
                Instruction::JumpCmpStackConst32(PackedOp::new(cmp_op, operand))
            }
        };
        self.instructions.push(instruction);
        Ok(true)
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

    /// Shapes stack lanes to the values consumed by a branch target.
    fn emit_dropkeep_to_label(&mut self, label_depth: u32) -> Result<()> {
        if self.is_unreachable() {
            return Ok(());
        }
        let frame = &self.control_stack[self.get_ctx_idx(label_depth)?];
        let base = frame.base;
        let label_types = if matches!(frame.kind, BlockKind::Loop) { &frame.params } else { &frame.results };
        self.emit_dropkeep(base, Self::value_counts(label_types));
        Ok(())
    }

    fn branch_needs_dropkeep(&self, depth: u32) -> Result<bool> {
        if self.is_unreachable() {
            return Ok(false);
        }
        let frame = &self.control_stack[self.get_ctx_idx(depth)?];
        let keep =
            Self::value_counts(if matches!(frame.kind, BlockKind::Loop) { &frame.params } else { &frame.results });
        let target = ValueCounts {
            c32: frame.base.c32 + keep.c32,
            c64: frame.base.c64 + keep.c64,
            c128: frame.base.c128 + keep.c128,
        };
        Ok(self.lane_counts.c32 != target.c32
            || self.lane_counts.c64 != target.c64
            || self.lane_counts.c128 != target.c128)
    }

    fn branch_target(&mut self, depth: u32) -> Result<Option<LabelId>> {
        let ctx_idx = self.get_ctx_idx(depth)?;
        let frame = &self.control_stack[ctx_idx];
        let target = match frame.kind {
            BlockKind::Function => None,
            BlockKind::Loop => Some(frame.start_label),
            BlockKind::Block | BlockKind::If | BlockKind::TryTable => Some(frame.end_label),
        };
        if matches!(frame.kind, BlockKind::Block | BlockKind::If | BlockKind::TryTable) {
            self.control_stack[ctx_idx].end_reachable = true;
        }
        Ok(target)
    }

    fn emit_branch_jump_or_return(&mut self, depth: u32) -> Result<()> {
        self.materialize_all_scalars();
        match self.branch_target(depth)? {
            Some(target) => self.emit_labeled_instruction(Instruction::Jump(0), target),
            None => self.instructions.push(Instruction::Return),
        }
        Ok(())
    }
}
