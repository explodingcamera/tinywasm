use tinywasm::{ExecProgress, Imports, ModuleInstance, Result, Store};

fn instantiate(store: &mut Store, source: &str, imports: Option<&Imports>) -> Result<ModuleInstance> {
    let module = tinywasm::parse_bytes(&wat::parse_str(source).unwrap())?;
    ModuleInstance::instantiate(store, &module, imports)
}

fn check_result(store: &mut Store, instance: &ModuleInstance, name: &str, expected: i32) -> Result<()> {
    let function = instance.func::<(), i32>(store, name)?;
    assert_eq!(function.call(store, ())?, expected);
    let mut execution = function.call_resumable(store, ())?;
    loop {
        match execution.resume_with_fuel(2)? {
            ExecProgress::Completed(result) => {
                assert_eq!(result, expected);
                return Ok(());
            }
            ExecProgress::Suspended => {}
        }
    }
}

#[test]
fn calls_and_tail_calls_restore_the_callers_memory() -> Result<()> {
    let mut store = Store::default();
    let leaf = instantiate(
        &mut store,
        r#"(module
        (memory 1) (data (i32.const 0) "\2a\00\00\00")
        (func (export "read") (result i32) i32.const 0 i32.load))"#,
        None,
    )?;
    let mut imports = Imports::new();
    imports.link_module("leaf", leaf)?;
    let middle = instantiate(
        &mut store,
        r#"(module
        (import "leaf" "read" (func $read (result i32)))
        (memory 1) (data (i32.const 0) "\07\00\00\00")
        (func (export "normal") (result i32) call $read i32.const 0 i32.load i32.add)
        (func (export "tail") (result i32) return_call $read))"#,
        Some(&imports),
    )?;
    imports.link_module("middle", middle)?;
    let root = instantiate(
        &mut store,
        r#"(module
        (import "middle" "normal" (func $normal (result i32)))
        (import "middle" "tail" (func $tail (result i32)))
        (memory 1) (data (i32.const 0) "\03\00\00\00")
        (func (export "normal") (result i32) call $normal i32.const 0 i32.load i32.add)
        (func (export "tail") (result i32) call $tail i32.const 0 i32.load i32.add))"#,
        Some(&imports),
    )?;
    check_result(&mut store, &root, "normal", 52)?;
    check_result(&mut store, &root, "tail", 45)
}

#[test]
fn exception_landing_pads_restore_the_callers_memory() -> Result<()> {
    let mut store = Store::default();
    let leaf = instantiate(
        &mut store,
        r#"(module
        (tag (export "tag"))
        (memory 1) (data (i32.const 0) "\2a\00\00\00")
        (func (export "throw") throw 0))"#,
        None,
    )?;
    let mut imports = Imports::new();
    imports.link_module("leaf", leaf)?;
    let middle = instantiate(
        &mut store,
        r#"(module
        (import "leaf" "throw" (func $throw))
        (func (export "throw") return_call $throw))"#,
        Some(&imports),
    )?;
    imports.link_module("middle", middle)?;
    let root = instantiate(
        &mut store,
        r#"(module
        (import "leaf" "tag" (tag $tag))
        (import "leaf" "throw" (func $normal))
        (import "middle" "throw" (func $tail))
        (memory 1) (data (i32.const 0) "\07\00\00\00")
        (func (export "normal") (result i32)
          (block $caught (try_table (catch $tag $caught) call $normal))
          i32.const 0 i32.load)
        (func (export "tail") (result i32)
          (block $caught (try_table (catch $tag $caught) call $tail))
          i32.const 0 i32.load))"#,
        Some(&imports),
    )?;
    check_result(&mut store, &root, "normal", 7)?;
    check_result(&mut store, &root, "tail", 7)
}
