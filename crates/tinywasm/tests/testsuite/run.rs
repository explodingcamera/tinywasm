use crate::testsuite::util::*;
use std::{
    borrow::Cow,
    panic::{catch_unwind, AssertUnwindSafe},
};

use super::TestSuite;
use eyre::{eyre, Result};
use log::{debug, error, info};
use tinywasm::{Extern, Imports, ModuleInstance};
use tinywasm_types::{MemoryType, ModuleInstanceAddr, TableType, ValType, WasmValue};
use wast::{lexer::Lexer, parser::ParseBuffer, Wast};

impl TestSuite {
    pub fn run_paths(&mut self, tests: &[&str]) -> Result<()> {
        tests.iter().for_each(|group| {
            let group_wast = std::fs::read(group).expect("failed to read test wast");
            let group_wast = Cow::Owned(group_wast);
            self.run_group(group, group_wast).expect("failed to run group");
        });

        Ok(())
    }

    fn imports(registered_modules: Vec<(String, ModuleInstanceAddr)>) -> Result<Imports> {
        let mut imports = Imports::new();

        let memory = Extern::memory(MemoryType::new_32(1, Some(2)));
        let table = Extern::table(
            TableType::new(ValType::FuncRef, 10, Some(20)),
            WasmValue::default_for(ValType::FuncRef),
        );

        imports
            .define("spectest", "memory", memory)?
            .define("spectest", "table", table)?
            .define("spectest", "global_i32", Extern::global(WasmValue::I32(666), false))?
            .define("spectest", "global_i64", Extern::global(WasmValue::I64(666), false))?
            .define("spectest", "global_f32", Extern::global(WasmValue::F32(666.0), false))?
            .define("spectest", "global_f64", Extern::global(WasmValue::F64(666.0), false))?;

        for (name, addr) in registered_modules {
            imports.link_module(&name, addr)?;
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
        let mut registered_modules = Vec::new();
        let mut last_module: Option<ModuleInstance> = None;

        println!("running {} tests for group: {}", wast_data.directives.len(), group_name);
        for (i, directive) in wast_data.directives.into_iter().enumerate() {
            let span = directive.span();
            use wast::WastDirective::*;

            match directive {
                Register { span, name, .. } => {
                    let Some(last) = &last_module else {
                        test_group.add_result(
                            &format!("Register({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("no module to register")),
                        );
                        continue;
                    };

                    registered_modules.push((name.to_string(), last.id()));
                    test_group.add_result(&format!("Register({})", i), span.linecol_in(wast), Ok(()));
                }

                Wat(mut module) => {
                    // TODO: modules are not properly isolated from each other - tests fail because of this otherwise
                    store = tinywasm::Store::default();
                    debug!("got wat module");
                    let result = catch_unwind_silent(|| {
                        let m = parse_module_bytes(&module.encode().expect("failed to encode module"))
                            .expect("failed to parse module bytes");
                        tinywasm::Module::from(m)
                            .instantiate(&mut store, Some(Self::imports(registered_modules.clone()).unwrap()))
                            .map_err(|e| {
                                println!("failed to instantiate module: {:?}", e);
                                e
                            })
                            .expect("failed to instantiate module")
                    })
                    .map_err(|e| eyre!("failed to parse wat module: {:?}", try_downcast_panic(e)));

                    match &result {
                        Err(_) => last_module = None,
                        Ok(m) => last_module = Some(m.clone()),
                    }

                    if let Err(err) = &result {
                        debug!("failed to parse module: {:?}", err)
                    }

                    test_group.add_result(&format!("Wat({})", i), span.linecol_in(wast), result.map(|_| ()));
                }

                AssertMalformed {
                    span,
                    mut module,
                    message: _,
                } => {
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

                AssertInvalid {
                    span,
                    mut module,
                    message: _,
                } => {
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

                AssertTrap { exec, message: _, span } => {
                    let res: Result<tinywasm::Result<()>, _> = catch_unwind_silent(|| {
                        let (module, name, args) = match exec {
                            wast::WastExecute::Wat(mut wat) => {
                                let module = parse_module_bytes(&wat.encode().expect("failed to encode module"))
                                    .expect("failed to parse module");
                                let module = tinywasm::Module::from(module);
                                module.instantiate(
                                    &mut store,
                                    Some(Self::imports(registered_modules.clone()).unwrap()),
                                )?;
                                return Ok(());
                            }
                            wast::WastExecute::Get { module: _, global: _ } => {
                                panic!("get not supported");
                            }
                            wast::WastExecute::Invoke(invoke) => (last_module.as_ref(), invoke.name, invoke.args),
                        };
                        let args = args
                            .into_iter()
                            .map(wastarg2tinywasmvalue)
                            .collect::<Result<Vec<_>>>()
                            .expect("failed to convert args");

                        exec_fn_instance(module, &mut store, name, &args).map(|_| ())
                    });

                    match res {
                        Err(err) => test_group.add_result(
                            &format!("AssertTrap({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("test panicked: {:?}", try_downcast_panic(err))),
                        ),
                        Ok(Err(tinywasm::Error::Trap(_))) => {
                            test_group.add_result(&format!("AssertTrap({})", i), span.linecol_in(wast), Ok(()))
                        }
                        Ok(Err(err)) => test_group.add_result(
                            &format!("AssertTrap({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("expected trap, got error: {:?}", err,)),
                        ),
                        Ok(Ok(())) => test_group.add_result(
                            &format!("AssertTrap({})", i),
                            span.linecol_in(wast),
                            Err(eyre!("expected trap, got ok")),
                        ),
                    }
                }

                Invoke(invoke) => {
                    let name = invoke.name;
                    let res: Result<Result<()>, _> = catch_unwind_silent(|| {
                        let args = invoke
                            .args
                            .into_iter()
                            .map(wastarg2tinywasmvalue)
                            .collect::<Result<Vec<_>>>()
                            .map_err(|e| {
                                error!("failed to convert args: {:?}", e);
                                e
                            })?;

                        exec_fn_instance(last_module.as_ref(), &mut store, invoke.name, &args).map_err(|e| {
                            error!("failed to execute function: {:?}", e);
                            e
                        })?;
                        Ok(())
                    });

                    let res = res
                        .map_err(|e| eyre!("test panicked: {:?}", try_downcast_panic(e)))
                        .and_then(|r| r);

                    test_group.add_result(&format!("Invoke({}-{})", name, i), span.linecol_in(wast), res);
                }

                AssertReturn { span, exec, results } => {
                    info!("AssertReturn: {:?}", exec);
                    let invoke = match match exec {
                        wast::WastExecute::Wat(_) => Err(eyre!("wat not supported")),
                        wast::WastExecute::Get { module: _, global: _ } => Err(eyre!("get not supported")),
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
                        let args = invoke
                            .args
                            .into_iter()
                            .map(wastarg2tinywasmvalue)
                            .collect::<Result<Vec<_>>>()
                            .map_err(|e| {
                                error!("failed to convert args: {:?}", e);
                                e
                            })?;

                        let outcomes =
                            exec_fn_instance(last_module.as_ref(), &mut store, invoke.name, &args).map_err(|e| {
                                error!("failed to execute function: {:?}", e);
                                e
                            })?;

                        debug!("outcomes: {:?}", outcomes);

                        let expected = results
                            .into_iter()
                            .map(wastret2tinywasmvalue)
                            .collect::<Result<Vec<_>>>()
                            .map_err(|e| {
                                error!("failed to convert expected results: {:?}", e);
                                e
                            })?;

                        debug!("expected: {:?}", expected);

                        if outcomes.len() != expected.len() {
                            return Err(eyre!(
                                "span: {:?} expected {} results, got {}",
                                span,
                                expected.len(),
                                outcomes.len()
                            ));
                        }

                        outcomes
                            .iter()
                            .zip(expected)
                            .enumerate()
                            .try_for_each(|(i, (outcome, exp))| {
                                (outcome.eq_loose(&exp))
                                    .then_some(())
                                    .ok_or_else(|| eyre!(" result {} did not match: {:?} != {:?}", i, outcome, exp))
                            })
                    });

                    let res = res
                        .map_err(|e| eyre!("test panicked: {:?}", try_downcast_panic(e)))
                        .and_then(|r| r);

                    test_group.add_result(
                        &format!("AssertReturn({}-{})", invoke_name, i),
                        span.linecol_in(wast),
                        res,
                    );
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
