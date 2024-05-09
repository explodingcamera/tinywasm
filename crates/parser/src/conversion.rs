use crate::Result;
use crate::{module::Code, visit::process_operators};
use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use tinywasm_types::*;
use wasmparser::{FuncValidator, OperatorsReader, ValidatorResources};

pub(crate) fn convert_module_elements<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Element<'a>>>>(
    elements: T,
) -> Result<Vec<tinywasm_types::Element>> {
    let elements = elements.into_iter().map(|element| convert_module_element(element?)).collect::<Result<Vec<_>>>()?;
    Ok(elements)
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
    let data_sections = data_sections.into_iter().map(|data| convert_module_data(data?)).collect::<Result<Vec<_>>>()?;
    Ok(data_sections)
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
    let imports = imports.into_iter().map(|import| convert_module_import(import?)).collect::<Result<Vec<_>>>()?;
    Ok(imports)
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
                size_max: if let Some(max) = ty.maximum {
                    Some(max.try_into().map_err(|_| {
                        crate::ParseError::UnsupportedOperator(format!("Table size max is too large: {}", max))
                    })?)
                } else {
                    None
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
    let memory_type =
        memory_types.into_iter().map(|memory| convert_module_memory(memory?)).collect::<Result<Vec<_>>>()?;

    Ok(memory_type)
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
    let table_type = table_types.into_iter().map(|table| convert_module_table(table?)).collect::<Result<Vec<_>>>()?;
    Ok(table_type)
}

pub(crate) fn convert_module_table(table: wasmparser::Table<'_>) -> Result<TableType> {
    let ty = convert_reftype(&table.ty.element_type);

    let size_initial = table.ty.initial.try_into().map_err(|_| {
        crate::ParseError::UnsupportedOperator(format!("Table size initial is too large: {}", table.ty.initial))
    })?;
    let size_max = if let Some(max) = table.ty.maximum {
        Some(
            max.try_into()
                .map_err(|_| crate::ParseError::UnsupportedOperator(format!("Table size max is too large: {}", max)))?,
        )
    } else {
        None
    };

    Ok(TableType { element_type: ty, size_initial: size_initial, size_max })
}

pub(crate) fn convert_module_globals<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Global<'a>>>>(
    globals: T,
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
) -> Result<Code> {
    let locals_reader = func.get_locals_reader()?;
    let count = locals_reader.get_count();
    let pos = locals_reader.original_position();

    let mut locals = Vec::with_capacity(count as usize);
    for (i, local) in locals_reader.into_iter().enumerate() {
        let local = local?;
        validator.define_locals(pos + i, local.0, local.1)?;
        for _ in 0..local.0 {
            locals.push(convert_valtype(&local.1));
        }
    }

    let body = process_operators(Some(&mut validator), &func)?;
    let locals = locals.into_boxed_slice();
    Ok((body, locals))
}

pub(crate) fn convert_module_type(ty: wasmparser::RecGroup) -> Result<FuncType> {
    let mut types = ty.types();

    if types.len() != 1 {
        return Err(crate::ParseError::UnsupportedOperator(
            "Expected exactly one type in the type section".to_string(),
        ));
    }
    let ty = types.next().unwrap().unwrap_func();

    let params =
        ty.params().iter().map(|p| Ok(convert_valtype(p))).collect::<Result<Vec<ValType>>>()?.into_boxed_slice();

    let results =
        ty.results().iter().map(|p| Ok(convert_valtype(p))).collect::<Result<Vec<ValType>>>()?.into_boxed_slice();

    Ok(FuncType { params, results })
}

pub(crate) fn convert_blocktype(blocktype: wasmparser::BlockType) -> BlockArgs {
    match blocktype {
        wasmparser::BlockType::Empty => BlockArgs::Empty,
        wasmparser::BlockType::Type(ty) => BlockArgs::Type(convert_valtype(&ty)),
        wasmparser::BlockType::FuncType(ty) => BlockArgs::FuncType(ty),
    }
}

pub(crate) fn convert_reftype(reftype: &wasmparser::RefType) -> ValType {
    match reftype {
        _ if reftype.is_func_ref() => ValType::RefFunc,
        _ if reftype.is_extern_ref() => ValType::RefExtern,
        _ => unimplemented!("Unsupported reference type: {:?}", reftype),
    }
}

pub(crate) fn convert_valtype(valtype: &wasmparser::ValType) -> ValType {
    use wasmparser::ValType::*;
    match valtype {
        I32 => ValType::I32,
        I64 => ValType::I64,
        F32 => ValType::F32,
        F64 => ValType::F64,
        Ref(r) => convert_reftype(r),
        V128 => unimplemented!("128-bit values are not supported yet"),
    }
}

pub(crate) fn convert_memarg(memarg: wasmparser::MemArg) -> MemoryArg {
    MemoryArg { offset: memarg.offset, mem_addr: memarg.memory }
}

pub(crate) fn process_const_operators(ops: OperatorsReader<'_>) -> Result<ConstInstruction> {
    let ops = ops.into_iter().collect::<wasmparser::Result<Vec<_>>>()?;
    // In practice, the len can never be something other than 2,
    // but we'll keep this here since it's part of the spec
    // Invalid modules will be rejected by the validator anyway (there are also tests for this in the testsuite)
    assert!(ops.len() >= 2);
    assert!(matches!(ops[ops.len() - 1], wasmparser::Operator::End));
    process_const_operator(ops[ops.len() - 2].clone())
}

pub(crate) fn process_const_operator(op: wasmparser::Operator<'_>) -> Result<ConstInstruction> {
    match op {
        wasmparser::Operator::RefNull { hty } => Ok(ConstInstruction::RefNull(convert_heaptype(hty))),
        wasmparser::Operator::RefFunc { function_index } => Ok(ConstInstruction::RefFunc(function_index)),
        wasmparser::Operator::I32Const { value } => Ok(ConstInstruction::I32Const(value)),
        wasmparser::Operator::I64Const { value } => Ok(ConstInstruction::I64Const(value)),
        wasmparser::Operator::F32Const { value } => Ok(ConstInstruction::F32Const(f32::from_bits(value.bits()))),
        wasmparser::Operator::F64Const { value } => Ok(ConstInstruction::F64Const(f64::from_bits(value.bits()))),
        wasmparser::Operator::GlobalGet { global_index } => Ok(ConstInstruction::GlobalGet(global_index)),
        op => Err(crate::ParseError::UnsupportedOperator(format!("Unsupported const instruction: {:?}", op))),
    }
}

pub(crate) fn convert_heaptype(heap: wasmparser::HeapType) -> ValType {
    match heap {
        wasmparser::HeapType::Func => ValType::RefFunc,
        wasmparser::HeapType::Extern => ValType::RefExtern,
        _ => unimplemented!("Unsupported heap type: {:?}", heap),
    }
}
