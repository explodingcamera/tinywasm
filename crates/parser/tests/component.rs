#![cfg(feature = "unstable-component-model")]

use tinywasm_parser::{ParseError, ParseLimitKind, ParseLimits, Parser, ParserOptions};
use tinywasm_types::{
    ComponentCoreInstance, ComponentCoreModule, ComponentCoreType, ComponentDeclaration, ComponentIndex,
    ComponentInstance,
};
use wasmparser::{Encoding, Payload};

#[test]
fn preserves_types_and_canonical_functions() {
    let wasm = wat::parse_str(
        r#"(component
            (core module $m (func (export "f")))
            (core instance $i (instantiate $m))
            (alias core export $i "f" (core func $f))
            (type $t (func))
            (func $lift (type $t) (canon lift (core func $f)))
            (core func (canon lower (func $lift)))
            (export "lift" (func $lift)))"#,
    )
    .unwrap();
    let parser = Parser::default();
    let parsed = parser.parse_component_bytes(&wasm).unwrap();
    assert!(matches!(parsed.declarations[3], ComponentDeclaration::Type(tinywasm_types::ComponentType::Func { .. })));
    assert!(matches!(
        parsed.declarations[4],
        ComponentDeclaration::Canonical(ref func)
            if func.name.as_ref() == "lift" && func.args == [tinywasm_types::CanonicalArgument::Index(0), tinywasm_types::CanonicalArgument::Index(0)]
    ));
    assert!(matches!(
        parsed.declarations[5],
        ComponentDeclaration::Canonical(ref func)
            if func.name.as_ref() == "lower" && func.args == [tinywasm_types::CanonicalArgument::Index(0)]
    ));
    #[cfg(feature = "std")]
    assert!(parsed == parser.parse_component_stream(wasm.as_slice()).unwrap());
}

#[test]
fn preserves_canonical_typed_arguments() {
    let wasm = wat::parse_str("(component (core func (canon context.get i32 0)))").unwrap();
    let parsed = Parser::default().parse_component_bytes(&wasm).unwrap();
    assert!(matches!(
        &parsed.declarations[0],
        ComponentDeclaration::Canonical(func)
            if func.name.as_ref() == "context.get"
                && func.args == [
                    tinywasm_types::CanonicalArgument::CoreValueType(tinywasm_types::WasmType::I32),
                    tinywasm_types::CanonicalArgument::Index(0),
                ]
    ));
}

#[test]
fn preserves_nested_type_declarations() {
    let wasm = wat::parse_str(
        r#"(component
            (type $r (record (field "name" string)))
            (type (component
                (type $f (func (param "name" string)))
                (import "item" (func (type $f)))
                (export "item" (func (type $f)))))
            (type (instance
                (type $f (func))
                (export "item" (func (type $f))))))"#,
    )
    .unwrap();
    let parser = Parser::default();
    let parsed = parser.parse_component_bytes(&wasm).unwrap();
    assert!(matches!(
        parsed.declarations[0],
        ComponentDeclaration::Type(tinywasm_types::ComponentType::Defined(
            tinywasm_types::ComponentDefinedType::Record(_)
        ))
    ));
    assert!(
        matches!(parsed.declarations[1], ComponentDeclaration::Type(tinywasm_types::ComponentType::Component(ref declarations)) if matches!(declarations[..], [tinywasm_types::ComponentTypeDeclaration::Type(_), tinywasm_types::ComponentTypeDeclaration::Import { .. }, tinywasm_types::ComponentTypeDeclaration::Export { .. }]))
    );
    assert!(
        matches!(parsed.declarations[2], ComponentDeclaration::Type(tinywasm_types::ComponentType::Instance(ref declarations)) if matches!(declarations[..], [tinywasm_types::ComponentTypeDeclaration::Type(_), tinywasm_types::ComponentTypeDeclaration::Export { .. }]))
    );
    #[cfg(feature = "std")]
    assert!(parsed == parser.parse_component_stream(wasm.as_slice()).unwrap());
}

#[test]
fn preserves_core_type_groups_and_module_type_references() {
    let wasm = wat::parse_str(
        r#"(component
            (core type (func))
            (core type (module
                (type $f (func (param i32)))
                (import "host" "f" (func (type $f)))
                (export "f" (func (type $f))))))"#,
    )
    .unwrap();
    let parsed = Parser::default().parse_component_bytes(&wasm).unwrap();
    assert!(matches!(parsed.declarations[0], ComponentDeclaration::CoreType(ComponentCoreType::Rec(_))));
    let ComponentDeclaration::CoreType(ComponentCoreType::Module(ref declarations)) = parsed.declarations[1] else {
        panic!("expected core module type");
    };
    assert!(matches!(declarations[0], tinywasm_types::CoreModuleTypeDeclaration::Type(_)));
    assert!(
        matches!(declarations[1], tinywasm_types::CoreModuleTypeDeclaration::Import(ref import) if matches!(import.kind, tinywasm_types::ImportKind::Function(0)))
    );
    assert!(
        matches!(declarations[2], tinywasm_types::CoreModuleTypeDeclaration::Export { ref ty, .. } if matches!(ty, tinywasm_types::ImportKind::Function(0)))
    );
}

#[test]
fn preserves_core_type_indices_in_module_type() {
    let wasm = wat::parse_str(
        r#"(component
            (core type (func))
            (core type (module
                (alias outer 1 0 (type))
                (rec (type $f (func)) (type (func (param (ref null $f))))))))"#,
    )
    .unwrap();
    let parsed = Parser::default().parse_component_bytes(&wasm).unwrap();
    let ComponentDeclaration::CoreType(ComponentCoreType::Module(declarations)) = &parsed.declarations[1] else {
        panic!("expected core module type");
    };
    assert!(matches!(declarations[0], tinywasm_types::CoreModuleTypeDeclaration::OuterAlias { count: 1, index: 0 }));
    let tinywasm_types::CoreModuleTypeDeclaration::Type(group) = &declarations[1] else {
        panic!("expected recursive core type");
    };
    let param = group.types[1].as_func().unwrap().params()[0];
    assert!(matches!(param, tinywasm_types::WasmType::Ref(reference) if reference.type_index() == Some(1)));
}

#[test]
fn preserves_component_instantiation() {
    let wasm = wat::parse_str("(component (component $c) (instance (instantiate $c)))").unwrap();
    let parsed = Parser::default().parse_component_bytes(&wasm).unwrap();
    assert!(matches!(
        parsed.declarations[1],
        ComponentDeclaration::Instance(ComponentInstance::Instantiate { component: 0, ref args }) if args.is_empty()
    ));
}

#[test]
fn preserves_component_export() {
    let wasm = wat::parse_str(
        r#"(component
            (core module $m)
            (export "m" (core module $m)))"#,
    )
    .unwrap();
    let parser = Parser::default();
    let parsed = parser.parse_component_bytes(&wasm).unwrap();
    assert!(matches!(
        &parsed.declarations[1],
        ComponentDeclaration::Export(export)
            if export.name.name.as_ref() == "m"
                && export.kind == tinywasm_types::ComponentExternalKind::Module
                && export.index == 0
    ));
    #[cfg(feature = "std")]
    assert!(parsed == parser.parse_component_stream(wasm.as_slice()).unwrap());
}

#[test]
fn preserves_core_instantiation() {
    let wasm = wat::parse_str(
        r#"(component
            (core module $m)
            (core instance (instantiate $m)))"#,
    )
    .unwrap();
    let component = Parser::default().parse_component_bytes(wasm).unwrap();
    assert!(matches!(
        component.declarations[1],
        ComponentDeclaration::CoreInstance(ComponentCoreInstance::Instantiate { module: 0, ref args }) if args.is_empty()
    ));
}

#[test]
fn preserves_core_module_indices_and_scopes() {
    let wasm = wat::parse_str(
        r#"(component
            (core type $module-type (module))
            (import "dep" (core module (type $module-type)))
            (core module (func (export "outer")))
            (component (core module (func (export "inner")))))"#,
    )
    .unwrap();
    let parser = Parser::default();
    let component = parser.parse_component_bytes(&wasm).unwrap();
    assert_eq!(component.modules.len(), 1);
    assert!(matches!(component.declarations[0], ComponentDeclaration::CoreType(ComponentCoreType::Module(_))));
    assert!(
        matches!(component.declarations[1], ComponentDeclaration::CoreModule(ComponentCoreModule::Import { ref name, ty: 0 }) if name.name.as_ref() == "dep")
    );
    assert!(matches!(component.declarations[2], ComponentDeclaration::CoreModule(ComponentCoreModule::Defined(0))));
    assert!(
        matches!(component.declarations[3], ComponentDeclaration::Component(ComponentIndex::Defined(ref nested)) if nested.modules.len() == 1 && matches!(nested.declarations[..], [ComponentDeclaration::CoreModule(ComponentCoreModule::Defined(0))]))
    );

    #[cfg(feature = "std")]
    {
        let streamed = parser.parse_component_stream(wasm.as_slice()).unwrap();
        assert!(streamed == component);
    }
}

#[test]
fn preserves_outer_module_alias() {
    let wasm = wat::parse_str(
        r#"(component
            (core module)
            (component
                (alias outer 1 0 (core module))
                (core module)))"#,
    )
    .unwrap();
    let component = Parser::default().parse_component_bytes(wasm).unwrap();
    let ComponentDeclaration::Component(ComponentIndex::Defined(nested)) = &component.declarations[1] else {
        panic!("expected nested component");
    };
    assert!(matches!(
        nested.declarations[..],
        [
            ComponentDeclaration::CoreModule(ComponentCoreModule::OuterAlias { count: 1, index: 0 }),
            ComponentDeclaration::CoreModule(ComponentCoreModule::Defined(0))
        ]
    ));
}

#[test]
fn extracts_modules_from_nested_components() {
    let wasm = wat::parse_str(
        r#"(component
            (core module (func (export "first")))
            (component (core module (func (export "nested"))))
            (core module (func (export "last"))))"#,
    )
    .unwrap();
    let component = Parser::default().parse_component_bytes(&wasm).unwrap();
    assert_eq!(component.modules.len(), 2);
    for (module, name) in component.modules.iter().zip(["first", "last"]) {
        assert_eq!(module.exports[0].name.as_ref(), name);
    }
    let ComponentDeclaration::Component(ComponentIndex::Defined(nested)) = &component.declarations[1] else {
        panic!("expected nested component");
    };
    assert_eq!(nested.modules[0].exports[0].name.as_ref(), "nested");
}

#[test]
fn rejects_core_module() {
    let wasm = wat::parse_str("(module)").unwrap();
    assert!(matches!(
        Parser::default().parse_component_bytes(&wasm),
        Err(ParseError::InvalidEncoding(Encoding::Module))
    ));
}

#[test]
fn empty_component_has_no_modules() {
    let wasm = wat::parse_str("(component)").unwrap();
    assert!(Parser::default().parse_component_bytes(&wasm).unwrap().modules.is_empty());
}

#[test]
fn applies_parser_limits() {
    let wasm = wat::parse_str("(component (core module))").unwrap();
    let limits = ParseLimits::new().with_max_module_bytes(wasm.len() - 1);
    let parser = Parser::new(ParserOptions::new().with_limits(limits));
    assert!(matches!(
        parser.parse_component_bytes(&wasm),
        Err(ParseError::LimitExceeded { kind: ParseLimitKind::ModuleBytes, .. })
    ));
}

#[test]
fn limits_component_section_items() {
    let wasm = wat::parse_str("(component (type (func)) (type (func)))").unwrap();
    let parser = Parser::new(ParserOptions::new().with_limits(ParseLimits::new().with_max_section_items(1)));
    assert!(matches!(
        parser.parse_component_bytes(&wasm),
        Err(ParseError::LimitExceeded { kind: ParseLimitKind::SectionItems, limit: 1 })
    ));
    #[cfg(feature = "std")]
    assert!(matches!(
        parser.parse_component_stream(wasm.as_slice()),
        Err(ParseError::LimitExceeded { kind: ParseLimitKind::SectionItems, limit: 1 })
    ));

    let nested = wat::parse_str("(component (type (component (type (func)) (type (func)))))").unwrap();
    assert!(matches!(
        parser.parse_component_bytes(&nested),
        Err(ParseError::LimitExceeded { kind: ParseLimitKind::SectionItems, limit: 1 })
    ));

    let rec_groups =
        wat::parse_str("(component (core type (module (rec (type (func)) (type (func))) (type (func)))))").unwrap();
    let parser = Parser::new(ParserOptions::new().with_limits(ParseLimits::new().with_max_section_items(2)));
    assert!(matches!(
        parser.parse_component_bytes(&rec_groups),
        Err(ParseError::LimitExceeded { kind: ParseLimitKind::SectionItems, limit: 2 })
    ));

    let fields = wat::parse_str("(component (type (record (field \"a\" u8) (field \"b\" u8))))").unwrap();
    let parser = Parser::new(ParserOptions::new().with_limits(ParseLimits::new().with_max_section_items(1)));
    assert!(matches!(
        parser.parse_component_bytes(&fields),
        Err(ParseError::LimitExceeded { kind: ParseLimitKind::SectionItems, limit: 1 })
    ));
}

#[test]
fn rejects_invalid_embedded_module() {
    let mut wasm = wat::parse_str("(component (core module))").unwrap();
    let range = wasmparser::Parser::new(0)
        .parse_all(&wasm)
        .find_map(|payload| match payload.unwrap() {
            Payload::ModuleSection { unchecked_range, .. } => Some(unchecked_range),
            _ => None,
        })
        .unwrap();
    wasm[range.start as usize] = 0xff;
    assert!(Parser::default().parse_component_bytes(wasm).is_err());
}

#[cfg(feature = "validate")]
#[test]
fn rejects_invalid_embedded_function_body() {
    let mut wasm = wat::parse_str("(component (core module (func (result i32) i32.const 1)))").unwrap();
    let range = wasmparser::Parser::new(0)
        .parse_all(&wasm)
        .find_map(|payload| match payload.unwrap() {
            Payload::CodeSectionEntry(body) => Some(body.range()),
            _ => None,
        })
        .unwrap();
    let body = &mut wasm[range.start as usize..range.end as usize];
    let opcode = body.windows(3).position(|bytes| bytes == [0x41, 1, 0x0b]).unwrap();
    body[opcode] = 0x42; // i64.const is invalid for an i32 result.

    let parser = Parser::default();
    assert!(parser.parse_component_bytes(&wasm).is_err());
    #[cfg(feature = "std")]
    assert!(parser.parse_component_stream(wasm.as_slice()).is_err());
}

#[cfg(feature = "std")]
#[test]
fn parses_component_stream() {
    struct OneByteReads<'a>(&'a [u8]);

    impl std::io::Read for OneByteReads<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let len = buf.len().min(self.0.len()).min(1);
            buf[..len].copy_from_slice(&self.0[..len]);
            self.0 = &self.0[len..];
            Ok(len)
        }
    }

    let wasm = wat::parse_str(
        r#"(component
            (core module (func (export "first")))
            (component (core module (func (export "nested")))))"#,
    )
    .unwrap();
    let parser = Parser::default();
    let from_bytes = parser.parse_component_bytes(&wasm).unwrap();
    let from_stream = parser.parse_component_stream(OneByteReads(&wasm)).unwrap();
    assert!(from_bytes == from_stream);
    assert_eq!(from_stream.modules.len(), 1);
    assert!(parser.parse_component_stream(OneByteReads(&wasm[..wasm.len() - 1])).is_err());
}
