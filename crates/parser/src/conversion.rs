use alloc::{boxed::Box, format, string::ToString, vec::Vec};
use log::info;
use tinywasm_types::{
    BlockArgs, ConstInstruction, ElementItem, Export, ExternalKind, FuncType, Global, GlobalType, Import, ImportKind,
    Instruction, MemArg, MemoryArch, MemoryType, TableType, ValType,
};
use wasmparser::{FuncValidator, OperatorsReader, ValidatorResources};

use crate::{module::CodeSection, Result};

pub(crate) fn convert_module_elements<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Element<'a>>>>(
    elements: T,
) -> Result<Vec<tinywasm_types::Element>> {
    let elements = elements
        .into_iter()
        .map(|element| convert_module_element(element?))
        .collect::<Result<Vec<_>>>()?;
    Ok(elements)
}

pub(crate) fn convert_module_element(element: wasmparser::Element<'_>) -> Result<tinywasm_types::Element> {
    let kind = match element.kind {
        wasmparser::ElementKind::Active {
            table_index,
            offset_expr,
        } => tinywasm_types::ElementKind::Active {
            table: table_index,
            offset: process_const_operators(offset_expr.get_operators_reader())?,
        },
        wasmparser::ElementKind::Passive => tinywasm_types::ElementKind::Passive,
        wasmparser::ElementKind::Declared => tinywasm_types::ElementKind::Declared,
    };

    let items = match element.items {
        wasmparser::ElementItems::Functions(funcs) => funcs
            .into_iter()
            .map(|func| Ok(ElementItem::Func(func?)))
            .collect::<Result<Vec<_>>>()?
            .into_boxed_slice(),

        wasmparser::ElementItems::Expressions(exprs) => exprs
            .into_iter()
            .map(|expr| {
                Ok(ElementItem::Expr(process_const_operators(
                    expr?.get_operators_reader(),
                )?))
            })
            .collect::<Result<Vec<_>>>()?
            .into_boxed_slice(),
    };

    Ok(tinywasm_types::Element {
        kind,
        items,
        ty: convert_valtype(&element.ty),
        range: element.range,
    })
}

pub(crate) fn convert_module_data_sections<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Data<'a>>>>(
    data_sections: T,
) -> Result<Vec<tinywasm_types::Data>> {
    let data_sections = data_sections
        .into_iter()
        .map(|data| convert_module_data(data?))
        .collect::<Result<Vec<_>>>()?;
    Ok(data_sections)
}

pub(crate) fn convert_module_data(data: wasmparser::Data<'_>) -> Result<tinywasm_types::Data> {
    Ok(tinywasm_types::Data {
        data: data.data.to_vec().into_boxed_slice(),
        range: data.range,
        kind: match data.kind {
            wasmparser::DataKind::Active {
                memory_index,
                offset_expr,
            } => {
                let offset = process_const_operators(offset_expr.get_operators_reader())?;
                tinywasm_types::DataKind::Active {
                    mem: memory_index,
                    offset,
                }
            }
            wasmparser::DataKind::Passive => tinywasm_types::DataKind::Passive,
        },
    })
}

pub(crate) fn convert_module_imports<'a, T: IntoIterator<Item = wasmparser::Result<wasmparser::Import<'a>>>>(
    imports: T,
) -> Result<Vec<Import>> {
    let imports = imports
        .into_iter()
        .map(|import| convert_module_import(import?))
        .collect::<Result<Vec<_>>>()?;
    Ok(imports)
}

pub(crate) fn convert_module_import(import: wasmparser::Import<'_>) -> Result<Import> {
    Ok(Import {
        module: import.module.to_string().into_boxed_str(),
        name: import.name.to_string().into_boxed_str(),
        kind: match import.ty {
            wasmparser::TypeRef::Func(ty) => ImportKind::Func(ty),
            wasmparser::TypeRef::Table(ty) => ImportKind::Table(convert_module_table(ty)?),
            wasmparser::TypeRef::Memory(ty) => ImportKind::Mem(convert_module_memory(ty)?),
            wasmparser::TypeRef::Global(ty) => ImportKind::Global(GlobalType {
                mutable: ty.mutable,
                ty: convert_valtype(&ty.content_type),
            }),
            wasmparser::TypeRef::Tag(ty) => {
                return Err(crate::ParseError::UnsupportedOperator(format!(
                    "Unsupported import kind: {:?}",
                    ty
                )))
            }
        },
    })
}

pub(crate) fn convert_module_memories<T: IntoIterator<Item = wasmparser::Result<wasmparser::MemoryType>>>(
    memory_types: T,
) -> Result<Vec<MemoryType>> {
    let memory_type = memory_types
        .into_iter()
        .map(|memory| convert_module_memory(memory?))
        .collect::<Result<Vec<_>>>()?;

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

pub(crate) fn convert_module_tables<T: IntoIterator<Item = wasmparser::Result<wasmparser::TableType>>>(
    table_types: T,
) -> Result<Vec<TableType>> {
    let table_type = table_types
        .into_iter()
        .map(|table| convert_module_table(table?))
        .collect::<Result<Vec<_>>>()?;

    Ok(table_type)
}

pub(crate) fn convert_module_table(table: wasmparser::TableType) -> Result<TableType> {
    let ty = convert_valtype(&table.element_type);
    Ok(TableType {
        element_type: ty,
        size_initial: table.initial,
        size_max: table.maximum,
    })
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

            Ok(Global {
                init: process_const_operators(ops)?,
                ty: GlobalType {
                    mutable: global.ty.mutable,
                    ty,
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(globals)
}

pub(crate) fn convert_module_export(export: wasmparser::Export) -> Result<Export> {
    let kind = match export.kind {
        wasmparser::ExternalKind::Func => ExternalKind::Func,
        wasmparser::ExternalKind::Table => ExternalKind::Table,
        wasmparser::ExternalKind::Memory => ExternalKind::Memory,
        wasmparser::ExternalKind::Global => ExternalKind::Global,
        wasmparser::ExternalKind::Tag => {
            return Err(crate::ParseError::UnsupportedOperator(format!(
                "Unsupported export kind: {:?}",
                export.kind
            )))
        }
    };

    Ok(Export {
        index: export.index,
        name: Box::from(export.name),
        kind,
    })
}

pub(crate) fn convert_module_code(
    func: wasmparser::FunctionBody,
    mut validator: FuncValidator<ValidatorResources>,
) -> Result<CodeSection> {
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

    let body_reader = func.get_operators_reader()?;
    let body = process_operators(body_reader.original_position(), body_reader.into_iter(), validator)?;

    Ok(CodeSection {
        locals: locals.into_boxed_slice(),
        body,
    })
}

pub(crate) fn convert_module_type(ty: wasmparser::Type) -> Result<FuncType> {
    let wasmparser::Type::Func(ty) = ty;
    let params = ty
        .params()
        .iter()
        .map(|p| Ok(convert_valtype(p)))
        .collect::<Result<Vec<ValType>>>()?
        .into_boxed_slice();

    let results = ty
        .results()
        .iter()
        .map(|p| Ok(convert_valtype(p)))
        .collect::<Result<Vec<ValType>>>()?
        .into_boxed_slice();

    Ok(FuncType { params, results })
}

pub(crate) fn convert_blocktype(blocktype: wasmparser::BlockType) -> BlockArgs {
    use wasmparser::BlockType::*;
    match blocktype {
        Empty => BlockArgs::Empty,

        // We should maybe have all this in a single variant for our custom bytecode

        // TODO: maybe solve this differently so we can support 128-bit values
        // without having to increase the size of the WasmValue enum
        Type(ty) => BlockArgs::Type(convert_valtype(&ty)),
        FuncType(ty) => BlockArgs::FuncType(ty),
    }
}

pub(crate) fn convert_valtype(valtype: &wasmparser::ValType) -> ValType {
    use wasmparser::ValType::*;
    match valtype {
        I32 => ValType::I32,
        I64 => ValType::I64,
        F32 => ValType::F32,
        F64 => ValType::F64,
        V128 => ValType::V128,
        FuncRef => ValType::FuncRef,
        ExternRef => ValType::ExternRef,
    }
}

pub(crate) fn convert_memarg(memarg: wasmparser::MemArg) -> MemArg {
    MemArg {
        offset: memarg.offset,
        align: memarg.align,
    }
}

pub(crate) fn process_const_operators(ops: OperatorsReader) -> Result<ConstInstruction> {
    let ops = ops.into_iter().collect::<wasmparser::Result<Vec<_>>>()?;
    // In practice, the len can never be something other than 2,
    // but we'll keep this here since it's part of the spec
    // Invalid modules will be rejected by the validator anyway (there are also tests for this in the testsuite)
    assert!(ops.len() >= 2);
    assert!(matches!(ops[ops.len() - 1], wasmparser::Operator::End));

    process_const_operator(ops[ops.len() - 2].clone())
}

pub fn process_const_operator(op: wasmparser::Operator) -> Result<ConstInstruction> {
    match op {
        wasmparser::Operator::I32Const { value } => Ok(ConstInstruction::I32Const(value)),
        wasmparser::Operator::I64Const { value } => Ok(ConstInstruction::I64Const(value)),
        wasmparser::Operator::F32Const { value } => Ok(ConstInstruction::F32Const(f32::from_bits(value.bits()))), // TODO: check if this is correct
        wasmparser::Operator::F64Const { value } => Ok(ConstInstruction::F64Const(f64::from_bits(value.bits()))), // TODO: check if this is correct
        wasmparser::Operator::GlobalGet { global_index } => Ok(ConstInstruction::GlobalGet(global_index)),
        op => Err(crate::ParseError::UnsupportedOperator(format!(
            "Unsupported instruction: {:?}",
            op
        ))),
    }
}

pub fn process_operators<'a>(
    mut offset: usize,
    ops: impl Iterator<Item = Result<wasmparser::Operator<'a>, wasmparser::BinaryReaderError>>,
    mut validator: FuncValidator<ValidatorResources>,
) -> Result<Box<[Instruction]>> {
    let mut instructions = Vec::new();
    let mut labels_ptrs = Vec::new(); // indexes into the instructions array

    for op in ops {
        info!("op: {:?}", op);

        let op = op?;
        validator.op(offset, &op)?;
        offset += 1;

        use wasmparser::Operator::*;
        let res = match op {
            BrTable { targets } => {
                let def = targets.default();
                let targets = targets
                    .targets()
                    .collect::<Result<Vec<u32>, wasmparser::BinaryReaderError>>()?;
                instructions.push(Instruction::BrTable(def, targets.len()));
                instructions.extend(targets.into_iter().map(Instruction::BrLabel));
                continue;
            }
            Unreachable => Instruction::Unreachable,
            Nop => Instruction::Nop,
            Block { blockty } => {
                labels_ptrs.push(instructions.len());
                Instruction::Block(convert_blocktype(blockty), 0)
            }
            Loop { blockty } => {
                labels_ptrs.push(instructions.len());
                Instruction::Loop(convert_blocktype(blockty), 0)
            }
            If { blockty } => {
                labels_ptrs.push(instructions.len());
                Instruction::If(convert_blocktype(blockty), None, 0)
            }
            Else => {
                labels_ptrs.push(instructions.len());
                Instruction::Else(0)
            }
            End => {
                if let Some(label_pointer) = labels_ptrs.pop() {
                    info!("ending block: {:?}", instructions[label_pointer]);

                    let current_instr_ptr = instructions.len();

                    // last_label_pointer is Some if we're ending a block
                    match instructions[label_pointer] {
                        Instruction::Else(ref mut else_instr_end_offset) => {
                            *else_instr_end_offset = current_instr_ptr - label_pointer;

                            // since we're ending an else block, we need to end the if block as well
                            let if_label_pointer = labels_ptrs.pop().ok_or(crate::ParseError::UnsupportedOperator(
                                "Expected to end an if block, but the last label was not an if".to_string(),
                            ))?;

                            let if_instruction = &mut instructions[if_label_pointer];
                            let Instruction::If(_, ref mut else_offset, ref mut end_offset) = if_instruction else {
                                return Err(crate::ParseError::UnsupportedOperator(
                                    "Expected to end an if block, but the last label was not an if".to_string(),
                                ));
                            };

                            *else_offset = Some(label_pointer - if_label_pointer);
                            *end_offset = current_instr_ptr - if_label_pointer;
                        }
                        Instruction::Block(_, ref mut end_offset)
                        | Instruction::Loop(_, ref mut end_offset)
                        | Instruction::If(_, _, ref mut end_offset) => {
                            *end_offset = current_instr_ptr - label_pointer;
                        }
                        _ => {
                            return Err(crate::ParseError::UnsupportedOperator(
                                "Expected to end a block, but the last label was not a block".to_string(),
                            ))
                        }
                    }

                    Instruction::EndBlockFrame
                } else {
                    // last_label_pointer is None if we're ending the function
                    Instruction::EndFunc
                }
            }

            Br { relative_depth } => Instruction::Br(relative_depth),
            BrIf { relative_depth } => Instruction::BrIf(relative_depth),
            Return => Instruction::Return,
            Call { function_index } => Instruction::Call(function_index),
            CallIndirect {
                type_index,
                table_index,
                ..
            } => Instruction::CallIndirect(type_index, table_index),
            Drop => Instruction::Drop,
            Select => Instruction::Select(None),
            TypedSelect { ty } => Instruction::Select(Some(convert_valtype(&ty))),
            LocalGet { local_index } => Instruction::LocalGet(local_index),
            LocalSet { local_index } => Instruction::LocalSet(local_index),
            LocalTee { local_index } => Instruction::LocalTee(local_index),
            GlobalGet { global_index } => Instruction::GlobalGet(global_index),
            GlobalSet { global_index } => Instruction::GlobalSet(global_index),
            MemorySize { .. } => Instruction::MemorySize,
            MemoryGrow { .. } => Instruction::MemoryGrow,
            I32Const { value } => Instruction::I32Const(value),
            I64Const { value } => Instruction::I64Const(value),
            F32Const { value } => Instruction::F32Const(f32::from_bits(value.bits())), // TODO: check if this is correct
            F64Const { value } => Instruction::F64Const(f64::from_bits(value.bits())), // TODO: check if this is correct
            I32Load { memarg } => Instruction::I32Load(convert_memarg(memarg)),
            I64Load { memarg } => Instruction::I64Load(convert_memarg(memarg)),
            F32Load { memarg } => Instruction::F32Load(convert_memarg(memarg)),
            F64Load { memarg } => Instruction::F64Load(convert_memarg(memarg)),
            I32Load8S { memarg } => Instruction::I32Load8S(convert_memarg(memarg)),
            I32Load8U { memarg } => Instruction::I32Load8U(convert_memarg(memarg)),
            I32Load16S { memarg } => Instruction::I32Load16S(convert_memarg(memarg)),
            I32Load16U { memarg } => Instruction::I32Load16U(convert_memarg(memarg)),
            I64Load8S { memarg } => Instruction::I64Load8S(convert_memarg(memarg)),
            I64Load8U { memarg } => Instruction::I64Load8U(convert_memarg(memarg)),
            I64Load16S { memarg } => Instruction::I64Load16S(convert_memarg(memarg)),
            I64Load16U { memarg } => Instruction::I64Load16U(convert_memarg(memarg)),
            I64Load32S { memarg } => Instruction::I64Load32S(convert_memarg(memarg)),
            I64Load32U { memarg } => Instruction::I64Load32U(convert_memarg(memarg)),
            I32Store { memarg } => Instruction::I32Store(convert_memarg(memarg)),
            I64Store { memarg } => Instruction::I64Store(convert_memarg(memarg)),
            F32Store { memarg } => Instruction::F32Store(convert_memarg(memarg)),
            F64Store { memarg } => Instruction::F64Store(convert_memarg(memarg)),
            I32Store8 { memarg } => Instruction::I32Store8(convert_memarg(memarg)),
            I32Store16 { memarg } => Instruction::I32Store16(convert_memarg(memarg)),
            I64Store8 { memarg } => Instruction::I64Store8(convert_memarg(memarg)),
            I64Store16 { memarg } => Instruction::I64Store16(convert_memarg(memarg)),
            I64Store32 { memarg } => Instruction::I64Store32(convert_memarg(memarg)),
            I32Eqz => Instruction::I32Eqz,
            I32Eq => Instruction::I32Eq,
            I32Ne => Instruction::I32Ne,
            I32LtS => Instruction::I32LtS,
            I32LtU => Instruction::I32LtU,
            I32GtS => Instruction::I32GtS,
            I32GtU => Instruction::I32GtU,
            I32LeS => Instruction::I32LeS,
            I32LeU => Instruction::I32LeU,
            I32GeS => Instruction::I32GeS,
            I32GeU => Instruction::I32GeU,
            I64Eqz => Instruction::I64Eqz,
            I64Eq => Instruction::I64Eq,
            I64Ne => Instruction::I64Ne,
            I64LtS => Instruction::I64LtS,
            I64LtU => Instruction::I64LtU,
            I64GtS => Instruction::I64GtS,
            I64GtU => Instruction::I64GtU,
            I64LeS => Instruction::I64LeS,
            I64LeU => Instruction::I64LeU,
            I64GeS => Instruction::I64GeS,
            I64GeU => Instruction::I64GeU,
            F32Eq => Instruction::F32Eq,
            F32Ne => Instruction::F32Ne,
            F32Lt => Instruction::F32Lt,
            F32Gt => Instruction::F32Gt,
            F32Le => Instruction::F32Le,
            F32Ge => Instruction::F32Ge,
            F64Eq => Instruction::F64Eq,
            F64Ne => Instruction::F64Ne,
            F64Lt => Instruction::F64Lt,
            F64Gt => Instruction::F64Gt,
            F64Le => Instruction::F64Le,
            F64Ge => Instruction::F64Ge,
            I32Clz => Instruction::I32Clz,
            I32Ctz => Instruction::I32Ctz,
            I32Popcnt => Instruction::I32Popcnt,
            I32Add => Instruction::I32Add,
            I32Sub => Instruction::I32Sub,
            I32Mul => Instruction::I32Mul,
            I32DivS => Instruction::I32DivS,
            I32DivU => Instruction::I32DivU,
            I32RemS => Instruction::I32RemS,
            I32RemU => Instruction::I32RemU,
            I32And => Instruction::I32And,
            I32Or => Instruction::I32Or,
            I32Xor => Instruction::I32Xor,
            I32Shl => Instruction::I32Shl,
            I32ShrS => Instruction::I32ShrS,
            I32ShrU => Instruction::I32ShrU,
            I32Rotl => Instruction::I32Rotl,
            I32Rotr => Instruction::I32Rotr,
            I64Clz => Instruction::I64Clz,
            I64Ctz => Instruction::I64Ctz,
            I64Popcnt => Instruction::I64Popcnt,
            I64Add => Instruction::I64Add,
            I64Sub => Instruction::I64Sub,
            I64Mul => Instruction::I64Mul,
            I64DivS => Instruction::I64DivS,
            I64DivU => Instruction::I64DivU,
            I64RemS => Instruction::I64RemS,
            I64RemU => Instruction::I64RemU,
            I64And => Instruction::I64And,
            I64Or => Instruction::I64Or,
            I64Xor => Instruction::I64Xor,
            I64Shl => Instruction::I64Shl,
            I64ShrS => Instruction::I64ShrS,
            I64ShrU => Instruction::I64ShrU,
            I64Rotl => Instruction::I64Rotl,
            I64Rotr => Instruction::I64Rotr,
            F32Abs => Instruction::F32Abs,
            F32Neg => Instruction::F32Neg,
            F32Ceil => Instruction::F32Ceil,
            F32Floor => Instruction::F32Floor,
            F32Trunc => Instruction::F32Trunc,
            F32Nearest => Instruction::F32Nearest,
            F32Sqrt => Instruction::F32Sqrt,
            F32Add => Instruction::F32Add,
            F32Sub => Instruction::F32Sub,
            F32Mul => Instruction::F32Mul,
            F32Div => Instruction::F32Div,
            F32Min => Instruction::F32Min,
            F32Max => Instruction::F32Max,
            F32Copysign => Instruction::F32Copysign,
            F64Abs => Instruction::F64Abs,
            F64Neg => Instruction::F64Neg,
            F64Ceil => Instruction::F64Ceil,
            F64Floor => Instruction::F64Floor,
            F64Trunc => Instruction::F64Trunc,
            F64Nearest => Instruction::F64Nearest,
            F64Sqrt => Instruction::F64Sqrt,
            F64Add => Instruction::F64Add,
            F64Sub => Instruction::F64Sub,
            F64Mul => Instruction::F64Mul,
            F64Div => Instruction::F64Div,
            F64Min => Instruction::F64Min,
            F64Max => Instruction::F64Max,
            F64Copysign => Instruction::F64Copysign,
            I32WrapI64 => Instruction::I32WrapI64,
            I32TruncF32S => Instruction::I32TruncF32S,
            I32TruncF32U => Instruction::I32TruncF32U,
            I32TruncF64S => Instruction::I32TruncF64S,
            I32TruncF64U => Instruction::I32TruncF64U,
            I64Extend8S => Instruction::I64Extend8S,
            I64Extend16S => Instruction::I64Extend16S,
            I64Extend32S => Instruction::I64Extend32S,
            I64ExtendI32S => Instruction::I64ExtendI32S,
            I64ExtendI32U => Instruction::I64ExtendI32U,
            I32Extend8S => Instruction::I32Extend8S,
            I32Extend16S => Instruction::I32Extend16S,
            I64TruncF32S => Instruction::I64TruncF32S,
            I64TruncF32U => Instruction::I64TruncF32U,
            I64TruncF64S => Instruction::I64TruncF64S,
            I64TruncF64U => Instruction::I64TruncF64U,
            F32ConvertI32S => Instruction::F32ConvertI32S,
            F32ConvertI32U => Instruction::F32ConvertI32U,
            F32ConvertI64S => Instruction::F32ConvertI64S,
            F32ConvertI64U => Instruction::F32ConvertI64U,
            F32DemoteF64 => Instruction::F32DemoteF64,
            F64ConvertI32S => Instruction::F64ConvertI32S,
            F64ConvertI32U => Instruction::F64ConvertI32U,
            F64ConvertI64S => Instruction::F64ConvertI64S,
            F64ConvertI64U => Instruction::F64ConvertI64U,
            F64PromoteF32 => Instruction::F64PromoteF32,
            I32ReinterpretF32 => Instruction::I32ReinterpretF32,
            I64ReinterpretF64 => Instruction::I64ReinterpretF64,
            F32ReinterpretI32 => Instruction::F32ReinterpretI32,
            F64ReinterpretI64 => Instruction::F64ReinterpretI64,
            I32TruncSatF32S => Instruction::I32TruncSatF32S,
            I32TruncSatF32U => Instruction::I32TruncSatF32U,
            I32TruncSatF64S => Instruction::I32TruncSatF64S,
            I32TruncSatF64U => Instruction::I32TruncSatF64U,
            I64TruncSatF32S => Instruction::I64TruncSatF32S,
            I64TruncSatF32U => Instruction::I64TruncSatF32U,
            I64TruncSatF64S => Instruction::I64TruncSatF64S,
            I64TruncSatF64U => Instruction::I64TruncSatF64U,
            op => {
                return Err(crate::ParseError::UnsupportedOperator(format!(
                    "Unsupported instruction: {:?}",
                    op
                )))
            }
        };

        instructions.push(res);
    }

    if !labels_ptrs.is_empty() {
        panic!(
            "last_label_pointer should be None after processing all instructions: {:?}",
            labels_ptrs
        );
    }

    validator.finish(offset)?;

    Ok(instructions.into_boxed_slice())
}
