use crate::testsuite::util::*;
use std::{borrow::Cow, collections::HashMap};

use super::TestSuite;
use eyre::{eyre, Result};
use log::{debug, error, info};
use tinywasm::{Extern, Imports, ModuleInstance};
use tinywasm_types::{ExternVal, MemoryType, ModuleInstanceAddr, TableType, ValType, WasmValue};
use wast::{lexer::Lexer, parser::ParseBuffer, QuoteWat, Wast};

#[derive(Default)]
struct RegisteredModules {
    modules: HashMap<String, ModuleInstanceAddr>,

    named_modules: HashMap<String, ModuleInstanceAddr>,
    last_module: Option<ModuleInstanceAddr>,
}

impl RegisteredModules {
    fn modules(&self) -> &HashMap<String, ModuleInstanceAddr> {
        &self.modules
    }

    fn update_last_module(&mut self, addr: ModuleInstanceAddr, name: Option<String>) {
        self.last_module = Some(addr);
        if let Some(name) = name {
            self.named_modules.insert(name, addr);
        }
    }
    fn register(&mut self, name: String, addr: ModuleInstanceAddr) {
        log::debug!("registering module: {}", name);
        self.modules.insert(name.clone(), addr);

        self.last_module = Some(addr);
        self.named_modules.insert(name, addr);
    }

    fn get_idx(&self, module_id: Option<wast::token::Id<'_>>) -> Option<&ModuleInstanceAddr> {
        match module_id {
            Some(module) => {
                log::debug!("getting module: {}", module.name());

                if let Some(addr) = self.modules.get(module.name()) {
                    return Some(addr);
                }

                if let Some(addr) = self.named_modules.get(module.name()) {
                    return Some(addr);
                }

                None
            }
            None => self.last_module.as_ref(),
        }
    }

    fn get<'a>(
        &self,
        module_id: Option<wast::token::Id<'_>>,
        store: &'a tinywasm::Store,
    ) -> Option<&'a ModuleInstance> {
        let addr = self.get_idx(module_id)?;
        store.get_module_instance(*addr)
    }

    fn last<'a>(&self, store: &'a tinywasm::Store) -> Option<&'a ModuleInstance> {
        store.get_module_instance(*self.last_module.as_ref()?)
    }
}

impl TestSuite {
    pub fn run_paths(&mut self, tests: &[&str]) -> Result<()> {
        tests.iter().for_each(|group| {
            let group_wast = std::fs::read(group).expect("failed to read test wast");
            let group_wast = Cow::Owned(group_wast);
            self.run_group(group, group_wast).expect("failed to run group");
        });

        Ok(())
    }

    fn imports(modules: &HashMap<std::string::String, u32>) -> Result<Imports> {
        let mut imports = Imports::new();

        let table =
            Extern::table(TableType::new(ValType::RefFunc, 10, Some(20)), WasmValue::default_for(ValType::RefFunc));

        let print = Extern::typed_func(|_ctx: tinywasm::FuncContext, _: ()| {
            log::debug!("print");
            Ok(())
        });

        let print_i32 = Extern::typed_func(|_ctx: tinywasm::FuncContext, arg: i32| {
            log::debug!("print_i32: {}", arg);
            Ok(())
        });

        let print_i64 = Extern::typed_func(|_ctx: tinywasm::FuncContext, arg: i64| {
            log::debug!("print_i64: {}", arg);
            Ok(())
        });

        let print_f32 = Extern::typed_func(|_ctx: tinywasm::FuncContext, arg: f32| {
            log::debug!("print_f32: {}", arg);
            Ok(())
        });

        let print_f64 = Extern::typed_func(|_ctx: tinywasm::FuncContext, arg: f64| {
            log::debug!("print_f64: {}", arg);
            Ok(())
        });

        let print_i32_f32 = Extern::typed_func(|_ctx: tinywasm::FuncContext, args: (i32, f32)| {
            log::debug!("print_i32_f32: {}, {}", args.0, args.1);
            Ok(())
        });

        let print_f64_f64 = Extern::typed_func(|_ctx: tinywasm::FuncContext, args: (f64, f64)| {
            log::debug!("print_f64_f64: {}, {}", args.0, args.1);
            Ok(())
        });

        imports
            .define("spectest", "memory", Extern::memory(MemoryType::new_32(1, Some(2))))?
            .define("spectest", "table", table)?
            .define("spectest", "global_i32", Extern::global(WasmValue::I32(666), false))?
            .define("spectest", "global_i64", Extern::global(WasmValue::I64(666), false))?
            .define("spectest", "global_f32", Extern::global(WasmValue::F32(666.0), false))?
            .define("spectest", "global_f64", Extern::global(WasmValue::F64(666.0), false))?
            .define("spectest", "print", print)?
            .define("spectest", "print_i32", print_i32)?
            .define("spectest", "print_i64", print_i64)?
            .define("spectest", "print_f32", print_f32)?
            .define("spectest", "print_f64", print_f64)?
            .define("spectest", "print_i32_f32", print_i32_f32)?
            .define("spectest", "print_f64_f64", print_f64_f64)?;

        for (name, addr) in modules {
            log::debug!("registering module: {}", name);
            imports.link_module(&name, *addr)?;
        }

        Ok(imports)
    }

    pub fn run_spec_group(&mut self, tests: &[&str]) -> Result<()> {
        tests.iter().for_each(|group| {
            let group_wast = wasm_testsuite::get_test_wast(group).expect("failed to get test wast");
            if self.1.contains(&group.to_string()) {
                info!("skipping group: {}", group);
                self.test_group(&format!("{} (skipped)", group), group);
                return;
            }

            self.run_group(group, group_wast).expect("failed to run group");
        });

        Ok(())
    }

    pub fn run_group(&mut self, group_name: &str, group_wast: Cow<'_, [u8]>) -> Result<()> {
        let file_name = group_name.split('/').last().unwrap_or(group_name);
        let test_group = self.test_group(file_name, group_name);
        let wast = std::str::from_utf8(&group_wast).expect("failed to convert wast to utf8");

        let mut lexer = Lexer::new(wast);
        // we need to allow confusing unicode characters since they are technically valid wasm
        lexer.allow_confusing_unicode(true);

        let buf = ParseBuffer::new_with_lexer(lexer).expect("failed to create parse buffer");
        let wast_data = wast::parser::parse::<Wast>(&buf).expect("failed to parse wat");

        let mut store = tinywasm::Store::default();
        let mut registered_modules = RegisteredModules::default();

        println!("running {} tests for group: {}", wast_data.directives.len(), group_name);
        for (i, directive) in wast_data.directives.into_iter().enumerate() {
            let span = directive.span();
            use wast::WastDirective::*;

            match directive {
                Register { span, name, .. } => {
                    let Some(last) = registered_modules.last(&store) else {
                        test_group.add_result(
                            &format!("Register({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("no module to register")),
                        );
                        continue;
                    };
                    registered_modules.register(name.to_string(), last.id());
                    test_group.add_result(&format!("Register({})", i), span.linecol_in(wast), Ok(()));
                }

                Wat(module) => {
                    debug!("got wat module");
                    let result = catch_unwind_silent(|| {
                        let (name, bytes) = match module {
                            QuoteWat::QuoteModule(_, quoted_wat) => {
                                let wat = quoted_wat
                                    .iter()
                                    .map(|(_, s)| std::str::from_utf8(&s).expect("failed to convert wast to utf8"))
                                    .collect::<Vec<_>>()
                                    .join("\n");

                                let lexer = Lexer::new(&wat);
                                let buf = ParseBuffer::new_with_lexer(lexer).expect("failed to create parse buffer");
                                let mut wat_data = wast::parser::parse::<wast::Wat>(&buf).expect("failed to parse wat");
                                (None, wat_data.encode().expect("failed to encode module"))
                            }
                            QuoteWat::Wat(mut wat) => {
                                let wast::Wat::Module(ref module) = wat else {
                                    unimplemented!("Not supported");
                                };
                                (
                                    module.id.map(|id| id.name().to_string()),
                                    wat.encode().expect("failed to encode module"),
                                )
                            }
                            _ => unimplemented!("Not supported"),
                        };

                        let m = parse_module_bytes(&bytes).expect("failed to parse module bytes");

                        let module_instance = tinywasm::Module::from(m)
                            .instantiate(&mut store, Some(Self::imports(registered_modules.modules()).unwrap()))
                            .expect("failed to instantiate module");

                        (name, module_instance)
                    })
                    .map_err(|e| eyre!("failed to parse wat module: {:?}", try_downcast_panic(e)));

                    match &result {
                        Err(err) => debug!("failed to parse module: {:?}", err),
                        Ok((name, module)) => registered_modules.update_last_module(module.id(), name.clone()),
                    };

                    test_group.add_result(&format!("Wat({})", i), span.linecol_in(wast), result.map(|_| ()));
                }

                AssertMalformed { span, mut module, message: _ } => {
                    let Ok(module) = module.encode() else {
                        test_group.add_result(&format!("AssertMalformed({})", i), span.linecol_in(wast), Ok(()));
                        continue;
                    };

                    let res = catch_unwind_silent(|| parse_module_bytes(&module))
                        .map_err(|e| eyre!("failed to parse module (expected): {:?}", try_downcast_panic(e)))
                        .and_then(|res| res);

                    test_group.add_result(
                        &format!("AssertMalformed({})", i),
                        span.linecol_in(wast),
                        match res {
                            Ok(_) => Err(eyre!("expected module to be malformed")),
                            Err(_) => Ok(()),
                        },
                    );
                }

                AssertInvalid { span, mut module, message: _ } => {
                    let res = catch_unwind_silent(move || parse_module_bytes(&module.encode().unwrap()))
                        .map_err(|e| eyre!("failed to parse module (invalid): {:?}", try_downcast_panic(e)))
                        .and_then(|res| res);

                    test_group.add_result(
                        &format!("AssertInvalid({})", i),
                        span.linecol_in(wast),
                        match res {
                            Ok(_) => Err(eyre!("expected module to be invalid")),
                            Err(_) => Ok(()),
                        },
                    );
                }

                AssertExhaustion { call, message, span } => {
                    let module = registered_modules.get_idx(call.module);
                    let args = convert_wastargs(call.args).expect("failed to convert args");
                    let res =
                        catch_unwind_silent(|| exec_fn_instance(module, &mut store, call.name, &args).map(|_| ()));

                    let Ok(Err(tinywasm::Error::Trap(trap))) = res else {
                        test_group.add_result(
                            &format!("AssertExhaustion({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("expected trap")),
                        );
                        continue;
                    };

                    if trap.message() != message {
                        test_group.add_result(
                            &format!("AssertExhaustion({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("expected trap: {}, got: {}", message, trap.message())),
                        );
                        continue;
                    }

                    test_group.add_result(&format!("AssertExhaustion({})", i), span.linecol_in(wast), Ok(()));
                }

                AssertTrap { exec, message, span } => {
                    let res: Result<tinywasm::Result<()>, _> = catch_unwind_silent(|| {
                        let invoke = match exec {
                            wast::WastExecute::Wat(mut wat) => {
                                let module = parse_module_bytes(&wat.encode().expect("failed to encode module"))
                                    .expect("failed to parse module");
                                let module = tinywasm::Module::from(module);
                                module.instantiate(
                                    &mut store,
                                    Some(Self::imports(registered_modules.modules()).unwrap()),
                                )?;
                                return Ok(());
                            }
                            wast::WastExecute::Get { module: _, global: _ } => {
                                panic!("get not supported");
                            }
                            wast::WastExecute::Invoke(invoke) => invoke,
                        };

                        let module = registered_modules.get_idx(invoke.module);
                        let args = convert_wastargs(invoke.args).expect("failed to convert args");
                        exec_fn_instance(module, &mut store, invoke.name, &args).map(|_| ())
                    });

                    match res {
                        Err(err) => test_group.add_result(
                            &format!("AssertTrap({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("test panicked: {:?}", try_downcast_panic(err))),
                        ),
                        Ok(Err(tinywasm::Error::Trap(trap))) => {
                            if trap.message() != message {
                                test_group.add_result(
                                    &format!("AssertTrap({})", i),
                                    span.linecol_in(wast),
                                    Err(eyre!("expected trap: {}, got: {}", message, trap.message())),
                                );
                                continue;
                            }

                            test_group.add_result(&format!("AssertTrap({})", i), span.linecol_in(wast), Ok(()))
                        }
                        Ok(Err(err)) => test_group.add_result(
                            &format!("AssertTrap({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("expected trap, {}, got: {:?}", message, err)),
                        ),
                        Ok(Ok(())) => test_group.add_result(
                            &format!("AssertTrap({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("expected trap {}, got Ok", message)),
                        ),
                    }
                }

                AssertUnlinkable { mut module, span, message } => {
                    let res = catch_unwind_silent(|| {
                        let module = parse_module_bytes(&module.encode().expect("failed to encode module"))
                            .expect("failed to parse module");
                        let module = tinywasm::Module::from(module);
                        module.instantiate(&mut store, Some(Self::imports(registered_modules.modules()).unwrap()))
                    });

                    match res {
                        Err(err) => test_group.add_result(
                            &format!("AssertUnlinkable({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("test panicked: {:?}", try_downcast_panic(err))),
                        ),
                        Ok(Err(tinywasm::Error::Linker(err))) => {
                            if err.message() != message {
                                test_group.add_result(
                                    &format!("AssertUnlinkable({})", i),
                                    span.linecol_in(wast),
                                    Err(eyre!("expected linker error: {}, got: {}", message, err.message())),
                                );
                                continue;
                            }

                            test_group.add_result(&format!("AssertUnlinkable({})", i), span.linecol_in(wast), Ok(()))
                        }
                        Ok(Err(err)) => test_group.add_result(
                            &format!("AssertUnlinkable({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("expected linker error, {}, got: {:?}", message, err)),
                        ),
                        Ok(Ok(_)) => test_group.add_result(
                            &format!("AssertUnlinkable({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("expected linker error {}, got Ok", message)),
                        ),
                    }
                }

                Invoke(invoke) => {
                    let name = invoke.name;

                    let res: Result<Result<()>, _> = catch_unwind_silent(|| {
                        let args = convert_wastargs(invoke.args)?;
                        let module = registered_modules.get_idx(invoke.module);
                        exec_fn_instance(module, &mut store, invoke.name, &args).map_err(|e| {
                            error!("failed to execute function: {:?}", e);
                            e
                        })?;
                        Ok(())
                    });

                    let res = res.map_err(|e| eyre!("test panicked: {:?}", try_downcast_panic(e))).and_then(|r| r);
                    test_group.add_result(&format!("Invoke({}-{})", name, i), span.linecol_in(wast), res);
                }

                AssertReturn { span, exec, results } => {
                    info!("AssertReturn: {:?}", exec);
                    let expected = convert_wastret(results)?;

                    let invoke = match match exec {
                        wast::WastExecute::Wat(_) => Err(eyre!("wat not supported")),
                        wast::WastExecute::Get { module: module_id, global } => {
                            let module = registered_modules.get(module_id, &store);
                            let Some(module) = module else {
                                test_group.add_result(
                                    &format!("AssertReturn(unsupported-{})", i),
                                    span.linecol_in(wast),
                                    Err(eyre!("no module to get global from")),
                                );
                                continue;
                            };

                            let module_global = match match module.export_addr(global) {
                                Some(ExternVal::Global(addr)) => {
                                    store.get_global_val(addr as usize).map_err(|_| eyre!("failed to get global"))
                                }
                                _ => Err(eyre!("no module to get global from")),
                            } {
                                Ok(module_global) => module_global,
                                Err(err) => {
                                    test_group.add_result(
                                        &format!("AssertReturn(unsupported-{})", i),
                                        span.linecol_in(wast),
                                        Err(eyre!("failed to get global: {:?}", err)),
                                    );
                                    continue;
                                }
                            };
                            let expected = expected.get(0).expect("expected global value");
                            let module_global = module_global.attach_type(expected.val_type());

                            if !module_global.eq_loose(expected) {
                                test_group.add_result(
                                    &format!("AssertReturn(unsupported-{})", i),
                                    span.linecol_in(wast),
                                    Err(eyre!("global value did not match: {:?} != {:?}", module_global, expected)),
                                );
                                continue;
                            }

                            test_group.add_result(
                                &format!("AssertReturn({}-{})", global, i),
                                span.linecol_in(wast),
                                Ok(()),
                            );

                            continue;
                            // check if module_global matches the expected results
                        }
                        wast::WastExecute::Invoke(invoke) => Ok(invoke),
                    } {
                        Ok(invoke) => invoke,
                        Err(err) => {
                            test_group.add_result(
                                &format!("AssertReturn(unsupported-{})", i),
                                span.linecol_in(wast),
                                Err(eyre!("unsupported directive: {:?}", err)),
                            );
                            continue;
                        }
                    };

                    let invoke_name = invoke.name;
                    let res: Result<Result<()>, _> = catch_unwind_silent(|| {
                        debug!("invoke: {:?}", invoke);
                        let args = convert_wastargs(invoke.args)?;
                        let module = registered_modules.get_idx(invoke.module);
                        let outcomes = exec_fn_instance(module, &mut store, invoke.name, &args).map_err(|e| {
                            error!("failed to execute function: {:?}", e);
                            e
                        })?;

                        debug!("outcomes: {:?}", outcomes);

                        debug!("expected: {:?}", expected);

                        if outcomes.len() != expected.len() {
                            return Err(eyre!(
                                "span: {:?} expected {} results, got {}",
                                span,
                                expected.len(),
                                outcomes.len()
                            ));
                        }

                        log::debug!("outcomes: {:?}", outcomes);

                        outcomes.iter().zip(expected).enumerate().try_for_each(|(i, (outcome, exp))| {
                            (outcome.eq_loose(&exp))
                                .then_some(())
                                .ok_or_else(|| eyre!(" result {} did not match: {:?} != {:?}", i, outcome, exp))
                        })
                    });

                    let res = res.map_err(|e| eyre!("test panicked: {:?}", try_downcast_panic(e))).and_then(|r| r);

                    test_group.add_result(&format!("AssertReturn({}-{})", invoke_name, i), span.linecol_in(wast), res);
                }
                _ => test_group.add_result(
                    &format!("Unknown({})", i),
                    span.linecol_in(wast),
                    Err(eyre!("unsupported directive")),
                ),
            }
        }

        Ok(())
    }
}
