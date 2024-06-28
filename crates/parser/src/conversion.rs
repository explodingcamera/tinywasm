use crate::Result;
use crate::{module::Code, visit::process_operators_and_validate};
use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use tinywasm_types::*;
use wasmparser::{FuncValidator, FuncValidatorAllocations, OperatorsReader, ValidatorResources};

pub(crate) fn convert_module_elements<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Element<'a>>>>(
    elements: T,
) -> Result<Vec<tinywasm_types::Element>> {
    elements.into_iter().map(|element| convert_module_element(element?)).collect::<Result<Vec<_>>>()
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

            Ok(tinywasm_types::Element { kind, items, ty: ValType::RefFunc, range: element.range })
        }

        wasmparser::ElementItems::Expressions(ty, exprs) => {
            let items = exprs
                .into_iter()
                .map(|expr| Ok(ElementItem::Expr(process_const_operators(expr?.get_operators_reader())?)))
                .collect::<Result<Vec<_>>>()?
                .into_boxed_slice();

            Ok(tinywasm_types::Element { kind, items, ty: convert_reftype(&ty), range: element.range })
        }
    }
}

pub(crate) fn convert_module_data_sections<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Data<'a>>>>(
    data_sections: T,
) -> Result<Vec<tinywasm_types::Data>> {
    data_sections.into_iter().map(|data| convert_module_data(data?)).collect::<Result<Vec<_>>>()
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

pub(crate) fn convert_module_imports<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Import<'a>>>>(
    imports: T,
) -> Result<Vec<Import>> {
    imports.into_iter().map(|import| convert_module_import(import?)).collect::<Result<Vec<_>>>()
}

pub(crate) fn convert_module_import(import: wasmparser::Import<'_>) -> Result<Import> {
    Ok(Import {
        module: import.module.to_string().into_boxed_str(),
        name: import.name.to_string().into_boxed_str(),
        kind: match import.ty {
            wasmparser::TypeRef::Func(ty) => ImportKind::Function(ty),
            wasmparser::TypeRef::Table(ty) => ImportKind::Table(TableType {
                element_type: convert_reftype(&ty.element_type),
                size_initial: ty.initial.try_into().map_err(|_| {
                    crate::ParseError::UnsupportedOperator(format!("Table size initial is too large: {}", ty.initial))
                })?,
                size_max: match ty.maximum {
                    Some(max) => Some(max.try_into().map_err(|_| {
                        crate::ParseError::UnsupportedOperator(format!("Table size max is too large: {}", max))
                    })?),
                    None => None,
                },
            }),
            wasmparser::TypeRef::Memory(ty) => ImportKind::Memory(convert_module_memory(ty)?),
            wasmparser::TypeRef::Global(ty) => {
                ImportKind::Global(GlobalType { mutable: ty.mutable, ty: convert_valtype(&ty.content_type) })
            }
            wasmparser::TypeRef::Tag(ty) => {
                return Err(crate::ParseError::UnsupportedOperator(format!("Unsupported import kind: {:?}", ty)))
            }
        },
    })
}

pub(crate) fn convert_module_memories<T: IntoIterator<Item = wasmparser::Result<wasmparser::MemoryType>>>(
    memory_types: T,
) -> Result<Vec<MemoryType>> {
    memory_types.into_iter().map(|memory| convert_module_memory(memory?)).collect::<Result<Vec<_>>>()
}

pub(crate) fn convert_module_memory(memory: wasmparser::MemoryType) -> Result<MemoryType> {
    Ok(MemoryType {
        arch: match memory.memory64 {
            true => MemoryArch::I64,
            false => MemoryArch::I32,
        },
        page_count_initial: memory.initial,
        page_count_max: memory.maximum,
    })
}

pub(crate) fn convert_module_tables<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Table<'a>>>>(
    table_types: T,
) -> Result<Vec<TableType>> {
    table_types.into_iter().map(|table| convert_module_table(table?)).collect::<Result<Vec<_>>>()
}

pub(crate) fn convert_module_table(table: wasmparser::Table<'_>) -> Result<TableType> {
    let size_initial = table.ty.initial.try_into().map_err(|_| {
        crate::ParseError::UnsupportedOperator(format!("Table size initial is too large: {}", table.ty.initial))
    })?;

    let size_max = match table.ty.maximum {
        Some(max) => Some(
            max.try_into()
                .map_err(|_| crate::ParseError::UnsupportedOperator(format!("Table size max is too large: {}", max)))?,
        ),
        None => None,
    };

    Ok(TableType { element_type: convert_reftype(&table.ty.element_type), size_initial, size_max })
}

pub(crate) fn convert_module_globals(
    globals: wasmparser::SectionLimited<'_, wasmparser::Global<'_>>,
) -> Result<Vec<Global>> {
    let globals = globals
        .into_iter()
        .map(|global| {
            let global = global?;
            let ty = convert_valtype(&global.ty.content_type);
            let ops = global.init_expr.get_operators_reader();
            Ok(Global { init: process_const_operators(ops)?, ty: GlobalType { mutable: global.ty.mutable, ty } })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(globals)
}

pub(crate) fn convert_module_export(export: wasmparser::Export<'_>) -> Result<Export> {
    let kind = match export.kind {
        wasmparser::ExternalKind::Func => ExternalKind::Func,
        wasmparser::ExternalKind::Table => ExternalKind::Table,
        wasmparser::ExternalKind::Memory => ExternalKind::Memory,
        wasmparser::ExternalKind::Global => ExternalKind::Global,
        wasmparser::ExternalKind::Tag => {
            return Err(crate::ParseError::UnsupportedOperator(format!("Unsupported export kind: {:?}", export.kind)))
        }
    };

    Ok(Export { index: export.index, name: Box::from(export.name), kind })
}

pub(crate) fn convert_module_code(
    func: wasmparser::FunctionBody<'_>,
    mut validator: FuncValidator<ValidatorResources>,
) -> Result<(Code, FuncValidatorAllocations)> {
    let locals_reader = func.get_locals_reader()?;
    let count = locals_reader.get_count();
    let pos = locals_reader.original_position();

    // maps a local's address to the index in the type's locals array
    let mut local_addr_map = Vec::with_capacity(count as usize);
    let mut local_counts = LocalCounts::default();

    for (i, local) in locals_reader.into_iter().enumerate() {
        let local = local?;
        validator.define_locals(pos + i, local.0, local.1)?;
    }

    for i in 0..validator.len_locals() {
        match validator.get_local_type(i) {
            Some(wasmparser::ValType::I32) | Some(wasmparser::ValType::F32) => {
                local_addr_map.push(local_counts.local_32);
                local_counts.local_32 += 1;
            }
            Some(wasmparser::ValType::I64) | Some(wasmparser::ValType::F64) => {
                local_addr_map.push(local_counts.local_64);
                local_counts.local_64 += 1;
            }
            Some(wasmparser::ValType::V128) => {
                local_addr_map.push(local_counts.local_128);
                local_counts.local_128 += 1;
            }
            Some(wasmparser::ValType::Ref(_)) => {
                local_addr_map.push(local_counts.local_ref);
                local_counts.local_ref += 1;
            }
            None => return Err(crate::ParseError::UnsupportedOperator("Unknown local type".to_string())),
        }
    }

    let (body, allocations) = process_operators_and_validate(validator, func, local_addr_map)?;
    Ok(((body, local_counts), allocations))
}

pub(crate) fn convert_module_type(ty: wasmparser::RecGroup) -> Result<FuncType> {
    let mut types = ty.types();

    if types.len() != 1 {
        return Err(crate::ParseError::UnsupportedOperator(
            "Expected exactly one type in the type section".to_string(),
        ));
    }

    let ty = types.next().unwrap().unwrap_func();
    let params = ty.params().iter().map(convert_valtype).collect::<Vec<ValType>>().into_boxed_slice();
    let results = ty.results().iter().map(convert_valtype).collect::<Vec<ValType>>().into_boxed_slice();

    Ok(FuncType { params, results })
}

pub(crate) fn convert_reftype(reftype: &wasmparser::RefType) -> ValType {
    match reftype {
        _ if reftype.is_func_ref() => ValType::RefFunc,
        _ if reftype.is_extern_ref() => ValType::RefExtern,
        _ => unimplemented!("Unsupported reference type: {:?}, {:?}", reftype, reftype.heap_type()),
    }
}

pub(crate) fn convert_valtype(valtype: &wasmparser::ValType) -> ValType {
    match valtype {
        wasmparser::ValType::I32 => ValType::I32,
        wasmparser::ValType::I64 => ValType::I64,
        wasmparser::ValType::F32 => ValType::F32,
        wasmparser::ValType::F64 => ValType::F64,
        wasmparser::ValType::V128 => ValType::V128,
        wasmparser::ValType::Ref(r) => convert_reftype(r),
    }
}

pub(crate) fn process_const_operators(ops: OperatorsReader<'_>) -> Result<ConstInstruction> {
    let ops = ops.into_iter().collect::<wasmparser::Result<Vec<_>>>()?;
    // In practice, the len can never be something other than 2,
    // but we'll keep this here since it's part of the spec
    // Invalid modules will be rejected by the validator anyway (there are also tests for this in the testsuite)
    assert!(ops.len() >= 2);
    assert!(matches!(ops[ops.len() - 1], wasmparser::Operator::End));

    match &ops[ops.len() - 2] {
        wasmparser::Operator::RefNull { hty } => Ok(ConstInstruction::RefNull(convert_heaptype(*hty))),
        wasmparser::Operator::RefFunc { function_index } => Ok(ConstInstruction::RefFunc(*function_index)),
        wasmparser::Operator::I32Const { value } => Ok(ConstInstruction::I32Const(*value)),
        wasmparser::Operator::I64Const { value } => Ok(ConstInstruction::I64Const(*value)),
        wasmparser::Operator::F32Const { value } => Ok(ConstInstruction::F32Const(f32::from_bits(value.bits()))),
        wasmparser::Operator::F64Const { value } => Ok(ConstInstruction::F64Const(f64::from_bits(value.bits()))),
        wasmparser::Operator::GlobalGet { global_index } => Ok(ConstInstruction::GlobalGet(*global_index)),
        op => Err(crate::ParseError::UnsupportedOperator(format!("Unsupported const instruction: {:?}", op))),
    }
}

pub(crate) fn convert_heaptype(heap: wasmparser::HeapType) -> ValType {
    match heap {
        wasmparser::HeapType::Abstract { shared: false, ty: wasmparser::AbstractHeapType::Func } => ValType::RefFunc,
        wasmparser::HeapType::Abstract { shared: false, ty: wasmparser::AbstractHeapType::Extern } => {
            ValType::RefExtern
        }
        _ => unimplemented!("Unsupported heap type: {:?}", heap),
    }
}
