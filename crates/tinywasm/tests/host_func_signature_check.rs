use eyre::Result;
use std::fmt::Write;
use tinywasm::{
    FuncContext, HostFunction, Imports, Module, Store,
    types::{FuncType, WasmType, WasmValue},
};
use tinywasm_types::ExternRef;

const VAL_LISTS: &[&[WasmValue]] = &[
    &[],
    &[WasmValue::I32(0)],
    &[WasmValue::I32(0), WasmValue::I32(0)],
    &[WasmValue::I32(0), WasmValue::I32(0), WasmValue::F64(0.0)],
    &[WasmValue::I32(0), WasmValue::F64(0.0), WasmValue::I32(0)],
    &[WasmValue::RefExtern(ExternRef::null()), WasmValue::F64(0.0), WasmValue::I32(0)],
];

fn module_cases() -> Vec<(Module, FuncType, Vec<WasmValue>)> {
    let mut cases = Vec::<(Module, FuncType, Vec<WasmValue>)>::new();
    for results in VAL_LISTS {
        for params in VAL_LISTS {
            let param_tys = params.iter().map(WasmType::from).collect::<Vec<_>>();
            let result_tys = results.iter().map(WasmType::from).collect::<Vec<_>>();
            let func_ty = FuncType::new(&param_tys, &result_tys);
            cases.push((proxy_module(&func_ty), func_ty, params.to_vec()));
        }
    }
    cases
}

#[test]
fn test_return_invalid_type() -> Result<()> {
    let cases = module_cases();

    for (module, ty, args) in cases {
        for returned_values in VAL_LISTS {
            let mut store = Store::default();
            let mut imports = Imports::new();
            let hfn = HostFunction::from_untyped(&mut store, &ty, |_: FuncContext<'_>, _| Ok(returned_values.to_vec()));
            imports.define("host", "hfn", hfn);

            let instance = module.clone().instantiate(&mut store, Some(imports)).unwrap();
            let caller = instance.func_untyped(&store, "call_hfn").unwrap();
            // Return-type mismatch is only observable at call time.
            let should_succeed = returned_values.iter().map(WasmType::from).eq(ty.results().iter().copied());
            let call_res = caller.call(&mut store, &args);
            assert_eq!(call_res.is_ok(), should_succeed);
        }
    }

    Ok(())
}

#[test]
fn test_linking_invalid_untyped_func() -> Result<()> {
    let cases = module_cases();
    for (module, expected_func_ty, _) in &cases {
        for (_, ty, _) in &cases {
            let mut store = Store::default();
            let tried_fn =
                HostFunction::from_untyped(&mut store, ty, |_: FuncContext<'_>, _| panic!("not intended to be called"));
            let mut imports = Imports::new();
            imports.define("host", "hfn", tried_fn);

            let should_succeed = ty == expected_func_ty;
            let link_res = module.clone().instantiate(&mut store, Some(imports));
            assert_eq!(link_res.is_ok(), should_succeed);
        }
    }
    Ok(())
}

#[test]
fn test_linking_invalid_typed_func() -> Result<()> {
    type Existing = (i32, i32, f64);
    type NonMatchingSingle = f64;
    type NonMatchingTuple = (f64, i32, i32);
    const DONT_CALL: &str = "not meant to be called";

    let cases = module_cases();
    for (module, _, _) in cases {
        let mut store = Store::default();
        let matching_none = vec![
            HostFunction::from(&mut store, |_, _: NonMatchingTuple| -> tinywasm::Result<Existing> {
                panic!("{DONT_CALL}")
            }),
            HostFunction::from(&mut store, |_, _: NonMatchingTuple| -> tinywasm::Result<()> { panic!("{DONT_CALL}") }),
            HostFunction::from(&mut store, |_, _: NonMatchingSingle| -> tinywasm::Result<Existing> {
                panic!("{DONT_CALL}")
            }),
            HostFunction::from(&mut store, |_, _: NonMatchingSingle| -> tinywasm::Result<()> { panic!("{DONT_CALL}") }),
            HostFunction::from(&mut store, |_, _: Existing| -> tinywasm::Result<NonMatchingTuple> {
                panic!("{DONT_CALL}")
            }),
            HostFunction::from(&mut store, |_, _: Existing| -> tinywasm::Result<NonMatchingSingle> {
                panic!("{DONT_CALL}")
            }),
            HostFunction::from(&mut store, |_, _: ()| -> tinywasm::Result<NonMatchingSingle> { panic!("{DONT_CALL}") }),
            HostFunction::from(&mut store, |_, _: ()| -> tinywasm::Result<NonMatchingTuple> { panic!("{DONT_CALL}") }),
            HostFunction::from(&mut store, |_, _: NonMatchingSingle| -> tinywasm::Result<NonMatchingTuple> {
                panic!("{DONT_CALL}")
            }),
            HostFunction::from(&mut store, |_, _: NonMatchingSingle| -> tinywasm::Result<NonMatchingSingle> {
                panic!("{DONT_CALL}")
            }),
        ];

        for typed_fn in matching_none {
            let mut imports = Imports::new();
            imports.define("host", "hfn", typed_fn);
            let link_failure = module.clone().instantiate(&mut store, Some(imports));
            assert!(link_failure.is_err(), "Expected linking to fail for mismatched typed func, but it succeeded");
        }
    }

    Ok(())
}

fn to_name(ty: &WasmType) -> &str {
    match ty {
        WasmType::I32 => "i32",
        WasmType::I64 => "i64",
        WasmType::F32 => "f32",
        WasmType::F64 => "f64",
        WasmType::V128 => "v128",
        WasmType::RefFunc => "funcref",
        WasmType::RefExtern => "externref",
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
    Module::parse_bytes(&wasm).expect("failed to make module")
}
