//! Calls, returns and exceptions that move between module instances. The interpreter runs one
//! instance's frames at a time and hands over to the other instance at each crossing.

use tinywasm::{ExecProgress, Imports, ModuleInstance, Store};

type TestResult = Result<(), Box<dyn core::error::Error>>;

const MODULE_A: &str = r#"
    (module
      (type $i2i (func (param i32) (result i32)))
      (tag $e (export "e") (param i32))
      (table $t (export "t") 4 funcref)
      (func $add1 (export "add1") (param i32) (result i32)
        (i32.add (local.get 0) (i32.const 1)))
      (func (export "throw") (param i32)
        (throw $e (local.get 0)))
      ;; Calls table slot 0, which the other instance fills.
      (func (export "apply") (param i32) (result i32)
        (i32.add (call_indirect $t (type $i2i) (local.get 0) (i32.const 0)) (i32.const 100)))
      ;; Same, catching `$e` from the callee.
      (func (export "apply_catching") (param i32) (result i32)
        (block $caught (result i32)
          (try_table (result i32) (catch $e $caught)
            (call_indirect $t (type $i2i) (local.get 0) (i32.const 0)))
          (return))
        (i32.add (i32.const 1000)))
      (elem (table $t) (i32.const 1) func $add1))
"#;

const MODULE_B: &str = r#"
    (module
      (type $i2i (func (param i32) (result i32)))
      (import "a" "e" (tag $e (param i32)))
      (import "a" "t" (table $t 4 funcref))
      (import "a" "add1" (func $add1 (param i32) (result i32)))
      (import "a" "throw" (func $throw (param i32)))
      (import "a" "apply" (func $apply (param i32) (result i32)))
      (import "a" "apply_catching" (func $apply_catching (param i32) (result i32)))
      (func $double (param i32) (result i32)
        (i32.mul (local.get 0) (i32.const 2)))
      (func $throws (param i32) (result i32)
        (throw $e (local.get 0)))
      (elem declare func $double $throws)

      (func (export "direct") (param i32) (result i32)
        (call $add1 (call $add1 (local.get 0))))
      (func (export "tail") (param i32) (result i32)
        (return_call $add1 (local.get 0)))
      (func (export "indirect") (param i32) (result i32)
        (call_indirect $t (type $i2i) (local.get 0) (i32.const 1)))
      ;; B -> A -> B -> A -> B
      (func (export "callback") (param i32) (result i32)
        (table.set $t (i32.const 0) (ref.func $double))
        (call $apply (local.get 0)))
      (func (export "catch_from_import") (param i32) (result i32)
        (block $caught (result i32)
          (try_table (catch $e $caught)
            (call $throw (local.get 0)))
          (i32.const -1)))
      ;; Thrown in B, unwinds through A, caught in B.
      (func (export "catch_through_import") (param i32) (result i32)
        (table.set $t (i32.const 0) (ref.func $throws))
        (block $caught (result i32)
          (try_table (result i32) (catch $e $caught)
            (call $apply (local.get 0)))
          (return))
        (i32.add (i32.const 2000)))
      ;; Thrown in B, caught in A.
      (func (export "caught_in_import") (param i32) (result i32)
        (table.set $t (i32.const 0) (ref.func $throws))
        (call $apply_catching (local.get 0)))
      ;; Four crossings per iteration.
      (func (export "crossings") (param $n i32) (result i32) (local $sum i32)
        (table.set $t (i32.const 0) (ref.func $double))
        (loop $next
          (local.set $sum (i32.add (local.get $sum) (call $apply (local.get $n))))
          (local.set $n (i32.sub (local.get $n) (i32.const 1)))
          (br_if $next (local.get $n)))
        (local.get $sum)))
"#;

fn instantiate(store: &mut Store) -> Result<ModuleInstance, Box<dyn core::error::Error>> {
    let a = tinywasm::parse_bytes(&wat::parse_str(MODULE_A)?)?;
    let b = tinywasm::parse_bytes(&wat::parse_str(MODULE_B)?)?;
    let a = ModuleInstance::instantiate(store, &a, None)?;
    let mut imports = Imports::new();
    imports.link_module("a", a)?;
    Ok(ModuleInstance::instantiate(store, &b, Some(&imports))?)
}

fn call(
    store: &mut Store,
    instance: &ModuleInstance,
    name: &str,
    arg: i32,
) -> Result<i32, Box<dyn core::error::Error>> {
    Ok(instance.func::<i32, i32>(store, name)?.call(store, arg)?)
}

#[test]
fn calls_and_returns_cross_instances() -> TestResult {
    let mut store = Store::default();
    let b = instantiate(&mut store)?;
    assert_eq!(call(&mut store, &b, "direct", 5)?, 7);
    assert_eq!(call(&mut store, &b, "tail", 5)?, 6);
    assert_eq!(call(&mut store, &b, "indirect", 5)?, 6);
    assert_eq!(call(&mut store, &b, "callback", 5)?, 110);
    Ok(())
}

#[test]
fn exceptions_cross_instances() -> TestResult {
    let mut store = Store::default();
    let b = instantiate(&mut store)?;
    assert_eq!(call(&mut store, &b, "catch_from_import", 7)?, 7);
    assert_eq!(call(&mut store, &b, "catch_through_import", 9)?, 2009);
    assert_eq!(call(&mut store, &b, "caught_in_import", 9)?, 1009);
    // The instances keep working after each unwind.
    assert_eq!(call(&mut store, &b, "callback", 5)?, 110);
    Ok(())
}

#[test]
fn budgeted_runs_cross_instances() -> TestResult {
    const N: i32 = 1000;
    const EXPECTED: i32 = N * (N + 1) + 100 * N;

    let mut store = Store::default();
    let b = instantiate(&mut store)?;
    assert_eq!(call(&mut store, &b, "crossings", N)?, EXPECTED);

    for fuel in [1, 3, 64, 1000] {
        let mut store = Store::default();
        let b = instantiate(&mut store)?;
        let func = b.func::<i32, i32>(&store, "crossings")?;
        let mut exec = func.call_resumable(&mut store, N)?;
        let mut suspensions = 0;
        let result = loop {
            match exec.resume_with_fuel(fuel)? {
                ExecProgress::Completed(value) => break value,
                ExecProgress::Suspended => suspensions += 1,
            }
        };
        assert_eq!(result, EXPECTED, "fuel {fuel}");
        assert!(suspensions > 0, "fuel {fuel}");
    }

    #[cfg(feature = "std")]
    {
        let mut store = Store::default();
        let b = instantiate(&mut store)?;
        let func = b.func::<i32, i32>(&store, "crossings")?;
        let mut exec = func.call_resumable(&mut store, N)?;
        let result = loop {
            match exec.resume_with_time_budget(std::time::Duration::from_micros(10))? {
                ExecProgress::Completed(value) => break value,
                ExecProgress::Suspended => {}
            }
        };
        assert_eq!(result, EXPECTED);

        // An effectively unlimited budget completes in one resume.
        let mut store = Store::default();
        let b = instantiate(&mut store)?;
        let func = b.func::<i32, i32>(&store, "crossings")?;
        let mut exec = func.call_resumable(&mut store, N)?;
        match exec.resume_with_time_budget(std::time::Duration::MAX)? {
            ExecProgress::Completed(value) => assert_eq!(value, EXPECTED),
            ExecProgress::Suspended => panic!("suspended with an unlimited time budget"),
        }
    }
    Ok(())
}
