use crate::validation::{FuncValidator, FuncValidatorAllocations, ValidatorResources};
#[cfg(feature = "validate")]
use crate::visit::process_operators_and_validate;
use crate::{Result, module::FunctionCode, visit::process_operators};
use alloc::{boxed::Box, format, vec::Vec};
use tinywasm_types::*;
use wasmparser::{CompositeInnerType, OperatorsReader, OperatorsReaderAllocations, UnpackedIndex};

pub(crate) fn value_lane(ty: wasmparser::ValType) -> ValueLane {
    match ty {
        wasmparser::ValType::I32 | wasmparser::ValType::F32 | wasmparser::ValType::Ref(_) => ValueLane::S32,
        wasmparser::ValType::I64 | wasmparser::ValType::F64 => ValueLane::S64,
        wasmparser::ValType::V128 => ValueLane::S128,
    }
}

pub(crate) fn convert_module_element(element: wasmparser::Element<'_>) -> Result<tinywasm_types::Element> {
    let kind = match element.kind {
        wasmparser::ElementKind::Active { table_index, offset_expr } => tinywasm_types::ElementKind::Active {
            table: table_index.unwrap_or(0),
            offset: process_const_operators(offset_expr.get_operators_reader())?,
        },
        wasmparser::ElementKind::Passive => tinywasm_types::ElementKind::Passive,
        wasmparser::ElementKind::Declared => tinywasm_types::ElementKind::Declared,
    };

    match element.items {
        wasmparser::ElementItems::Functions(funcs) => {
            let items = funcs
                .into_iter()
                .map(|func| Ok(ElementItem::Func(func?)))
                .collect::<Result<Vec<_>>>()?
                .into_boxed_slice();

            Ok(tinywasm_types::Element { kind, items, ty: RefType::FUNCREF, range: element.range })
        }

        wasmparser::ElementItems::Expressions(ty, exprs) => {
            let items = exprs
                .into_iter()
                .map(|expr| Ok(ElementItem::Expr(process_const_operators(expr?.get_operators_reader())?)))
                .collect::<Result<Vec<_>>>()?
                .into_boxed_slice();

            Ok(tinywasm_types::Element { kind, items, ty: convert_ref_type(ty)?, range: element.range })
        }
    }
}

pub(crate) fn convert_module_data(data: wasmparser::Data<'_>) -> Result<tinywasm_types::Data> {
    Ok(tinywasm_types::Data {
        data: data.data.to_vec().into_boxed_slice(),
        range: data.range,
        kind: match data.kind {
            wasmparser::DataKind::Active { memory_index, offset_expr } => {
                let offset = process_const_operators(offset_expr.get_operators_reader())?;
                tinywasm_types::DataKind::Active { mem: memory_index, offset }
            }
            wasmparser::DataKind::Passive => tinywasm_types::DataKind::Passive,
        },
    })
}

pub(crate) fn convert_module_import(import: wasmparser::Import<'_>) -> Result<Import> {
    let kind = match import.ty {
        wasmparser::TypeRef::Func(ty) => ImportKind::Function(ty),
        wasmparser::TypeRef::Table(ty) => {
            let element_type = convert_ref_type(ty.element_type)?;
            ImportKind::Table(if ty.table64 {
                TableType::new64(element_type, ty.initial, ty.maximum)
            } else {
                TableType::new(element_type, ty.initial, ty.maximum)
            })
        }
        wasmparser::TypeRef::Memory(ty) => ImportKind::Memory(convert_module_memory(ty)),
        wasmparser::TypeRef::Global(ty) => {
            ImportKind::Global(GlobalType::new(convert_valtype(&ty.content_type)?, ty.mutable))
        }
        wasmparser::TypeRef::Tag(ty) => ImportKind::Tag(convert_tag_type(ty)),
        _ => {
            return Err(crate::ParseError::UnsupportedOperator(format!("Unsupported import kind: {:?}", import.ty)));
        }
    };

    Ok(Import { module: import.module.into(), name: import.name.into(), kind })
}

pub(crate) fn convert_module_memory(memory: wasmparser::MemoryType) -> MemoryType {
    MemoryType::new(
        if memory.memory64 { MemoryArch::I64 } else { MemoryArch::I32 },
        memory.initial,
        memory.maximum,
        memory.page_size_log2.map(|x| 1 << x),
    )
}

pub(crate) fn convert_module_globals(
    globals: wasmparser::SectionLimited<'_, wasmparser::Global<'_>>,
) -> Result<Box<[Global]>> {
    globals
        .into_iter()
        .map(|global| {
            let global = global?;
            let ty = convert_valtype(&global.ty.content_type)?;
            let ops = global.init_expr.get_operators_reader();
            Ok(Global { init: process_const_operators(ops)?, ty: GlobalType::new(ty, global.ty.mutable) })
        })
        .collect::<Result<Box<_>>>()
}

pub(crate) fn convert_module_export(export: wasmparser::Export<'_>) -> Result<Export> {
    let kind = match export.kind {
        wasmparser::ExternalKind::Func => ExternalKind::Func,
        wasmparser::ExternalKind::Table => ExternalKind::Table,
        wasmparser::ExternalKind::Memory => ExternalKind::Memory,
        wasmparser::ExternalKind::Global => ExternalKind::Global,
        wasmparser::ExternalKind::Tag => ExternalKind::Tag,
        wasmparser::ExternalKind::FuncExact => {
            return Err(crate::ParseError::UnsupportedOperator(format!("Unsupported export kind: {:?}", export.kind)));
        }
    };

    Ok(Export { index: export.index, name: Box::from(export.name), kind })
}

pub(crate) const fn convert_tag_type(ty: wasmparser::TagType) -> TagType {
    TagType::new(ty.func_type_idx)
}

fn extend_local_types(local_types: &mut Vec<ValueLane>, count: u32, ty: wasmparser::ValType) -> Result<()> {
    let size = value_lane(ty);
    let count =
        usize::try_from(count).map_err(|_| crate::ParseError::Other("local declaration count is too large".into()))?;
    local_types.reserve(count);
    local_types.extend(core::iter::repeat_n(size, count));
    Ok(())
}

pub(crate) fn convert_module_code(
    func: wasmparser::FunctionBody<'_>,
    validator: Option<FuncValidator<ValidatorResources>>,
    reader_allocs: OperatorsReaderAllocations,
    metadata: &crate::visit::ModuleMetadata,
    ty_idx: u32,
) -> Result<(FunctionCode, Option<FuncValidatorAllocations>, OperatorsReaderAllocations)> {
    let locals_reader = func.get_locals_reader()?;
    #[cfg(feature = "validate")]
    let locals_position = locals_reader.original_position();
    let signature = metadata.signature(ty_idx)?.clone();
    let mut local_types = signature.params.clone();

    #[cfg(feature = "validate")]
    let mut validator = validator;

    #[cfg(feature = "validate")]
    for (local_index, local) in locals_reader.into_iter().enumerate() {
        let local = local?;
        if let Some(validator) = validator.as_mut() {
            validator.define_locals(locals_position + local_index, local.0, local.1)?;
        }
        extend_local_types(&mut local_types, local.0, local.1)?;
    }

    #[cfg(not(feature = "validate"))]
    for local in locals_reader {
        let local = local?;
        extend_local_types(&mut local_types, local.0, local.1)?;
    }

    // maps a local's address to the index in the type's locals array
    let mut local_addr_map = Vec::with_capacity(local_types.len());
    let mut local_counts = ValueCounts::default();

    for ty in &local_types {
        let (count, error) = match ty {
            ValueLane::S32 => (&mut local_counts.c32, "too many 32-bit locals"),
            ValueLane::S64 => (&mut local_counts.c64, "too many 64-bit locals"),
            ValueLane::S128 => (&mut local_counts.c128, "too many 128-bit locals"),
        };
        local_addr_map.push(*count);
        *count = count.checked_add(1).ok_or_else(|| crate::ParseError::Other(error.into()))?;
    }

    #[cfg(feature = "validate")]
    let (body, data, validator_allocs, reader_allocs) = match validator {
        Some(validator) => {
            let (body, data, validator_allocs, reader_allocs) = process_operators_and_validate(
                validator,
                func,
                local_types,
                local_addr_map,
                metadata,
                ty_idx,
                reader_allocs,
            )?;
            (body, data, Some(validator_allocs), reader_allocs)
        }
        None => {
            let (body, data, reader_allocs) =
                process_operators(func, local_types, local_addr_map, metadata, ty_idx, reader_allocs)?;
            (body, data, None, reader_allocs)
        }
    };
    #[cfg(not(feature = "validate"))]
    let (body, data, validator_allocs, reader_allocs) = {
        let _ = validator;
        let (body, data, reader_allocs) =
            process_operators(func, local_types, local_addr_map, metadata, ty_idx, reader_allocs)?;
        (body, data, None, reader_allocs)
    };
    Ok((
        FunctionCode { instructions: body, data, locals: local_counts, uses_local_memory: false },
        validator_allocs,
        reader_allocs,
    ))
}

pub(crate) fn convert_rec_group(ty: wasmparser::RecGroup, group_start: u32, types: &mut Vec<SubType>) -> Result<u32> {
    let group_len = u32::try_from(ty.types().len())
        .map_err(|_| crate::ParseError::Other("recursive type group is too large".into()))?;
    types.reserve(group_len as usize);
    for ty in ty.into_types() {
        let composite = &ty.composite_type;
        if composite.shared {
            return Err(crate::ParseError::UnsupportedOperator("shared composite types are unsupported".into()));
        }
        if composite.descriptor_idx.is_some() || composite.describes_idx.is_some() {
            return Err(crate::ParseError::UnsupportedOperator("descriptor types are unsupported".into()));
        }

        let supertype =
            ty.supertype_idx.map(|idx| convert_type_index(idx.unpack(), group_start, group_len)).transpose()?;
        let composite = match &composite.inner {
            CompositeInnerType::Func(ty) => {
                let params = ty
                    .params()
                    .iter()
                    .map(|ty| convert_valtype_in_group(ty, group_start, group_len))
                    .collect::<Result<Vec<_>>>()?;
                let results = ty
                    .results()
                    .iter()
                    .map(|ty| convert_valtype_in_group(ty, group_start, group_len))
                    .collect::<Result<Vec<_>>>()?;
                CompositeType::Func(FuncType::new(&params, &results))
            }
            CompositeInnerType::Struct(ty) => CompositeType::Struct(StructType {
                fields: ty
                    .fields
                    .iter()
                    .map(|field| convert_field_type(field, group_start, group_len))
                    .collect::<Result<_>>()?,
            }),
            CompositeInnerType::Array(ty) => {
                CompositeType::Array(ArrayType { field: convert_field_type(&ty.0, group_start, group_len)? })
            }
            CompositeInnerType::Cont(_) => {
                return Err(crate::ParseError::UnsupportedOperator("continuation types are unsupported".into()));
            }
        };
        types.push(SubType { is_final: ty.is_final, supertype, composite });
    }
    Ok(group_len)
}

fn convert_type_index(index: UnpackedIndex, group_start: u32, group_len: u32) -> Result<TypeAddr> {
    match index {
        UnpackedIndex::Module(index) => Ok(index),
        UnpackedIndex::RecGroup(index) if index < group_len => {
            group_start.checked_add(index).ok_or_else(|| crate::ParseError::Other("type index is too large".into()))
        }
        UnpackedIndex::RecGroup(index) => {
            Err(crate::ParseError::Other(format!("recursive group type index out of bounds: {index}")))
        }
        #[cfg(feature = "validate")]
        UnpackedIndex::Id(_) => {
            Err(crate::ParseError::UnsupportedOperator(format!("unsupported canonical type index: {index}")))
        }
    }
}

fn convert_field_type(field: &wasmparser::FieldType, group_start: u32, group_len: u32) -> Result<FieldType> {
    let storage = match &field.element_type {
        wasmparser::StorageType::I8 => StorageType::I8,
        wasmparser::StorageType::I16 => StorageType::I16,
        wasmparser::StorageType::Val(ty) => StorageType::Value(convert_valtype_in_group(ty, group_start, group_len)?),
    };
    Ok(FieldType { storage, mutable: field.mutable })
}

pub(crate) fn convert_ref_type(ty: wasmparser::RefType) -> Result<RefType> {
    convert_heap_type(ty.heap_type(), ty.is_nullable())
}

pub(crate) fn convert_valtype(valtype: &wasmparser::ValType) -> Result<WasmType> {
    convert_valtype_with_group(valtype, None)
}

fn convert_valtype_in_group(valtype: &wasmparser::ValType, group_start: u32, group_len: u32) -> Result<WasmType> {
    convert_valtype_with_group(valtype, Some((group_start, group_len)))
}

fn convert_valtype_with_group(valtype: &wasmparser::ValType, group: Option<(u32, u32)>) -> Result<WasmType> {
    match valtype {
        wasmparser::ValType::I32 => Ok(WasmType::I32),
        wasmparser::ValType::I64 => Ok(WasmType::I64),
        wasmparser::ValType::F32 => Ok(WasmType::F32),
        wasmparser::ValType::F64 => Ok(WasmType::F64),
        wasmparser::ValType::V128 => Ok(WasmType::V128),
        wasmparser::ValType::Ref(r) => {
            Ok(WasmType::Ref(convert_heap_type_with_group(r.heap_type(), r.is_nullable(), group)?))
        }
    }
}

pub(crate) fn process_const_operators(ops: OperatorsReader<'_>) -> Result<Box<[ConstInstruction]>> {
    let mut out = Vec::new();
    let mut operator_count = 0;
    let mut end_reached = false;

    for op in ops {
        let op = op?;
        operator_count += 1;
        if matches!(op, wasmparser::Operator::End) {
            end_reached = true;
            break;
        }

        let instr = match op {
            wasmparser::Operator::RefNull { hty } => {
                convert_heap_type(hty, false)?;
                ConstInstruction::Ref(RefValue::Null)
            }
            wasmparser::Operator::RefFunc { function_index } => {
                ConstInstruction::Ref(RefValue::Func(FuncRef::new(function_index)))
            }
            wasmparser::Operator::RefI31 => ConstInstruction::RefI31,
            wasmparser::Operator::AnyConvertExtern => ConstInstruction::AnyConvertExtern,
            wasmparser::Operator::ExternConvertAny => ConstInstruction::ExternConvertAny,
            wasmparser::Operator::StructNew { struct_type_index } => ConstInstruction::StructNew(struct_type_index),
            wasmparser::Operator::StructNewDefault { struct_type_index } => {
                ConstInstruction::StructNewDefault(struct_type_index)
            }
            wasmparser::Operator::ArrayNew { array_type_index } => ConstInstruction::ArrayNew(array_type_index),
            wasmparser::Operator::ArrayNewDefault { array_type_index } => {
                ConstInstruction::ArrayNewDefault(array_type_index)
            }
            wasmparser::Operator::ArrayNewFixed { array_type_index, array_size } => {
                ConstInstruction::ArrayNewFixed(array_type_index, array_size)
            }
            wasmparser::Operator::I32Const { value } => ConstInstruction::I32Const(value),
            wasmparser::Operator::I64Const { value } => ConstInstruction::I64Const(value),
            wasmparser::Operator::F32Const { value } => ConstInstruction::F32Const(f32::from_bits(value.bits())),
            wasmparser::Operator::F64Const { value } => ConstInstruction::F64Const(f64::from_bits(value.bits())),
            wasmparser::Operator::V128Const { value } => ConstInstruction::V128Const(*value.bytes()),
            wasmparser::Operator::GlobalGet { global_index } => ConstInstruction::GlobalGet(global_index),
            wasmparser::Operator::I32Add => ConstInstruction::I32Add,
            wasmparser::Operator::I32Sub => ConstInstruction::I32Sub,
            wasmparser::Operator::I32Mul => ConstInstruction::I32Mul,
            wasmparser::Operator::I64Add => ConstInstruction::I64Add,
            wasmparser::Operator::I64Sub => ConstInstruction::I64Sub,
            wasmparser::Operator::I64Mul => ConstInstruction::I64Mul,
            other => {
                return Err(crate::ParseError::UnsupportedOperator(format!(
                    "Unsupported const instruction: {other:?}"
                )));
            }
        };
        out.push(instr);
    }

    if operator_count < 2 || !end_reached {
        return Err(crate::ParseError::Other("constant expression did not end correctly".into()));
    }

    Ok(out.into_boxed_slice())
}

pub(crate) fn convert_heap_type(heap: wasmparser::HeapType, nullable: bool) -> Result<RefType> {
    convert_heap_type_with_group(heap, nullable, None)
}

fn convert_heap_type_with_group(
    heap: wasmparser::HeapType,
    nullable: bool,
    group: Option<(u32, u32)>,
) -> Result<RefType> {
    match heap {
        wasmparser::HeapType::Abstract { shared: false, ty } => Ok(RefType::new_abstract(
            nullable,
            match ty {
                wasmparser::AbstractHeapType::Any => AbstractHeapType::Any,
                wasmparser::AbstractHeapType::Eq => AbstractHeapType::Eq,
                wasmparser::AbstractHeapType::I31 => AbstractHeapType::I31,
                wasmparser::AbstractHeapType::Struct => AbstractHeapType::Struct,
                wasmparser::AbstractHeapType::Array => AbstractHeapType::Array,
                wasmparser::AbstractHeapType::None => AbstractHeapType::None,
                wasmparser::AbstractHeapType::Func => AbstractHeapType::Func,
                wasmparser::AbstractHeapType::NoFunc => AbstractHeapType::NoFunc,
                wasmparser::AbstractHeapType::Exn => AbstractHeapType::Exn,
                wasmparser::AbstractHeapType::NoExn => AbstractHeapType::NoExn,
                wasmparser::AbstractHeapType::Extern => AbstractHeapType::Extern,
                wasmparser::AbstractHeapType::NoExtern => AbstractHeapType::NoExtern,
                wasmparser::AbstractHeapType::Cont | wasmparser::AbstractHeapType::NoCont => {
                    return Err(crate::ParseError::UnsupportedOperator(format!("Unsupported heap type: {heap:?}")));
                }
            },
        )),
        wasmparser::HeapType::Concrete(index) => {
            let index = match index {
                UnpackedIndex::Module(index) => index,
                index @ UnpackedIndex::RecGroup(_) => {
                    let (group_start, group_len) = group.ok_or_else(|| {
                        crate::ParseError::UnsupportedOperator(format!(
                            "recursive-group heap type outside a type group: {index}"
                        ))
                    })?;
                    convert_type_index(index, group_start, group_len)?
                }
                #[cfg(feature = "validate")]
                index @ UnpackedIndex::Id(_) => {
                    return Err(crate::ParseError::UnsupportedOperator(format!(
                        "unsupported canonical heap type index: {index}"
                    )));
                }
            };
            RefType::new_concrete(nullable, index)
                .ok_or_else(|| crate::ParseError::Other(format!("heap type index is too large: {index}")))
        }
        wasmparser::HeapType::Abstract { shared: true, .. } | wasmparser::HeapType::Exact(_) => {
            Err(crate::ParseError::UnsupportedOperator(format!("Unsupported heap type: {heap:?}")))
        }
    }
}
