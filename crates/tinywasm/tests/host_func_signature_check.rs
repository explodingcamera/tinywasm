use std::fmt::Write;
use tinywasm::types::{ExternRef, FuncType, RefType, RefValue, WasmType, WasmValue};
use tinywasm::{FuncContext, HostFunction, Imports, Module, ModuleInstance, Store};

const VAL_LISTS: &[&[WasmValue]] = &[
    &[],
    &[WasmValue::I32(0)],
    &[WasmValue::I32(0), WasmValue::I32(0)],
    &[WasmValue::I32(0), WasmValue::I32(0), WasmValue::F64(0.0)],
    &[WasmValue::I32(0), WasmValue::F64(0.0), WasmValue::I32(0)],
    &[WasmValue::Ref(RefValue::Extern(ExternRef::new(0))), WasmValue::F64(0.0), WasmValue::I32(0)],
];

fn module_cases() -> Vec<(Module, FuncType, Vec<WasmValue>)> {
    let mut cases = Vec::<(Module, FuncType, Vec<WasmValue>)>::new();
    for results in VAL_LISTS {
        for params in VAL_LISTS {
            let param_tys = params.iter().map(|value| value.ty().expect("non-null fixture")).collect::<Vec<_>>();
            let result_tys = results.iter().map(|value| value.ty().expect("non-null fixture")).collect::<Vec<_>>();
            let func_ty = FuncType::new(&param_tys, &result_tys);
            cases.push((proxy_module(&func_ty), func_ty, params.to_vec()));
        }
    }
    cases
}

#[test]
fn test_return_invalid_type() -> Result<(), Box<dyn core::error::Error>> {
    let cases = module_cases();

    for (module, ty, args) in cases {
        for returned_values in VAL_LISTS {
            let mut store = Store::default();
            let mut imports = Imports::new();
            let hfn = HostFunction::from_untyped(&ty, |_: FuncContext<'_>, _| Ok(returned_values.to_vec()));
            imports.define("host", "hfn", hfn);

            let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports)).unwrap();
            let caller = instance.func_untyped(&store, "call_hfn").unwrap();
            // Return-type mismatch is only observable at call time.
            let should_succeed = returned_values.len() == ty.results().len()
                && returned_values.iter().zip(ty.results()).all(|(value, ty)| value.matches_type(*ty));
            let call_res = caller.call(&mut store, &args);
            assert_eq!(call_res.is_ok(), should_succeed);
        }
    }

    Ok(())
}

#[test]
fn test_linking_invalid_untyped_func() -> Result<(), Box<dyn core::error::Error>> {
    let cases = module_cases();
    for (module, expected_func_ty, _) in &cases {
        for (_, ty, _) in &cases {
            let mut store = Store::default();
            let tried_fn = HostFunction::from_untyped(ty, |_: FuncContext<'_>, _| panic!("not intended to be called"));
            let mut imports = Imports::new();
            imports.define("host", "hfn", tried_fn);

            let should_succeed = ty == expected_func_ty;
            let link_res = ModuleInstance::instantiate(&mut store, module, Some(&imports));
            assert_eq!(link_res.is_ok(), should_succeed);
        }
    }
    Ok(())
}

#[test]
fn test_linking_invalid_typed_func() -> Result<(), Box<dyn core::error::Error>> {
    type Existing = (i32, i32, f64);
    type NonMatchingSingle = f64;
    type NonMatchingTuple = (f64, i32, i32);
    const DONT_CALL: &str = "not meant to be called";

    let cases = module_cases();
    for (module, _, _) in cases {
        let mut store = Store::default();
        let matching_none = vec![
            HostFunction::from(|_, _: NonMatchingTuple| -> tinywasm::Result<Existing> { panic!("{DONT_CALL}") }),
            HostFunction::from(|_, _: NonMatchingTuple| -> tinywasm::Result<()> { panic!("{DONT_CALL}") }),
            HostFunction::from(|_, _: NonMatchingSingle| -> tinywasm::Result<Existing> { panic!("{DONT_CALL}") }),
            HostFunction::from(|_, _: NonMatchingSingle| -> tinywasm::Result<()> { panic!("{DONT_CALL}") }),
            HostFunction::from(|_, _: Existing| -> tinywasm::Result<NonMatchingTuple> { panic!("{DONT_CALL}") }),
            HostFunction::from(|_, _: Existing| -> tinywasm::Result<NonMatchingSingle> { panic!("{DONT_CALL}") }),
            HostFunction::from(|_, _: ()| -> tinywasm::Result<NonMatchingSingle> { panic!("{DONT_CALL}") }),
            HostFunction::from(|_, _: ()| -> tinywasm::Result<NonMatchingTuple> { panic!("{DONT_CALL}") }),
            HostFunction::from(|_, _: NonMatchingSingle| -> tinywasm::Result<NonMatchingTuple> {
                panic!("{DONT_CALL}")
            }),
            HostFunction::from(|_, _: NonMatchingSingle| -> tinywasm::Result<NonMatchingSingle> {
                panic!("{DONT_CALL}")
            }),
        ];

        for typed_fn in matching_none {
            let mut imports = Imports::new();
            imports.define("host", "hfn", typed_fn);
            let link_failure = ModuleInstance::instantiate(&mut store, &module, Some(&imports));
            assert!(link_failure.is_err(), "Expected linking to fail for mismatched typed func, but it succeeded");
        }
    }

    Ok(())
}

#[test]
fn concrete_host_references_use_canonical_types() -> Result<(), Box<dyn core::error::Error>> {
    let wasm = wat::parse_str(
        r#"
        (module
          (type $a (func))
          (type $b (func (param i32)))
          (type $takes-a (func (param (ref $a))))
          (type $returns-a (func (result (ref $a))))
          (elem declare func $a-func $b-func)
          (func $a-func (type $a))
          (func $b-func (type $b) local.get 0 drop)
          (func (export "right-ref") (type $returns-a) ref.func $a-func)
          (func (export "wrong-ref") (result (ref $b)) ref.func $b-func)
          (func (export "takes-a") (type $takes-a) unreachable))
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, None)?;

    let right_ref = instance.func_untyped(&store, "right-ref")?.call(&mut store, &[])?;
    let wrong_ref = instance.func_untyped(&store, "wrong-ref")?.call(&mut store, &[])?;
    let return_ty = instance.func_untyped(&store, "right-ref")?.ty(&store)?.clone();
    let param_ty = instance.func_untyped(&store, "takes-a")?.ty(&store)?.clone();

    let wrong_result = wrong_ref.clone();
    let wrong_return =
        HostFunction::from_untyped(&return_ty, move |_, _| Ok(wrong_result.clone())).instantiate(&mut store)?;
    assert!(wrong_return.call(&mut store, &[]).is_err());

    let accept_a = HostFunction::from_untyped(&param_ty, |_, _| Ok(Vec::new())).instantiate(&mut store)?;
    assert!(accept_a.call(&mut store, &wrong_ref).is_err());
    assert!(accept_a.call(&mut store, &right_ref).is_ok());

    Ok(())
}

#[test]
fn imported_host_functions_resolve_concrete_types() -> Result<(), Box<dyn core::error::Error>> {
    let wasm = wat::parse_str(
        r#"
        (module
          (type $object (struct))
          (import "host" "inspect" (func $inspect (param (ref null $object))))
          (func (export "call")
            ref.null $object
            call $inspect))
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let concrete = RefType::new_concrete(true, 0).expect("valid module-local type index");
    let host_ty = FuncType::new(&[WasmType::Ref(concrete)], &[]);
    let host = HostFunction::from_untyped(&host_ty, |_, args| {
        assert_eq!(args, &[WasmValue::Ref(RefValue::Null)]);
        Ok(Vec::new())
    });
    let mut imports = Imports::new();
    imports.define("host", "inspect", host);
    let mut store = Store::default();
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;

    instance.func::<(), ()>(&store, "call")?.call(&mut store, ())?;
    Ok(())
}

#[test]
fn host_tail_calls_return_from_the_current_frame() -> Result<(), Box<dyn core::error::Error>> {
    let wasm = wat::parse_str(
        r#"
        (module
          (type $t (func (result i32)))
          (import "host" "answer" (func $answer (type $t)))
          (elem declare func $answer)
          (table 1 funcref)
          (elem (i32.const 0) func $answer)
          (func $direct (export "direct") (type $t) return_call $answer)
          (func (export "indirect") (type $t)
            i32.const 0
            return_call_indirect (type $t))
          (func (export "reference") (type $t)
            ref.func $answer
            return_call_ref $t)
          (func (export "nested") (result i32)
            call $direct
            i32.const 1
            i32.add))
        "#,
    )?;
    let module = tinywasm::parse_bytes(&wasm)?;
    let mut store = Store::default();
    let mut imports = Imports::new();
    imports.define("host", "answer", HostFunction::from(|_, ()| Ok(42_i32)));
    let instance = ModuleInstance::instantiate(&mut store, &module, Some(&imports))?;

    for name in ["direct", "indirect", "reference"] {
        assert_eq!(instance.func::<(), i32>(&store, name)?.call(&mut store, ())?, 42);
    }
    assert_eq!(instance.func::<(), i32>(&store, "nested")?.call(&mut store, ())?, 43);

    Ok(())
}

#[test]
fn host_calls_reject_unknown_function_references() -> Result<(), Box<dyn core::error::Error>> {
    let mut store = Store::default();
    let ty = FuncType::new(&[WasmType::Ref(tinywasm::types::RefType::FUNCREF)], &[]);
    let host = HostFunction::from_untyped(&ty, |_, _| Ok(Vec::new())).instantiate(&mut store)?;
    let invalid = WasmValue::Ref(RefValue::Func(tinywasm::types::FuncRef::new(u32::MAX)));

    assert!(host.call(&mut store, &[invalid]).is_err());
    Ok(())
}

fn to_name(ty: &WasmType) -> &str {
    match ty {
        WasmType::I32 => "i32",
        WasmType::I64 => "i64",
        WasmType::F32 => "f32",
        WasmType::F64 => "f64",
        WasmType::V128 => "v128",
        WasmType::Ref(ty) if ty.is_func() => "funcref",
        WasmType::Ref(ty) if ty.is_extern() => "externref",
        WasmType::Ref(_) => panic!("unsupported reference type fixture"),
    }
}

fn proxy_module(func_ty: &FuncType) -> Module {
    let results = func_ty.results();
    let params = func_ty.params();
    let join_surround = |list: &[WasmType], keyword| {
        if list.is_empty() {
            return "".to_string();
        }
        let step = list.iter().map(|ty| format!("{} ", to_name(ty))).collect::<String>();
        format!("({keyword} {step})")
    };

    let results_text = join_surround(results, "result");
    let params_text = join_surround(params, "param");

    let params_gets: String = params.iter().enumerate().fold(String::new(), |mut acc, (num, _)| {
        let _ = writeln!(acc, "(local.get {num})");
        acc
    });

    let result_drops = "(drop)\n".repeat(results.len());
    let wasm_text = format!(
        r#"(module
        (import "host" "hfn" (func $host_fn {params_text} {results_text}))
        (func (export "call_hfn") {params_text} {results_text}
            {params_gets}
            (call $host_fn)
        )
        (func (export "call_hfn_discard") {params_text}
            {params_gets}
            (call $host_fn)
            ;; Keep stack balanced for arbitrary result arity.
            {result_drops}
        )
    )
    "#
    );
    let wasm = wat::parse_str(wasm_text).expect("failed to parse wat");
    tinywasm::parse_bytes(&wasm).expect("failed to make module")
}
