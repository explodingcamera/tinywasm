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
    &[WasmValue::I32(0), WasmValue::I32(0)],                      // 2 of the same
    &[WasmValue::I32(0), WasmValue::I32(0), WasmValue::F64(0.0)], // add another type
    &[WasmValue::I32(0), WasmValue::F64(0.0), WasmValue::I32(0)], // reorder
    &[WasmValue::RefExtern(ExternRef::null()), WasmValue::F64(0.0), WasmValue::I32(0)], // all different types
];
// (f64, i32, i32) and (f64) can be used to "match_none"

fn get_type_lists() -> impl Iterator<Item = impl Iterator<Item = ValType> + Clone> + Clone {
    VAL_LISTS.iter().map(|l| l.iter().map(WasmValue::val_type))
}
fn get_modules() -> Vec<(Module, FuncType, Vec<WasmValue>)> {
    let mut result = Vec::<(Module, FuncType, Vec<WasmValue>)>::new();
    let val_and_tys = get_type_lists().zip(VAL_LISTS);
    for res_types in get_type_lists() {
        for (arg_types, arg_vals) in val_and_tys.clone() {
            let ty = FuncType { results: res_types.clone().collect(), params: arg_types.collect() };
            result.push((proxy_module(&ty), ty, arg_vals.to_vec()));
        }
    }
    result
}

#[test]
fn test_return_invalid_type() -> Result<()> {
    // try to return from host functions types that don't match their signatures
    let mod_list = get_modules();

    for (module, func_ty, test_args) in mod_list {
        for result_to_try in VAL_LISTS {
            let mut store = Store::default();
            let mut imports = Imports::new();
            imports
                .define("host", "hfn", Extern::func(&func_ty, |_: FuncContext<'_>, _| Ok(result_to_try.to_vec())))
                .unwrap();

            let instance = module.clone().instantiate(&mut store, Some(imports)).unwrap();
            let caller = instance.exported_func_untyped(&store, "call_hfn").unwrap();
            let res_types_returned = result_to_try.iter().map(WasmValue::val_type);
            dbg!(&res_types_returned, &func_ty);
            let res_types_expected = &func_ty.results;
            let should_succeed = res_types_returned.eq(res_types_expected.iter().cloned());
            // Extern::func that returns wrong type(s) can only be detected when it runs
            let call_res = caller.call(&mut store, &test_args);
            dbg!(&call_res);
            assert_eq!(call_res.is_ok(), should_succeed);
            println!("this time ok");
        }
    }
    Ok(())
}

#[test]
fn test_linking_invalid_untyped_func() -> Result<()> {
    // try to import host functions with function types no matching those expected by modules
    let mod_list = get_modules();
    for (module, actual_func_ty, _) in &mod_list {
        for (_, func_ty_to_try, _) in &mod_list {
            let tried_fn = Extern::func(func_ty_to_try, |_: FuncContext<'_>, _| panic!("not intended to be called"));
            let mut store = Store::default();
            let mut imports = Imports::new();
            imports.define("host", "hfn", tried_fn).unwrap();

            let should_succeed = func_ty_to_try == actual_func_ty;
            let link_res = module.clone().instantiate(&mut store, Some(imports));

            assert_eq!(link_res.is_ok(), should_succeed);
        }
    }
    Ok(())
}

#[test]
fn test_linking_invalid_typed_func() -> Result<()> {
    type Existing = (i32, i32, f64);
    type NonMatchingOne = f64;
    type NonMatchingMul = (f64, i32, i32);
    const DONT_CALL: &str = "not meant to be called";

    // they don't match any signature from get_modules()
    #[rustfmt::skip] // to make it table-like
    let matching_none= &[
        Extern::typed_func(|_, _: NonMatchingMul| -> tinywasm::Result<Existing>       { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: NonMatchingMul| -> tinywasm::Result<()>             { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: NonMatchingOne| -> tinywasm::Result<Existing>       { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: NonMatchingOne| -> tinywasm::Result<()>             { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: Existing      | -> tinywasm::Result<NonMatchingMul> { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: Existing      | -> tinywasm::Result<NonMatchingOne> { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: ()            | -> tinywasm::Result<NonMatchingOne> { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: ()            | -> tinywasm::Result<NonMatchingMul> { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: NonMatchingOne| -> tinywasm::Result<NonMatchingMul> { panic!("{DONT_CALL}") } ),
        Extern::typed_func(|_, _: NonMatchingOne| -> tinywasm::Result<NonMatchingOne> { panic!("{DONT_CALL}") } ),
    ];

    let mod_list = get_modules();
    for (module, _, _) in mod_list {
        for typed_fn in matching_none.clone() {
            let mut store = Store::default();
            let mut imports = Imports::new();
            imports.define("host", "hfn", typed_fn).unwrap();
            let link_failure = module.clone().instantiate(&mut store, Some(imports));
            link_failure.expect_err("no func in matching_none list should link to any mod");
        }
    }

    // the valid cases are well-checked in other tests
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

// make a module with imported function {module:"host", name:"hfn"} that takes specified results and returns specified params
// and 2 wasm functions: call_hfn takes params, passes them to hfn and returns it's results
// and 2 wasm functions: call_hfn_discard takes params, passes them to hfn and drops it's results
fn proxy_module(func_ty: &FuncType) -> Module {
    let results = func_ty.results.as_ref();
    let params = func_ty.params.as_ref();
    let join_surround = |list: &[ValType], keyword| {
        if list.is_empty() {
            return "".to_string();
        }
        let step = list.iter().map(|ty| format!("{} ", to_name(ty)).to_string()).collect::<String>();
        format!("({keyword} {step})")
    };

    let results_text = join_surround(results, "result");
    let params_text = join_surround(params, "param");

    let params_gets: String = params.iter().enumerate().fold(String::new(), |mut acc, (num, _)| {
        let _ = writeln!(acc, "(local.get {num})");
        acc
    });

    let result_drops = "(drop)\n".repeat(results.len()).to_string();
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
            {result_drops}
        )
    )
    "#
    );
    let wasm = wat::parse_str(wasm_text).expect("failed to parse wat");
    Module::parse_bytes(&wasm).expect("failed to make module")
}
