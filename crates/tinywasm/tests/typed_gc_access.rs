use tinywasm::types::{ArrayRef, RefValue, StructRef, WasmValue};
use tinywasm::{GcStorageType, ModuleInstance, Store};

#[test]
fn typed_struct_and_array_access() -> tinywasm::Result<()> {
    let wasm = wat::parse_str(
        r#"(module
            (type $s (struct (field (mut i32)) (field i8)))
            (type $refs (struct (field (ref $s)) (field (ref null $s))))
            (type $a (array (mut i16)))
            (func $make-struct (result (ref $s))
              i32.const 7
              i32.const 258
              struct.new $s)
            (func (export "struct") (result (ref struct))
              call $make-struct)
            (func (export "refs") (result (ref struct))
              call $make-struct
              ref.null $s
              struct.new $refs)
            (func (export "array") (result (ref array))
              i32.const 65537
              i32.const 2
              array.new $a))"#,
    )
    .expect("valid WAT");
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &tinywasm::parse_bytes(&wasm)?, None)?;

    let structure: StructRef = instance.func::<(), StructRef>(&store, "struct")?.call(&mut store, ())?;
    assert_eq!(structure.fields(&mut store)?, [WasmValue::I32(7), WasmValue::I32(2)]);
    structure.set_field(&mut store, 0, WasmValue::I32(9))?;
    assert_eq!(structure.field(&mut store, 0)?, WasmValue::I32(9));
    assert_eq!(structure.ty(&store)?.field(&store, 1)?.storage(), GcStorageType::I8);

    let references: StructRef = instance.func::<(), StructRef>(&store, "refs")?.call(&mut store, ())?;
    let fields = references.fields(&mut store)?;
    let WasmValue::Ref(RefValue::Any(nested)) = &fields[0] else { panic!("expected struct reference") };
    assert_eq!(fields[1], WasmValue::Ref(RefValue::Null));
    store.gc()?;
    assert_eq!(nested.as_struct().unwrap().field(&mut store, 0)?, WasmValue::I32(7));

    let array: ArrayRef = instance.func::<(), ArrayRef>(&store, "array")?.call(&mut store, ())?;
    assert_eq!(array.len(&store)?, 2);
    assert_eq!(array.get(&mut store, 0)?, WasmValue::I32(1));
    array.set(&mut store, 1, WasmValue::I32(3))?;
    assert_eq!(array.get(&mut store, 1)?, WasmValue::I32(3));
    Ok(())
}
