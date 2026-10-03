use alloc::{boxed::Box, vec, vec::Vec};

use crate::{ParseError, Result};
use tinywasm_types::{CanonicalArgument as Arg, CanonicalFunction as Owned, CanonicalOption as Opt};
use tinywasm_types::{
    ComponentAlias as Alias, ComponentDefinedType as Defined, ComponentOuterAliasKind as OuterKind,
    ComponentPrimitiveType as Primitive, ComponentType as Type, ComponentTypeDeclaration as Declaration,
    ComponentValueType as Value,
};
use tinywasm_types::{
    ComponentCoreType, ComponentName, ComponentValueType, CoreModuleTypeDeclaration, ExternalKind, TypeSection,
};
use wasmparser::{
    CanonicalFunction, CanonicalOption, ComponentAlias, ComponentDefinedType, ComponentOuterAliasKind, ComponentType,
    ComponentTypeDeclaration, ComponentValType, InstanceTypeDeclaration, PrimitiveValType,
};
use wasmparser::{ComponentExternalKind, ComponentTypeRef};

pub(super) fn convert_component_name(name: wasmparser::ComponentExternName<'_>) -> ComponentName {
    ComponentName {
        name: name.name.into(),
        implements: name.implements.map(Into::into),
        version_suffix: name.version_suffix.map(Into::into),
        external_id: name.external_id.map(Into::into),
    }
}

pub(super) fn convert_external_kind(kind: ComponentExternalKind) -> tinywasm_types::ComponentExternalKind {
    use tinywasm_types::ComponentExternalKind as Owned;
    match kind {
        ComponentExternalKind::Module => Owned::Module,
        ComponentExternalKind::Func => Owned::Func,
        ComponentExternalKind::Value => Owned::Value,
        ComponentExternalKind::Type => Owned::Type,
        ComponentExternalKind::Instance => Owned::Instance,
        ComponentExternalKind::Component => Owned::Component,
    }
}

pub(super) fn convert_type_ref(ty: ComponentTypeRef) -> tinywasm_types::ComponentTypeRef {
    use tinywasm_types::ComponentTypeRef as Owned;
    match ty {
        ComponentTypeRef::Module(index) => Owned::Module(index),
        ComponentTypeRef::Func(index) => Owned::Func(index),
        ComponentTypeRef::Value(value) => Owned::Value(match value {
            wasmparser::ComponentValType::Type(index) => ComponentValueType::Type(index),
            wasmparser::ComponentValType::Primitive(primitive) => {
                ComponentValueType::Primitive(primitive_type(primitive))
            }
        }),
        ComponentTypeRef::Type(wasmparser::TypeBounds::Eq(index)) => Owned::Type(index),
        ComponentTypeRef::Type(wasmparser::TypeBounds::SubResource) => Owned::SubResource,
        ComponentTypeRef::Instance(index) => Owned::Instance(index),
        ComponentTypeRef::Component(index) => Owned::Component(index),
    }
}

pub(super) fn convert_export(export: wasmparser::ComponentExport<'_>) -> tinywasm_types::ComponentExport {
    tinywasm_types::ComponentExport {
        name: convert_component_name(export.name),
        kind: convert_external_kind(export.kind),
        index: export.index,
        ty: export.ty.map(convert_type_ref),
    }
}

pub(super) fn convert_core_kind(kind: wasmparser::ExternalKind) -> Result<ExternalKind> {
    Ok(match kind {
        wasmparser::ExternalKind::Func => ExternalKind::Func,
        wasmparser::ExternalKind::Table => ExternalKind::Table,
        wasmparser::ExternalKind::Memory => ExternalKind::Memory,
        wasmparser::ExternalKind::Global => ExternalKind::Global,
        wasmparser::ExternalKind::Tag => ExternalKind::Tag,
        wasmparser::ExternalKind::FuncExact => {
            return Err(ParseError::UnsupportedSection("exact function export".into()));
        }
    })
}

pub(super) fn convert_core_type(
    ty: wasmparser::CoreType<'_>,
    next_index: &mut u32,
    limits: &crate::ParseLimits,
) -> Result<ComponentCoreType> {
    fn rec(group: wasmparser::RecGroup, next_index: &mut u32, limits: &crate::ParseLimits) -> Result<TypeSection> {
        limits.check(crate::ParseLimitKind::SectionItems, group.types().len())?;
        let mut types = Vec::new();
        let len = crate::conversion::convert_rec_group(group, *next_index, &mut types)?;
        *next_index = next_index.checked_add(len).ok_or_else(|| ParseError::Other("too many core types".into()))?;
        Ok(TypeSection { types: types.into_boxed_slice(), rec_group_lengths: Box::new([len]) })
    }

    Ok(match ty {
        wasmparser::CoreType::Rec(group) => ComponentCoreType::Rec(rec(group, next_index, limits)?),
        wasmparser::CoreType::Module(declarations) => {
            limits.check(crate::ParseLimitKind::SectionItems, declarations.len())?;
            let mut module_type_index = 0;
            let mut expanded = 0usize;
            let mut converted = Vec::with_capacity(declarations.len());
            for declaration in declarations {
                let count = match &declaration {
                    wasmparser::ModuleTypeDeclaration::Type(group) => group.types().len(),
                    _ => 1,
                };
                expanded = expanded
                    .checked_add(count)
                    .ok_or_else(|| ParseError::Other("module type count overflow".into()))?;
                limits.check(crate::ParseLimitKind::SectionItems, expanded)?;
                converted.push(match declaration {
                    wasmparser::ModuleTypeDeclaration::Type(group) => {
                        CoreModuleTypeDeclaration::Type(rec(group, &mut module_type_index, limits)?)
                    }
                    wasmparser::ModuleTypeDeclaration::Import(import) => {
                        CoreModuleTypeDeclaration::Import(crate::conversion::convert_module_import(import)?)
                    }
                    wasmparser::ModuleTypeDeclaration::Export { name, ty } => CoreModuleTypeDeclaration::Export {
                        name: name.into(),
                        ty: crate::conversion::convert_import_kind(ty)?,
                    },
                    wasmparser::ModuleTypeDeclaration::OuterAlias { count, index, .. } => {
                        module_type_index = module_type_index
                            .checked_add(1)
                            .ok_or_else(|| ParseError::Other("too many core types".into()))?;
                        CoreModuleTypeDeclaration::OuterAlias { count, index }
                    }
                });
            }
            *next_index = next_index.checked_add(1).ok_or_else(|| ParseError::Other("too many core types".into()))?;
            ComponentCoreType::Module(converted)
        }
    })
}

pub(super) fn primitive_type(ty: PrimitiveValType) -> Primitive {
    match ty {
        PrimitiveValType::Bool => Primitive::Bool,
        PrimitiveValType::S8 => Primitive::S8,
        PrimitiveValType::U8 => Primitive::U8,
        PrimitiveValType::S16 => Primitive::S16,
        PrimitiveValType::U16 => Primitive::U16,
        PrimitiveValType::S32 => Primitive::S32,
        PrimitiveValType::U32 => Primitive::U32,
        PrimitiveValType::S64 => Primitive::S64,
        PrimitiveValType::U64 => Primitive::U64,
        PrimitiveValType::F32 => Primitive::F32,
        PrimitiveValType::F64 => Primitive::F64,
        PrimitiveValType::Char => Primitive::Char,
        PrimitiveValType::String => Primitive::String,
        PrimitiveValType::ErrorContext => Primitive::ErrorContext,
    }
}

pub(super) fn value_type(ty: ComponentValType) -> Value {
    match ty {
        ComponentValType::Type(index) => Value::Type(index),
        ComponentValType::Primitive(ty) => Value::Primitive(primitive_type(ty)),
    }
}

pub(super) fn convert_alias(alias: ComponentAlias<'_>) -> Result<Alias> {
    Ok(match alias {
        ComponentAlias::InstanceExport { kind, instance_index, name } => {
            Alias::InstanceExport { kind: convert_external_kind(kind), instance: instance_index, name: name.into() }
        }
        ComponentAlias::CoreInstanceExport { kind, instance_index, name } => {
            Alias::CoreInstanceExport { kind: convert_core_kind(kind)?, instance: instance_index, name: name.into() }
        }
        ComponentAlias::Outer { kind, count, index } => Alias::Outer {
            kind: match kind {
                ComponentOuterAliasKind::CoreModule => OuterKind::CoreModule,
                ComponentOuterAliasKind::CoreType => OuterKind::CoreType,
                ComponentOuterAliasKind::Type => OuterKind::Type,
                ComponentOuterAliasKind::Component => OuterKind::Component,
            },
            count,
            index,
        },
    })
}

pub(super) fn convert_component_type(ty: ComponentType<'_>, limits: &crate::ParseLimits) -> Result<Type> {
    Ok(match ty {
        ComponentType::Defined(ty) => Type::Defined(match ty {
            ComponentDefinedType::Primitive(ty) => Defined::Primitive(primitive_type(ty)),
            ComponentDefinedType::Record(fields) => {
                limits.check(crate::ParseLimitKind::SectionItems, fields.len())?;
                Defined::Record(fields.iter().map(|(name, ty)| ((*name).into(), value_type(*ty))).collect())
            }
            ComponentDefinedType::Variant(cases) => {
                limits.check(crate::ParseLimitKind::SectionItems, cases.len())?;
                Defined::Variant(cases.iter().map(|case| (case.name.into(), case.ty.map(value_type))).collect())
            }
            ComponentDefinedType::List(ty) => Defined::List(value_type(ty)),
            ComponentDefinedType::Map(key, value) => Defined::Map(value_type(key), value_type(value)),
            ComponentDefinedType::FixedLengthList(ty, len) => Defined::FixedLengthList(value_type(ty), len),
            ComponentDefinedType::Tuple(types) => {
                limits.check(crate::ParseLimitKind::SectionItems, types.len())?;
                Defined::Tuple(types.iter().copied().map(value_type).collect())
            }
            ComponentDefinedType::Flags(names) => {
                limits.check(crate::ParseLimitKind::SectionItems, names.len())?;
                Defined::Flags(names.iter().map(|name| (*name).into()).collect())
            }
            ComponentDefinedType::Enum(names) => {
                limits.check(crate::ParseLimitKind::SectionItems, names.len())?;
                Defined::Enum(names.iter().map(|name| (*name).into()).collect())
            }
            ComponentDefinedType::Option(ty) => Defined::Option(value_type(ty)),
            ComponentDefinedType::Result { ok, err } => {
                Defined::Result { ok: ok.map(value_type), err: err.map(value_type) }
            }
            ComponentDefinedType::Own(index) => Defined::Own(index),
            ComponentDefinedType::Borrow(index) => Defined::Borrow(index),
            ComponentDefinedType::Future(ty) => Defined::Future(ty.map(value_type)),
            ComponentDefinedType::Stream(ty) => Defined::Stream(ty.map(value_type)),
        }),
        ComponentType::Func(ty) => {
            limits.check(crate::ParseLimitKind::SectionItems, ty.params.len())?;
            Type::Func {
                async_: ty.async_,
                params: ty.params.iter().map(|(name, ty)| ((*name).into(), value_type(*ty))).collect(),
                result: ty.result.map(value_type),
            }
        }
        ComponentType::Resource { rep, dtor } => {
            Type::Resource { rep: crate::conversion::convert_valtype(&rep)?, dtor }
        }
        ComponentType::Component(declarations) => {
            limits.check(crate::ParseLimitKind::SectionItems, declarations.len())?;
            let mut next_core_type = 0;
            Type::Component(
                declarations
                    .into_vec()
                    .into_iter()
                    .map(|decl| convert_type_declaration(decl, &mut next_core_type, limits))
                    .collect::<Result<Vec<_>>>()?,
            )
        }
        ComponentType::Instance(declarations) => {
            limits.check(crate::ParseLimitKind::SectionItems, declarations.len())?;
            let mut next_core_type = 0;
            Type::Instance(
                declarations
                    .into_vec()
                    .into_iter()
                    .map(|decl| {
                        convert_type_declaration(
                            match decl {
                                InstanceTypeDeclaration::CoreType(ty) => ComponentTypeDeclaration::CoreType(ty),
                                InstanceTypeDeclaration::Type(ty) => ComponentTypeDeclaration::Type(ty),
                                InstanceTypeDeclaration::Alias(alias) => ComponentTypeDeclaration::Alias(alias),
                                InstanceTypeDeclaration::Export { name, ty } => {
                                    ComponentTypeDeclaration::Export { name, ty }
                                }
                            },
                            &mut next_core_type,
                            limits,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
        }
    })
}

fn convert_type_declaration(
    decl: ComponentTypeDeclaration<'_>,
    next_core_type: &mut u32,
    limits: &crate::ParseLimits,
) -> Result<Declaration> {
    Ok(match decl {
        ComponentTypeDeclaration::CoreType(ty) => Declaration::CoreType(convert_core_type(ty, next_core_type, limits)?),
        ComponentTypeDeclaration::Type(ty) => Declaration::Type(convert_component_type(ty, limits)?),
        ComponentTypeDeclaration::Alias(alias) => {
            if let ComponentAlias::Outer { kind: ComponentOuterAliasKind::CoreType, .. } = alias {
                *next_core_type =
                    next_core_type.checked_add(1).ok_or_else(|| ParseError::Other("too many core types".into()))?;
            }
            Declaration::Alias(convert_alias(alias)?)
        }
        ComponentTypeDeclaration::Import(import) => {
            Declaration::Import { name: convert_component_name(import.name), ty: convert_type_ref(import.ty) }
        }
        ComponentTypeDeclaration::Export { name, ty } => {
            Declaration::Export { name: convert_component_name(name), ty: convert_type_ref(ty) }
        }
    })
}

fn convert_option(option: CanonicalOption) -> Opt {
    match option {
        CanonicalOption::UTF8 => Opt::UTF8,
        CanonicalOption::UTF16 => Opt::UTF16,
        CanonicalOption::CompactUTF16 => Opt::CompactUTF16,
        CanonicalOption::Async => Opt::Async,
        CanonicalOption::Gc => Opt::Gc,
        CanonicalOption::Memory(index) => Opt::Memory(index),
        CanonicalOption::Realloc(index) => Opt::Realloc(index),
        CanonicalOption::PostReturn(index) => Opt::PostReturn(index),
        CanonicalOption::Callback(index) => Opt::Callback(index),
        CanonicalOption::CoreType(index) => Opt::CoreType(index),
    }
}

pub(super) fn convert_canonical(func: CanonicalFunction) -> Result<Owned> {
    let (name, args, options) = match func {
        CanonicalFunction::Lift { core_func_index, type_index, options } => (
            "lift",
            vec![Arg::Index(core_func_index), Arg::Index(type_index)],
            options.into_vec().into_iter().map(convert_option).collect(),
        ),
        CanonicalFunction::Lower { func_index, options } => {
            ("lower", vec![Arg::Index(func_index)], options.into_vec().into_iter().map(convert_option).collect())
        }
        CanonicalFunction::ResourceNew { resource } => ("resource.new", vec![Arg::Index(resource)], Vec::new()),
        CanonicalFunction::ResourceDrop { resource } => ("resource.drop", vec![Arg::Index(resource)], Vec::new()),
        CanonicalFunction::ResourceRep { resource } => ("resource.rep", vec![Arg::Index(resource)], Vec::new()),
        CanonicalFunction::ThreadSpawnRef { func_ty_index } => {
            ("thread.spawn-ref", vec![Arg::Index(func_ty_index)], Vec::new())
        }
        CanonicalFunction::ThreadSpawnIndirect { func_ty_index, table_index } => {
            ("thread.spawn-indirect", vec![Arg::Index(func_ty_index), Arg::Index(table_index)], Vec::new())
        }
        CanonicalFunction::ThreadAvailableParallelism => ("thread.available_parallelism", Vec::new(), Vec::new()),
        CanonicalFunction::BackpressureInc => ("backpressure.inc", Vec::new(), Vec::new()),
        CanonicalFunction::BackpressureDec => ("backpressure.dec", Vec::new(), Vec::new()),
        CanonicalFunction::TaskReturn { result, options } => (
            "task.return",
            vec![Arg::ResultType(result.map(value_type))],
            options.into_vec().into_iter().map(convert_option).collect(),
        ),
        CanonicalFunction::TaskCancel => ("task.cancel", Vec::new(), Vec::new()),
        CanonicalFunction::ContextGet { ty, slot } => (
            "context.get",
            vec![Arg::CoreValueType(crate::conversion::convert_valtype(&ty)?), Arg::Index(slot)],
            Vec::new(),
        ),
        CanonicalFunction::ContextSet { ty, slot } => (
            "context.set",
            vec![Arg::CoreValueType(crate::conversion::convert_valtype(&ty)?), Arg::Index(slot)],
            Vec::new(),
        ),
        CanonicalFunction::ThreadYield => ("thread.yield", Vec::new(), Vec::new()),
        CanonicalFunction::SubtaskDrop => ("subtask.drop", Vec::new(), Vec::new()),
        CanonicalFunction::SubtaskCancel { async_ } => ("subtask.cancel", vec![Arg::Bool(async_)], Vec::new()),
        CanonicalFunction::StreamNew { ty } => ("stream.new", vec![Arg::Index(ty)], Vec::new()),
        CanonicalFunction::StreamRead { ty, options } => {
            ("stream.read", vec![Arg::Index(ty)], options.into_vec().into_iter().map(convert_option).collect())
        }
        CanonicalFunction::StreamWrite { ty, options } => {
            ("stream.write", vec![Arg::Index(ty)], options.into_vec().into_iter().map(convert_option).collect())
        }
        CanonicalFunction::StreamForward { ty } => ("stream.forward", vec![Arg::Index(ty)], Vec::new()),
        CanonicalFunction::StreamCancelRead { ty, async_ } => {
            ("stream.cancel-read", vec![Arg::Index(ty), Arg::Bool(async_)], Vec::new())
        }
        CanonicalFunction::StreamCancelWrite { ty, async_ } => {
            ("stream.cancel-write", vec![Arg::Index(ty), Arg::Bool(async_)], Vec::new())
        }
        CanonicalFunction::StreamDropReadable { ty } => ("stream.drop-readable", vec![Arg::Index(ty)], Vec::new()),
        CanonicalFunction::StreamDropWritable { ty } => ("stream.drop-writable", vec![Arg::Index(ty)], Vec::new()),
        CanonicalFunction::FutureNew { ty } => ("future.new", vec![Arg::Index(ty)], Vec::new()),
        CanonicalFunction::FutureRead { ty, options } => {
            ("future.read", vec![Arg::Index(ty)], options.into_vec().into_iter().map(convert_option).collect())
        }
        CanonicalFunction::FutureWrite { ty, options } => {
            ("future.write", vec![Arg::Index(ty)], options.into_vec().into_iter().map(convert_option).collect())
        }
        CanonicalFunction::FutureForward { ty } => ("future.forward", vec![Arg::Index(ty)], Vec::new()),
        CanonicalFunction::FutureCancelRead { ty, async_ } => {
            ("future.cancel-read", vec![Arg::Index(ty), Arg::Bool(async_)], Vec::new())
        }
        CanonicalFunction::FutureCancelWrite { ty, async_ } => {
            ("future.cancel-write", vec![Arg::Index(ty), Arg::Bool(async_)], Vec::new())
        }
        CanonicalFunction::FutureDropReadable { ty } => ("future.drop-readable", vec![Arg::Index(ty)], Vec::new()),
        CanonicalFunction::FutureDropWritable { ty } => ("future.drop-writable", vec![Arg::Index(ty)], Vec::new()),
        CanonicalFunction::ErrorContextNew { options } => {
            ("error-context.new", Vec::new(), options.into_vec().into_iter().map(convert_option).collect())
        }
        CanonicalFunction::ErrorContextDebugMessage { options } => {
            ("error-context.debug-message", Vec::new(), options.into_vec().into_iter().map(convert_option).collect())
        }
        CanonicalFunction::ErrorContextDrop => ("error-context.drop", Vec::new(), Vec::new()),
        CanonicalFunction::WaitableSetNew => ("waitable-set.new", Vec::new(), Vec::new()),
        CanonicalFunction::WaitableSetWait { memory } => ("waitable-set.wait", vec![Arg::Index(memory)], Vec::new()),
        CanonicalFunction::WaitableSetPoll { memory } => ("waitable-set.poll", vec![Arg::Index(memory)], Vec::new()),
        CanonicalFunction::WaitableSetDrop => ("waitable-set.drop", Vec::new(), Vec::new()),
        CanonicalFunction::WaitableJoin => ("waitable.join", Vec::new(), Vec::new()),
        CanonicalFunction::ThreadIndex => ("thread.index", Vec::new(), Vec::new()),
        CanonicalFunction::ThreadNewIndirect { func_ty_index, table_index } => {
            ("thread.new-indirect", vec![Arg::Index(func_ty_index), Arg::Index(table_index)], Vec::new())
        }
        CanonicalFunction::ThreadResumeLater => ("thread.resume-later", Vec::new(), Vec::new()),
        CanonicalFunction::ThreadSuspend => ("thread.suspend", Vec::new(), Vec::new()),
        CanonicalFunction::ThreadSuspendThenResume => ("thread.suspend-then-resume", Vec::new(), Vec::new()),
        CanonicalFunction::ThreadYieldThenResume => ("thread.yield-then-resume", Vec::new(), Vec::new()),
        CanonicalFunction::ThreadSuspendThenPromote => ("thread.suspend-then-promote", Vec::new(), Vec::new()),
        CanonicalFunction::ThreadYieldThenPromote => ("thread.yield-then-promote", Vec::new(), Vec::new()),
    };
    Ok(Owned { name: name.into(), args, options })
}
