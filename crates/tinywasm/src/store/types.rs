use alloc::{boxed::Box, vec::Vec};
use tinywasm_types::*;

use super::Store;

pub(crate) fn canonicalize_ref_type(ty: RefType, type_addrs: &[TypeAddr]) -> RefType {
    let Some(type_addr) = ty.type_index() else { return ty };
    let canonical = *type_addrs.get(type_addr as usize).expect("validated type address should exist");
    RefType::new_concrete(ty.is_nullable(), canonical).expect("canonical type addresses fit in references")
}

pub(crate) fn canonicalize_value_type(ty: WasmType, type_addrs: &[TypeAddr]) -> WasmType {
    match ty {
        WasmType::Ref(ty) => WasmType::Ref(canonicalize_ref_type(ty, type_addrs)),
        ty => ty,
    }
}

fn map_value_type(ty: WasmType, resolve: &mut impl FnMut(TypeAddr) -> TypeAddr) -> WasmType {
    match ty {
        WasmType::Ref(ty) if ty.is_concrete() => {
            let addr = resolve(ty.type_index().expect("concrete reference has a type index"));
            WasmType::Ref(
                RefType::new_concrete(ty.is_nullable(), addr).expect("canonical type addresses fit in references"),
            )
        }
        ty => ty,
    }
}

fn map_field_type(field: FieldType, resolve: &mut impl FnMut(TypeAddr) -> TypeAddr) -> FieldType {
    let storage = match field.storage {
        StorageType::Value(ty) => StorageType::Value(map_value_type(ty, resolve)),
        storage => storage,
    };
    FieldType { storage, mutable: field.mutable }
}

fn map_subtype(ty: &SubType, mut resolve: impl FnMut(TypeAddr) -> TypeAddr) -> SubType {
    let supertype = ty.supertype.map(&mut resolve);
    let composite = match &ty.composite {
        CompositeType::Func(ty) => {
            let params = ty.params().iter().copied().map(|ty| map_value_type(ty, &mut resolve)).collect::<Vec<_>>();
            let results = ty.results().iter().copied().map(|ty| map_value_type(ty, &mut resolve)).collect::<Vec<_>>();
            CompositeType::Func(FuncType::new(&params, &results))
        }
        CompositeType::Struct(ty) => CompositeType::Struct(StructType {
            fields: ty.fields.iter().copied().map(|field| map_field_type(field, &mut resolve)).collect(),
        }),
        CompositeType::Array(ty) => CompositeType::Array(ArrayType { field: map_field_type(ty.field, &mut resolve) }),
    };
    SubType { is_final: ty.is_final, supertype, composite }
}

fn subtypes_equal(ty: &SubType, registered: &SubType, resolve: impl Fn(TypeAddr) -> TypeAddr + Copy) -> bool {
    let values_equal = |ty: WasmType, registered: WasmType| match (ty, registered) {
        (WasmType::Ref(ty), WasmType::Ref(registered)) if ty.is_concrete() => {
            ty.is_nullable() == registered.is_nullable() && ty.type_index().map(resolve) == registered.type_index()
        }
        _ => ty == registered,
    };
    let fields_equal = |ty: FieldType, registered: FieldType| {
        ty.mutable == registered.mutable
            && match (ty.storage, registered.storage) {
                (StorageType::Value(ty), StorageType::Value(registered)) => values_equal(ty, registered),
                (ty, registered) => ty == registered,
            }
    };

    ty.is_final == registered.is_final
        && ty.supertype.map(resolve) == registered.supertype
        && match (&ty.composite, &registered.composite) {
            (CompositeType::Func(ty), CompositeType::Func(registered)) => {
                ty.params().len() == registered.params().len()
                    && ty.results().len() == registered.results().len()
                    && ty
                        .params()
                        .iter()
                        .chain(ty.results())
                        .copied()
                        .zip(registered.params().iter().chain(registered.results()).copied())
                        .all(|(ty, registered)| values_equal(ty, registered))
            }
            (CompositeType::Struct(ty), CompositeType::Struct(registered)) => {
                ty.fields.len() == registered.fields.len()
                    && ty
                        .fields
                        .iter()
                        .copied()
                        .zip(registered.fields.iter().copied())
                        .all(|(ty, registered)| fields_equal(ty, registered))
            }
            (CompositeType::Array(ty), CompositeType::Array(registered)) => fields_equal(ty.field, registered.field),
            _ => false,
        }
}

impl Store {
    pub(crate) fn register_module_types(&mut self, section: &TypeSection) -> Box<[TypeAddr]> {
        let mut type_addrs = Vec::with_capacity(section.types.len());
        let mut module_group_start = 0usize;

        for &group_len in &section.rec_group_lengths {
            let group_len = group_len as usize;
            let module_group_end = module_group_start.checked_add(group_len).expect("type group is too large");
            let group = section
                .types
                .get(module_group_start..module_group_end)
                .expect("validated recursive group length fits the type section");

            let resolve = |module_addr: TypeAddr, canonical_group_start: usize| {
                let module_addr = module_addr as usize;
                if (module_group_start..module_group_end).contains(&module_addr) {
                    (canonical_group_start + module_addr - module_group_start) as TypeAddr
                } else {
                    *type_addrs
                        .get(module_addr)
                        .expect("validated type reference targets the current or a prior recursive group")
                }
            };

            let mut canonical_group_start = 0;
            let mut matching_group = None;
            for &canonical_group_len in &self.state.canonical_rec_group_lengths {
                let canonical_group_len = canonical_group_len as usize;
                if canonical_group_len == group_len
                    && group.iter().zip(&self.state.canonical_types[canonical_group_start..]).all(|(ty, registered)| {
                        subtypes_equal(ty, registered, |addr| resolve(addr, canonical_group_start))
                    })
                {
                    matching_group = Some(canonical_group_start);
                    break;
                }
                canonical_group_start += canonical_group_len;
            }

            let canonical_group_start = match matching_group {
                Some(start) => start,
                None => {
                    let start = self.state.canonical_types.len();
                    assert!(
                        start.checked_add(group_len).is_some_and(|end| end <= (1 << 30)),
                        "too many canonical types"
                    );
                    self.state
                        .canonical_types
                        .extend(group.iter().map(|ty| map_subtype(ty, |addr| resolve(addr, start))));
                    self.state.canonical_rec_group_lengths.push(group_len as u32);
                    start
                }
            };
            type_addrs.extend((canonical_group_start..canonical_group_start + group_len).map(|addr| addr as TypeAddr));
            module_group_start = module_group_end;
        }
        debug_assert_eq!(module_group_start, section.types.len());
        type_addrs.into_boxed_slice()
    }

    pub(crate) fn register_host_type(&mut self, ty: &FuncType) -> TypeAddr {
        let mut group_start = 0usize;
        for &group_len in &self.state.canonical_rec_group_lengths {
            if group_len == 1
                && self.state.canonical_types[group_start].is_final
                && self.state.canonical_types[group_start].supertype.is_none()
                && self.state.canonical_types[group_start].as_func() == Some(ty)
            {
                return group_start as TypeAddr;
            }
            group_start += group_len as usize;
        }
        let addr = self.state.canonical_types.len();
        assert!(addr < (1 << 30), "too many canonical types");
        self.state.canonical_types.push(SubType {
            is_final: true,
            supertype: None,
            composite: CompositeType::Func(ty.clone()),
        });
        self.state.canonical_rec_group_lengths.push(1);
        addr as TypeAddr
    }
}
