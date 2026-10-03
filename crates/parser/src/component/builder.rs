use super::conversion::{
    convert_component_name, convert_core_kind, convert_core_type, convert_export, convert_external_kind,
    convert_type_ref,
};
use super::*;
use alloc::boxed::Box;
use tinywasm_types::{
    ComponentCoreInstance, ComponentCoreModule, ComponentDeclaration, ComponentIndex, ComponentInstanceArg,
    ComponentInstantiationArg, Module,
};
use wasmparser::{ComponentAlias, ComponentExternalKind, ComponentOuterAliasKind, ComponentTypeRef};

#[derive(Default)]
pub(super) struct ComponentBuilder {
    component: Component,
    pub(super) parents: Vec<(Component, u32)>,
    next_core_type: u32,
}

impl ComponentBuilder {
    pub(super) fn push_module(&mut self, module: Module) {
        self.component
            .declarations
            .push(ComponentDeclaration::CoreModule(ComponentCoreModule::Defined(self.component.modules.len())));
        self.component.modules.push(module);
    }

    pub(super) fn enter_component(&mut self) {
        self.parents.push((core::mem::take(&mut self.component), core::mem::take(&mut self.next_core_type)));
    }

    pub(super) fn leave_component(&mut self) {
        let (mut parent, next_core_type) = self.parents.pop().expect("nested component without parent");
        parent.declarations.push(ComponentDeclaration::Component(ComponentIndex::Defined(Box::new(core::mem::take(
            &mut self.component,
        )))));
        self.component = parent;
        self.next_core_type = next_core_type;
    }

    pub(super) fn record_declarations(&mut self, payload: &Payload<'_>, limits: &crate::ParseLimits) -> Result<()> {
        match payload {
            Payload::ComponentImportSection(reader) => {
                limits.check(ParseLimitKind::SectionItems, reader.count() as usize)?;
                for import in reader.clone() {
                    let import = import?;
                    let name = convert_component_name(import.name);
                    match import.ty {
                        ComponentTypeRef::Module(ty) => self
                            .component
                            .declarations
                            .push(ComponentDeclaration::CoreModule(ComponentCoreModule::Import { name, ty })),
                        ComponentTypeRef::Component(ty) => self
                            .component
                            .declarations
                            .push(ComponentDeclaration::Component(ComponentIndex::Import { name, ty })),
                        ty => self
                            .component
                            .declarations
                            .push(ComponentDeclaration::Import { name, ty: convert_type_ref(ty) }),
                    }
                }
            }
            Payload::ComponentExportSection(reader) => {
                limits.check(ParseLimitKind::SectionItems, reader.count() as usize)?;
                for export in reader.clone() {
                    self.component.declarations.push(ComponentDeclaration::Export(convert_export(export?)));
                }
            }
            Payload::ComponentInstanceSection(reader) => {
                limits.check(ParseLimitKind::SectionItems, reader.count() as usize)?;
                for instance in reader.clone() {
                    let instance = match instance? {
                        wasmparser::ComponentInstance::Instantiate { component_index, args } => {
                            tinywasm_types::ComponentInstance::Instantiate {
                                component: component_index,
                                args: args
                                    .iter()
                                    .map(|arg| ComponentInstanceArg {
                                        name: arg.name.into(),
                                        kind: convert_external_kind(arg.kind),
                                        index: arg.index,
                                    })
                                    .collect(),
                            }
                        }
                        wasmparser::ComponentInstance::FromExports(exports) => {
                            tinywasm_types::ComponentInstance::FromExports(
                                exports.iter().cloned().map(convert_export).collect(),
                            )
                        }
                    };
                    self.component.declarations.push(ComponentDeclaration::Instance(instance));
                }
            }
            Payload::ComponentStartSection { start, .. } => {
                self.component.declarations.push(ComponentDeclaration::Start(tinywasm_types::ComponentStart {
                    func: start.func_index,
                    arguments: start.arguments.to_vec(),
                    results: start.results,
                }));
            }
            Payload::ComponentAliasSection(reader) => {
                limits.check(ParseLimitKind::SectionItems, reader.count() as usize)?;
                for alias in reader.clone() {
                    match alias? {
                        ComponentAlias::InstanceExport {
                            kind: ComponentExternalKind::Module,
                            instance_index,
                            name,
                        } => {
                            self.component.declarations.push(ComponentDeclaration::CoreModule(
                                ComponentCoreModule::InstanceExport { instance: instance_index, name: name.into() },
                            ));
                        }
                        ComponentAlias::InstanceExport {
                            kind: ComponentExternalKind::Component,
                            instance_index,
                            name,
                        } => {
                            self.component.declarations.push(ComponentDeclaration::Component(
                                ComponentIndex::InstanceExport { instance: instance_index, name: name.into() },
                            ));
                        }
                        ComponentAlias::Outer { kind: ComponentOuterAliasKind::CoreModule, count, index } => {
                            self.component.declarations.push(ComponentDeclaration::CoreModule(
                                ComponentCoreModule::OuterAlias { count, index },
                            ));
                        }
                        ComponentAlias::Outer { kind: ComponentOuterAliasKind::Component, count, index } => {
                            self.component
                                .declarations
                                .push(ComponentDeclaration::Component(ComponentIndex::OuterAlias { count, index }));
                        }
                        ComponentAlias::InstanceExport { kind, instance_index, name } => {
                            self.component.declarations.push(ComponentDeclaration::Alias(
                                tinywasm_types::ComponentAlias::InstanceExport {
                                    kind: convert_external_kind(kind),
                                    instance: instance_index,
                                    name: name.into(),
                                },
                            ));
                        }
                        ComponentAlias::CoreInstanceExport { kind, instance_index, name } => {
                            self.component.declarations.push(ComponentDeclaration::Alias(
                                tinywasm_types::ComponentAlias::CoreInstanceExport {
                                    kind: convert_core_kind(kind)?,
                                    instance: instance_index,
                                    name: name.into(),
                                },
                            ));
                        }
                        ComponentAlias::Outer { kind, count, index } => {
                            let kind = match kind {
                                ComponentOuterAliasKind::CoreType => {
                                    self.next_core_type = self
                                        .next_core_type
                                        .checked_add(1)
                                        .ok_or_else(|| ParseError::Other("too many core types".into()))?;
                                    tinywasm_types::ComponentOuterAliasKind::CoreType
                                }
                                ComponentOuterAliasKind::Type => tinywasm_types::ComponentOuterAliasKind::Type,
                                ComponentOuterAliasKind::CoreModule | ComponentOuterAliasKind::Component => {
                                    unreachable!()
                                }
                            };
                            self.component.declarations.push(ComponentDeclaration::Alias(
                                tinywasm_types::ComponentAlias::Outer { kind, count, index },
                            ));
                        }
                    }
                }
            }
            Payload::InstanceSection(reader) => {
                limits.check(ParseLimitKind::SectionItems, reader.count() as usize)?;
                for instance in reader.clone() {
                    let instance = match instance? {
                        wasmparser::Instance::Instantiate { module_index, args } => {
                            ComponentCoreInstance::Instantiate {
                                module: module_index,
                                args: args
                                    .iter()
                                    .map(|arg| ComponentInstantiationArg { name: arg.name.into(), instance: arg.index })
                                    .collect(),
                            }
                        }
                        wasmparser::Instance::FromExports(exports) => ComponentCoreInstance::FromExports(
                            exports
                                .iter()
                                .map(|export| {
                                    let kind = convert_core_kind(export.kind)?;
                                    Ok(tinywasm_types::Export { name: export.name.into(), kind, index: export.index })
                                })
                                .collect::<Result<_>>()?,
                        ),
                    };
                    self.component.declarations.push(ComponentDeclaration::CoreInstance(instance));
                }
            }
            Payload::CoreTypeSection(reader) => {
                limits.check(ParseLimitKind::SectionItems, reader.count() as usize)?;
                let mut expanded = 0usize;
                for ty in reader.clone() {
                    let ty = ty?;
                    let count = match &ty {
                        wasmparser::CoreType::Rec(group) => group.types().len(),
                        wasmparser::CoreType::Module(_) => 1,
                    };
                    expanded = expanded
                        .checked_add(count)
                        .ok_or_else(|| ParseError::Other("core type count overflow".into()))?;
                    limits.check(ParseLimitKind::SectionItems, expanded)?;
                    let ty = convert_core_type(ty, &mut self.next_core_type, limits)?;
                    self.component.declarations.push(ComponentDeclaration::CoreType(ty));
                }
            }
            Payload::ComponentTypeSection(reader) => {
                limits.check(ParseLimitKind::SectionItems, reader.count() as usize)?;
                for ty in reader.clone() {
                    self.component
                        .declarations
                        .push(ComponentDeclaration::Type(conversion::convert_component_type(ty?, limits)?));
                }
            }
            Payload::ComponentCanonicalSection(reader) => {
                limits.check(ParseLimitKind::SectionItems, reader.count() as usize)?;
                for func in reader.clone() {
                    self.component
                        .declarations
                        .push(ComponentDeclaration::Canonical(conversion::convert_canonical(func?)?));
                }
            }
            Payload::UnknownSection { .. } => return Err(ParseError::UnsupportedSection("Unknown section".into())),
            _ => {}
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Component {
        self.component
    }
}
