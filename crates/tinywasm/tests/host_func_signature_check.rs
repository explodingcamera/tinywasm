use eyre::Result;
use std::fmt::Write;
use tinywasm::{
    Extern, FuncContext, Imports, Module, Store,
    types::{FuncType, ValType, WasmValue},
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

fn value_types(values: &[WasmValue]) -> Box<[ValType]> {
    values.iter().map(WasmValue::val_type).collect()
}

fn module_cases() -> Vec<(Module, FuncType, Vec<WasmValue>)> {
    let mut cases = Vec::<(Module, FuncType, Vec<WasmValue>)>::new();
    for results in VAL_LISTS {
        for params in VAL_LISTS {
            let func_ty = FuncType { results: value_types(results), params: value_types(params) };
            cases.push((proxy_module(&func_ty), func_ty, params.to_vec()));
        }
    }
    cases
}

#[test]
fn test_return_invalid_type() -> Result<()> {
    let cases = module_cases();

    for (module, func_ty, args) in cases {
        for returned_values in VAL_LISTS {
            let mut store = Store::default();
            let mut imports = Imports::new();
            imports
                .define("host", "hfn", Extern::func(&func_ty, |_: FuncContext<'_>, _| Ok(returned_values.to_vec())))
                .unwrap();

            let instance = module.clone().instantiate(&mut store, Some(imports)).unwrap();
            let caller = instance.func(&store, "call_hfn").unwrap();
            // Return-type mismatch is only observable at call time.
            let should_succeed = returned_values.iter().map(WasmValue::val_type).eq(func_ty.results.iter().copied());
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
        for (_, func_ty_to_try, _) in &cases {
            let tried_fn = Extern::func(func_ty_to_try, |_: FuncContext<'_>, _| panic!("not intended to be called"));
            let mut store = Store::default();
            let mut imports = Imports::new();
            imports.define("host", "hfn", tried_fn).unwrap();

            let should_succeed = func_ty_to_try == expected_func_ty;
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

    // None of these typed host signatures are produced by module_cases().
    let matching_none = vec![
        Extern::typed_func(|_, _: NonMatchingTuple| -> tinywasm::Result<Existing> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: NonMatchingTuple| -> tinywasm::Result<()> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: NonMatchingSingle| -> tinywasm::Result<Existing> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: NonMatchingSingle| -> tinywasm::Result<()> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: Existing| -> tinywasm::Result<NonMatchingTuple> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: Existing| -> tinywasm::Result<NonMatchingSingle> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: ()| -> tinywasm::Result<NonMatchingSingle> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: ()| -> tinywasm::Result<NonMatchingTuple> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: NonMatchingSingle| -> tinywasm::Result<NonMatchingTuple> { panic!("{DONT_CALL}") }),
        Extern::typed_func(|_, _: NonMatchingSingle| -> tinywasm::Result<NonMatchingSingle> { panic!("{DONT_CALL}") }),
    ];

    let cases = module_cases();
    for (module, _, _) in cases {
        for typed_fn in matching_none.iter().cloned() {
            let mut store = Store::default();
            let mut imports = Imports::new();
            imports.define("host", "hfn", typed_fn).unwrap();
            let link_failure = module.clone().instantiate(&mut store, Some(imports));
            assert!(link_failure.is_err(), "Expected linking to fail for mismatched typed func, but it succeeded");
        }
    }

    Ok(())
}

fn to_name(ty: &ValType) -> &str {
    match ty {
        ValType::I32 => "i32",
        ValType::I64 => "i64",
        ValType::F32 => "f32",
        ValType::F64 => "f64",
        ValType::V128 => "v128",
        ValType::RefFunc => "funcref",
        ValType::RefExtern => "externref",
    }
}

fn proxy_module(func_ty: &FuncType) -> Module {
    let results = func_ty.results.as_ref();
    let params = func_ty.params.as_ref();
    let join_surround = |list: &[ValType], keyword| {
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
