use crate::testsuite::util::*;
use std::borrow::Cow;

use super::TestSuite;
use eyre::{eyre, Result};
use tinywasm_types::TinyWasmModule;
use wast::{lexer::Lexer, parser::ParseBuffer, Wast};

impl TestSuite {
    pub fn run(&mut self, tests: &[&str]) -> Result<()> {
        tests.iter().for_each(|group| {
            let test_group = self.test_group(group);

            let wast = if group.starts_with("./") {
                let file = std::fs::read(group).expect("failed to read test wast");
                Cow::Owned(file)
            } else {
                wasm_testsuite::get_test_wast(group).expect("failed to get test wast")
            };

            let wast = std::str::from_utf8(&wast).expect("failed to convert wast to utf8");

            let mut lexer = Lexer::new(wast);
            // we need to allow confusing unicode characters since they are technically valid wasm
            lexer.allow_confusing_unicode(true);

            let buf = ParseBuffer::new_with_lexer(lexer).expect("failed to create parse buffer");
            let wast_data = wast::parser::parse::<Wast>(&buf).expect("failed to parse wat");

            let mut last_module: Option<TinyWasmModule> = None;
            for (i, directive) in wast_data.directives.into_iter().enumerate() {
                let span = directive.span();
                use wast::WastDirective::*;
                let name = format!("{}-{}", group, i);

                match directive {
                    // TODO: needs to support more binary sections
                    Wat(mut module) => {
                        let result = catch_unwind_silent(move || parse_module_bytes(&module.encode().unwrap()))
                            .map_err(|e| eyre!("failed to parse module: {:?}", e))
                            .and_then(|res| res);

                        match &result {
                            Err(_) => last_module = None,
                            Ok(m) => last_module = Some(m.clone()),
                        }

                        test_group.add_result(&format!("{}-parse", name), span, result.map(|_| ()));
                    }

                    AssertMalformed {
                        span,
                        mut module,
                        message: _,
                    } => {
                        let Ok(module) = module.encode() else {
                            test_group.add_result(&format!("{}-malformed", name), span, Ok(()));
                            continue;
                        };

                        let res = catch_unwind_silent(|| parse_module_bytes(&module))
                            .map_err(|e| eyre!("failed to parse module: {:?}", e))
                            .and_then(|res| res);

                        test_group.add_result(
                            &format!("{}-malformed", name),
                            span,
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
                            .map_err(|e| eyre!("failed to parse module: {:?}", e))
                            .and_then(|res| res);

                        test_group.add_result(
                            &format!("{}-invalid", name),
                            span,
                            match res {
                                Ok(_) => Err(eyre!("expected module to be invalid")),
                                Err(_) => Ok(()),
                            },
                        );
                    }

                    AssertTrap { exec, message: _, span } => {
                        let res: Result<tinywasm::Result<()>, _> = catch_unwind_silent(|| {
                            let (module, name) = match exec {
                                wast::WastExecute::Wat(_wat) => unimplemented!("wat"),
                                wast::WastExecute::Get { module: _, global: _ } => unimplemented!("get"),
                                wast::WastExecute::Invoke(invoke) => (last_module.as_ref(), invoke.name),
                            };
                            exec_fn(module, name, &[]).map(|_| ())
                        });

                        match res {
                            Err(err) => test_group.add_result(
                                &format!("{}-trap", name),
                                span,
                                Err(eyre!("test panicked: {:?}", err)),
                            ),
                            Ok(Err(tinywasm::Error::Trap(_))) => {
                                test_group.add_result(&format!("{}-trap", name), span, Ok(()))
                            }
                            Ok(Err(err)) => test_group.add_result(
                                &format!("{}-trap", name),
                                span,
                                Err(eyre!("expected trap, got error: {:?}", err)),
                            ),
                            Ok(Ok(())) => test_group.add_result(
                                &format!("{}-trap", name),
                                span,
                                Err(eyre!("expected trap, got ok")),
                            ),
                        }
                    }

                    AssertReturn { span, exec, results } => {
                        let res: Result<Result<()>, _> = catch_unwind_silent(|| {
                            let invoke = match exec {
                                wast::WastExecute::Wat(_) => unimplemented!("wat"),
                                wast::WastExecute::Get { module: _, global: _ } => {
                                    return Err(eyre!("get not supported"))
                                }
                                wast::WastExecute::Invoke(invoke) => invoke,
                            };

                            let args = invoke
                                .args
                                .into_iter()
                                .map(wastarg2tinywasmvalue)
                                .collect::<Result<Vec<_>>>()?;

                            let outcomes = exec_fn(last_module.as_ref(), invoke.name, &args)?;
                            let expected = results
                                .into_iter()
                                .map(wastret2tinywasmvalue)
                                .collect::<Result<Vec<_>>>()?;

                            if outcomes.len() != expected.len() {
                                return Err(eyre!("expected {} results, got {}", expected.len(), outcomes.len()));
                            }
                            outcomes
                                .iter()
                                .zip(expected)
                                .enumerate()
                                .try_for_each(|(i, (outcome, exp))| {
                                    (outcome == &exp)
                                        .then_some(())
                                        .ok_or_else(|| eyre!("result {} did not match: {:?} != {:?}", i, outcome, exp))
                                })
                        });

                        let res = res.map_err(|e| eyre!("test panicked: {:?}", e)).and_then(|r| r);
                        test_group.add_result(&format!("{}-return", name), span, res);
                    }
                    _ => test_group.add_result(&format!("{}-unknown", name), span, Err(eyre!("unsupported directive"))),
                }
            }
        });

        Ok(())
    }
}
