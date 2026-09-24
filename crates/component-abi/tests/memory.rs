use tinywasm::types::{MemoryArch, MemoryType};
use tinywasm::{FuncContext, HostFunction, Imports, Memory, ModuleInstance, Store};
use tinywasm_component_abi::{AbiError, CANONICAL_MAX_BYTES, CanonicalMemory, Limits};

const LIMITS: Limits = Limits { max_transfer_bytes: 64 };

fn memory() -> (Store, CanonicalMemory) {
    let mut store = Store::default();
    let handle = Memory::try_new(&mut store, MemoryType::default().with_page_count_initial(1)).unwrap();
    let abi = CanonicalMemory::new(handle, &store, LIMITS).unwrap();
    (store, abi)
}

#[test]
fn borrows_and_writes_without_allocating() {
    let (mut store, abi) = memory();
    abi.write_bytes(&mut store, 7, b"hello").unwrap();
    assert_eq!(abi.bytes(&store, 7, 5).unwrap(), b"hello");
    assert_eq!(abi.utf8(&store, 7, 5).unwrap(), "hello");
    abi.write_bytes(&mut store, 16, &[1, 0, 0, 0, 0xfe, 0xff, 0xff, 0xff]).unwrap();
    assert_eq!(abi.u32_list(&store, 16, 2).unwrap().collect::<Vec<_>>(), [1, 0xffff_fffe]);
    assert_eq!(abi.u32_list(&store, 16, 0).unwrap().len(), 0);
}

#[test]
fn rejects_pathological_lengths_pointers_alignment_and_encoding() {
    let (mut store, abi) = memory();
    assert!(matches!(abi.bytes(&store, 0, 65), Err(AbiError::TooLarge)));
    assert!(matches!(abi.bytes(&store, 0, (CANONICAL_MAX_BYTES + 1) as u32), Err(AbiError::TooLarge)));
    assert!(matches!(abi.u32_list(&store, 0, 17), Err(AbiError::TooLarge)));
    assert!(matches!(abi.bytes(&store, u32::MAX, u32::MAX), Err(AbiError::TooLarge)));
    assert!(matches!(abi.bytes(&store, u32::MAX, 4), Err(AbiError::OutOfBounds)));
    assert!(matches!(abi.bytes(&store, 65536, 1), Err(AbiError::OutOfBounds)));
    assert!(abi.bytes(&store, 65536, 0).unwrap().is_empty());
    assert!(matches!(abi.u32_list(&store, 1, 1), Err(AbiError::Misaligned)));
    assert!(matches!(abi.u32_list(&store, 65536, 1), Err(AbiError::OutOfBounds)));
    assert!(matches!(abi.write_bytes(&mut store, 65535, b"xx"), Err(AbiError::OutOfBounds)));
    assert!(matches!(abi.write_bytes(&mut store, 0, &[0; 65]), Err(AbiError::TooLarge)));
    abi.write_bytes(&mut store, 0, &[0xff]).unwrap();
    assert!(matches!(abi.utf8(&store, 0, 1), Err(AbiError::InvalidUtf8)));
}

#[test]
fn arbitrary_guest_ranges_never_escape_memory() {
    let (store, abi) = memory();
    let mut random = 0x9e37_79b9_u32;
    for i in 0..20_000 {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        let ptr = if i % 2 == 0 { random % 65_540 } else { random };
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        let len = if i % 2 == 0 { random % 70 } else { random };

        let expected = usize::try_from(len).unwrap() <= LIMITS.max_transfer_bytes
            && (ptr as usize).checked_add(len as usize).is_some_and(|end| end <= 65536);
        assert_eq!(abi.bytes(&store, ptr, len).is_ok(), expected, "ptr={ptr} len={len}");
    }
}

#[test]
fn rejects_memory64_and_foreign_store_handles() {
    let mut store = Store::default();
    let memory64 =
        Memory::try_new(&mut store, MemoryType::default().with_arch(MemoryArch::I64).with_page_count_initial(1))
            .unwrap();
    assert!(matches!(CanonicalMemory::new(memory64, &store, LIMITS), Err(AbiError::Memory64)));

    let (other_store, _) = memory();
    assert!(matches!(CanonicalMemory::new(memory64, &other_store, LIMITS), Err(AbiError::Runtime(_))));
}

#[test]
fn custom_wit_shaped_import_uses_borrowed_guest_data() -> tinywasm::Result<()> {
    let wasm = wat::parse_str(
        r#"(module
            (import "snqr:example/bytes@0.1.0" "checksum" (func $checksum (param i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 8) "\01\02\03\04")
            (func (export "run") (result i32)
                i32.const 8
                i32.const 4
                call $checksum))"#,
    )
    .unwrap();
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut imports = Imports::new();
    imports.define(
        "snqr:example/bytes@0.1.0",
        "checksum",
        HostFunction::from(|ctx: FuncContext<'_>, (ptr, len): (i32, i32)| -> tinywasm::Result<i32> {
            let abi = CanonicalMemory::new(ctx.memory("memory")?, ctx.store(), LIMITS)?;
            let bytes = abi.bytes(ctx.store(), ptr as u32, len as u32)?;
            Ok(bytes.iter().map(|&byte| i32::from(byte)).sum())
        }),
    );
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;
    assert_eq!(instance.func::<(), i32>(&store, "run")?.call(&mut store, ())?, 10);
    Ok(())
}
