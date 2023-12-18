use crate::testsuite::util::*;

use super::TestSuite;
use eyre::{eyre, Result};
use log::debug;
use tinywasm_types::TinyWasmModule;
use wast::{lexer::Lexer, parser::ParseBuffer, QuoteWat, Wast};

impl TestSuite {
    pub fn run(&mut self, tests: &[&str]) -> Result<()> {
        tests.iter().for_each(|group| {
            let test_group = self.test_group(group);

            let wast = wasm_testsuite::get_test_wast(group).expect("failed to get test wast");
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

                        println!("result: {:?}", result);

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
                            println!("malformed module: {:?}", module);
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

                    AssertReturn { span, exec, results } => {
                        let Some(module) = last_module.as_ref() else {
                            println!("no module found for assert_return: {:?}", exec);
                            test_group.add_result(&format!("{}-return", name), span, Err(eyre!("no module found")));
                            continue;
                        };

                        let res: Result<Result<()>, _> = catch_unwind_silent(|| {
                            let mut store = tinywasm::Store::new();
                            let module = tinywasm::Module::from(module);
                            let instance = module.instantiate(&mut store)?;

                            use wast::WastExecute::*;
                            let invoke = match exec {
                                Wat(_) => return Result::Ok(()), // not used by the testsuite
                                Get { module: _, global: _ } => return Result::Ok(()),
                                Invoke(invoke) => invoke,
                            };

                            let args = invoke
                                .args
                                .into_iter()
                                .map(wastarg2tinywasmvalue)
                                .collect::<Result<Vec<_>>>()?;
                            let res = instance.get_func(&store, invoke.name)?.call(&mut store, &args)?;
                            let expected = results
                                .into_iter()
                                .map(wastret2tinywasmvalue)
                                .collect::<Result<Vec<_>>>()?;

                            if res.len() != expected.len() {
                                return Result::Err(eyre!("expected {} results, got {}", expected.len(), res.len()));
                            }

                            for (i, (res, expected)) in res.iter().zip(expected).enumerate() {
                                if res != &expected {
                                    return Result::Err(eyre!(
                                        "result {} did not match: {:?} != {:?}",
                                        i,
                                        res,
                                        expected
                                    ));
                                }
                            }

                            Ok(())
                        });

                        let res = match res {
                            Err(e) => Err(eyre!("test panicked: {:?}", e)),
                            Ok(Err(e)) => Err(e),
                            Ok(Ok(())) => Ok(()),
                        };

                        test_group.add_result(&format!("{}-return", name), span, res);
                    }
                    Invoke(m) => {
                        debug!("invoke: {:?}", m);
                    }
                    _ => test_group.add_result(&format!("{}-unknown", name), span, Err(eyre!("unsupported directive"))),
                }
            }
        });

        Ok(())
    }
}
