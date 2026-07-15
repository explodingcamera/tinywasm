use alloc::sync::Arc;

use crate::{Result, module::FunctionCode, visit::process_operators_and_validate};
use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use tinywasm_types::*;
use wasmparser::{
    CompositeInnerType, FuncValidator, FuncValidatorAllocations, OperatorsReader, OperatorsReaderAllocations,
    ValidatorResources,
};

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

            Ok(tinywasm_types::Element { kind, items, ty: WasmType::RefFunc, range: element.range })
        }

        wasmparser::ElementItems::Expressions(ty, exprs) => {
            let items = exprs
                .into_iter()
                .map(|expr| Ok(ElementItem::Expr(process_const_operators(expr?.get_operators_reader())?)))
                .collect::<Result<Vec<_>>>()?
                .into_boxed_slice();

            Ok(tinywasm_types::Element { kind, items, ty: convert_reftype(ty)?, range: element.range })
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
            let element_type = convert_reftype(ty.element_type)?;
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
        wasmparser::TypeRef::Tag(ty) => {
            return Err(crate::ParseError::UnsupportedOperator(format!("Unsupported import kind: {ty:?}")));
        }
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
        wasmparser::ExternalKind::Tag | wasmparser::ExternalKind::FuncExact => {
            return Err(crate::ParseError::UnsupportedOperator(format!("Unsupported export kind: {:?}", export.kind)));
        }
    };

    Ok(Export { index: export.index, name: Box::from(export.name), kind })
}

pub(crate) fn convert_module_code(
    func: wasmparser::FunctionBody<'_>,
    mut validator: Option<FuncValidator<ValidatorResources>>,
    reader_allocs: OperatorsReaderAllocations,
    metadata: &crate::visit::ModuleMetadata,
    ty_idx: u32,
) -> Result<(FunctionCode, Option<FuncValidatorAllocations>, OperatorsReaderAllocations)> {
    let locals_reader = func.get_locals_reader()?;
    let pos = locals_reader.original_position();
    let signature = metadata.signature(ty_idx)?.clone();
    let mut local_types = signature.params.clone();

    for (i, local) in locals_reader.into_iter().enumerate() {
        let local = local?;
        if let Some(validator) = validator.as_mut() {
            validator.define_locals(pos + i, local.0, local.1)?;
        }
        let size = crate::visit::OperandSize::from(local.1);
        let count = usize::try_from(local.0)
            .map_err(|_| crate::ParseError::Other("local declaration count is too large".into()))?;
        local_types.reserve(count);
        local_types.extend(core::iter::repeat_n(size, count));
    }

    // maps a local's address to the index in the type's locals array
    let mut local_addr_map = Vec::with_capacity(local_types.len());
    let mut local_counts = ValueCounts::default();

    for ty in &local_types {
        let (count, error) = match ty {
            crate::visit::OperandSize::S32 => (&mut local_counts.c32, "too many 32-bit locals"),
            crate::visit::OperandSize::S64 => (&mut local_counts.c64, "too many 64-bit locals"),
            crate::visit::OperandSize::S128 => (&mut local_counts.c128, "too many 128-bit locals"),
        };
        local_addr_map.push(*count);
        *count = count.checked_add(1).ok_or_else(|| crate::ParseError::Other(error.into()))?;
    }

    let (body, data, validator_allocs, reader_allocs) =
        process_operators_and_validate(validator, func, local_types, local_addr_map, metadata, ty_idx, reader_allocs)?;
    Ok((
        FunctionCode { instructions: body, data, locals: local_counts, uses_local_memory: false },
        validator_allocs,
        reader_allocs,
    ))
}

pub(crate) fn convert_module_type(ty: wasmparser::RecGroup) -> Result<Arc<FuncType>> {
    let mut types = ty.types();
    if types.len() != 1 {
        return Err(crate::ParseError::UnsupportedOperator(
            "Expected exactly one type in the type section".to_string(),
        ));
    }

    let ty = types.next().unwrap();
    let CompositeInnerType::Func(ty) = &ty.composite_type.inner else {
        return Err(crate::ParseError::UnsupportedOperator(format!(
            "Unsupported non-function type in type section: {}",
            ty.composite_type
        )));
    };
    let params: Vec<_> = ty.params().iter().map(convert_valtype).collect();
    let params = params.into_iter().collect::<Result<Vec<_>>>()?;
    let results = ty.results().iter().map(convert_valtype).collect::<Result<Vec<_>>>()?;
    Ok(FuncType::new(&params, &results).into())
}

pub(crate) fn convert_reftype(reftype: wasmparser::RefType) -> Result<WasmType> {
    match reftype {
        _ if reftype.is_func_ref() => Ok(WasmType::RefFunc),
        _ if reftype.is_extern_ref() => Ok(WasmType::RefExtern),
        _ => Err(crate::ParseError::UnsupportedOperator(format!(
            "Unsupported reference type: {reftype:?}, {:?}",
            reftype.heap_type()
        ))),
    }
}

pub(crate) fn convert_valtype(valtype: &wasmparser::ValType) -> Result<WasmType> {
    match valtype {
        wasmparser::ValType::I32 => Ok(WasmType::I32),
        wasmparser::ValType::I64 => Ok(WasmType::I64),
        wasmparser::ValType::F32 => Ok(WasmType::F32),
        wasmparser::ValType::F64 => Ok(WasmType::F64),
        wasmparser::ValType::V128 => Ok(WasmType::V128),
        wasmparser::ValType::Ref(r) => convert_reftype(*r),
    }
}

pub(crate) fn process_const_operators(ops: OperatorsReader<'_>) -> Result<Box<[ConstInstruction]>> {
    let ops = ops.into_iter().collect::<wasmparser::Result<Vec<_>>>()?;
    // In practice, the len can never be something other than 2,
    // but we'll keep this here since it's part of the spec
    // Invalid modules will be rejected by the validator anyway (there are also tests for this in the testsuite)
    debug_assert!(ops.len() >= 2);
    debug_assert!(matches!(ops[ops.len() - 1], wasmparser::Operator::End));

    let mut out = Vec::with_capacity(ops.len().saturating_sub(1));
    for op in ops.iter().take(ops.len() - 1) {
        let instr = match op {
            wasmparser::Operator::RefNull { hty } => match convert_heaptype(*hty)? {
                WasmType::RefFunc => ConstInstruction::RefFunc(None),
                WasmType::RefExtern => ConstInstruction::RefExtern(None),
                other => {
                    return Err(crate::ParseError::UnsupportedOperator(format!(
                        "Unsupported ref.null heap type lowered to {other:?}"
                    )));
                }
            },
            wasmparser::Operator::RefFunc { function_index } => ConstInstruction::RefFunc(Some(*function_index)),
            wasmparser::Operator::I32Const { value } => ConstInstruction::I32Const(*value),
            wasmparser::Operator::I64Const { value } => ConstInstruction::I64Const(*value),
            wasmparser::Operator::F32Const { value } => ConstInstruction::F32Const(f32::from_bits(value.bits())),
            wasmparser::Operator::F64Const { value } => ConstInstruction::F64Const(f64::from_bits(value.bits())),
            wasmparser::Operator::V128Const { value } => ConstInstruction::V128Const(*value.bytes()),
            wasmparser::Operator::GlobalGet { global_index } => ConstInstruction::GlobalGet(*global_index),
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

    Ok(out.into_boxed_slice())
}

pub(crate) fn convert_heaptype(heap: wasmparser::HeapType) -> Result<WasmType> {
    match heap {
        wasmparser::HeapType::Abstract { shared: false, ty: wasmparser::AbstractHeapType::Func } => {
            Ok(WasmType::RefFunc)
        }
        wasmparser::HeapType::Abstract { shared: false, ty: wasmparser::AbstractHeapType::Extern } => {
            Ok(WasmType::RefExtern)
        }
        _ => Err(crate::ParseError::UnsupportedOperator(format!("Unsupported heap type: {heap:?}"))),
    }
}
