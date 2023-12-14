use std::{
    collections::BTreeMap,
    fmt::{Debug, Formatter},
};

use eyre::{eyre, Result};
use log::debug;
use tinywasm_types::TinyWasmModule;
use wast::{
    lexer::Lexer,
    parser::{self, ParseBuffer},
    QuoteWat, Wast,
};

fn parse_module(mut module: wast::core::Module) -> Result<TinyWasmModule> {
    let parser = tinywasm_parser::Parser::new();
    Ok(parser.parse_module_bytes(module.encode().expect("failed to encode module"))?)
}

#[test]
#[ignore]
fn test_mvp() -> Result<()> {
    let mut test_suite = TestSuite::new();

    wasm_testsuite::MVP_TESTS.iter().for_each(|group| {
        let test_group = test_suite.test_group(group);

        let wast = wasm_testsuite::get_test_wast(group).expect("failed to get test wast");
        let wast = std::str::from_utf8(&wast).expect("failed to convert wast to utf8");

        let mut lexer = Lexer::new(wast);
        // we need to allow confusing unicode characters since they are technically valid wasm
        lexer.allow_confusing_unicode(true);

        let buf = ParseBuffer::new_with_lexer(lexer).expect("failed to create parse buffer");
        let wast_data = parser::parse::<Wast>(&buf).expect("failed to parse wat");

        let mut last_module: Option<TinyWasmModule> = None;
        for (i, directive) in wast_data.directives.into_iter().enumerate() {
            let span = directive.span();
            use wast::WastDirective::*;
            let name = format!("{}-{}", group, i);

            match directive {
                // TODO: needs to support more binary sections
                Wat(QuoteWat::Wat(wast::Wat::Module(module))) => {
                    let result = std::panic::catch_unwind(|| parse_module(module))
                        .map_err(|e| eyre!("failed to parse module: {:?}", e))
                        .and_then(|res| res);

                    match &result {
                        Err(_) => last_module = None,
                        Ok(m) => last_module = Some(m.clone()),
                    }

                    test_group.add_result(&format!("{}-parse", name), span, result.map(|_| ()));
                }

                // these all pass already :)
                AssertMalformed {
                    span,
                    module: QuoteWat::Wat(wast::Wat::Module(module)),
                    message: _,
                } => {
                    let res = std::panic::catch_unwind(|| parse_module(module).map(|_| ()));
                    test_group.add_result(
                        &format!("{}-malformed", name),
                        span,
                        match res {
                            Ok(Ok(_)) => Err(eyre!("expected module to be malformed")),
                            Err(_) | Ok(Err(_)) => Ok(()),
                        },
                    );
                }
                AssertReturn { span, exec, results } => {
                    let Some(module) = last_module.as_ref() else {
                        // println!("no module found for assert_return: {:?}", exec);
                        continue;
                    };

                    let res: Result<Result<()>, _> = std::panic::catch_unwind(|| {
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
                                return Result::Err(eyre!("result {} did not match: {:?} != {:?}", i, res, expected));
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
                // _ => test_group.add_result(
                //     &format!("{}-unknown", name),
                //     span,
                //     Err(Error::Other("test not implemented".to_string())),
                // ),
                // TODO: implement more test directives
                _ => {}
            }
        }
    });

    if test_suite.failed() {
        eprintln!("\n\nfailed one or more tests:\n{:#?}", test_suite);
        Err(eyre!("failed one or more tests"))
    } else {
        println!("\n\npassed all tests:\n{:#?}", test_suite);
        Ok(())
    }
}

fn wastarg2tinywasmvalue(arg: wast::WastArg) -> Result<tinywasm_types::WasmValue> {
    let wast::WastArg::Core(arg) = arg else {
        return Err(eyre!("unsupported arg type"));
    };

    use tinywasm_types::WasmValue;
    use wast::core::WastArgCore::*;
    Ok(match arg {
        F32(f) => WasmValue::F32(f32::from_bits(f.bits)),
        F64(f) => WasmValue::F64(f64::from_bits(f.bits)),
        I32(i) => WasmValue::I32(i),
        I64(i) => WasmValue::I64(i),
        _ => return Err(eyre!("unsupported arg type")),
    })
}

fn wastret2tinywasmvalue(arg: wast::WastRet) -> Result<tinywasm_types::WasmValue> {
    let wast::WastRet::Core(arg) = arg else {
        return Err(eyre!("unsupported arg type"));
    };

    use tinywasm_types::WasmValue;
    use wast::core::WastRetCore::*;
    Ok(match arg {
        F32(f) => nanpattern2tinywasmvalue(f)?,
        F64(f) => nanpattern2tinywasmvalue(f)?,
        I32(i) => WasmValue::I32(i),
        I64(i) => WasmValue::I64(i),
        _ => return Err(eyre!("unsupported arg type")),
    })
}

enum Bits {
    U32(u32),
    U64(u64),
}
trait FloatToken {
    fn bits(&self) -> Bits;
}
impl FloatToken for wast::token::Float32 {
    fn bits(&self) -> Bits {
        Bits::U32(self.bits)
    }
}
impl FloatToken for wast::token::Float64 {
    fn bits(&self) -> Bits {
        Bits::U64(self.bits)
    }
}

fn nanpattern2tinywasmvalue<T>(arg: wast::core::NanPattern<T>) -> Result<tinywasm_types::WasmValue>
where
    T: FloatToken,
{
    use wast::core::NanPattern::*;
    Ok(match arg {
        CanonicalNan => tinywasm_types::WasmValue::F32(f32::NAN),
        ArithmeticNan => tinywasm_types::WasmValue::F32(f32::NAN),
        Value(v) => match v.bits() {
            Bits::U32(v) => tinywasm_types::WasmValue::F32(f32::from_bits(v)),
            Bits::U64(v) => tinywasm_types::WasmValue::F64(f64::from_bits(v)),
        },
    })
}

struct TestSuite(BTreeMap<String, TestGroup>);

impl TestSuite {
    fn new() -> Self {
        Self(BTreeMap::new())
    }

    fn failed(&self) -> bool {
        self.0.values().any(|group| group.stats().1 > 0)
    }

    fn test_group(&mut self, name: &str) -> &mut TestGroup {
        self.0.entry(name.to_string()).or_insert_with(TestGroup::new)
    }
}

impl Debug for TestSuite {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use owo_colors::OwoColorize;
        let mut total_passed = 0;
        let mut total_failed = 0;

        for (group_name, group) in &self.0 {
            let (group_passed, group_failed) = group.stats();
            total_passed += group_passed;
            total_failed += group_failed;

            writeln!(f, "{}", group_name.bold().underline())?;
            writeln!(f, "  Tests Passed: {}", group_passed.to_string().green())?;
            writeln!(f, "  Tests Failed: {}", group_failed.to_string().red())?;

            // for (test_name, test) in &group.tests {
            //     write!(f, "    {}: ", test_name.bold())?;
            //     match &test.result {
            //         Ok(()) => {
            //             writeln!(f, "{}", "Passed".green())?;
            //         }
            //         Err(e) => {
            //             writeln!(f, "{}", "Failed".red())?;
            //             // writeln!(f, "Error: {:?}", e)?;
            //         }
            //     }
            //     writeln!(f, "      Span: {:?}", test.span)?;
            // }
        }

        writeln!(f, "\n{}", "Total Test Summary:".bold().underline())?;
        writeln!(f, "  Total Tests: {}", (total_passed + total_failed))?;
        writeln!(f, "  Total Passed: {}", total_passed.to_string().green())?;
        writeln!(f, "  Total Failed: {}", total_failed.to_string().red())?;
        Ok(())
    }
}

struct TestGroup {
    tests: BTreeMap<String, TestCase>,
}

impl TestGroup {
    fn new() -> Self {
        Self { tests: BTreeMap::new() }
    }

    fn stats(&self) -> (usize, usize) {
        let mut passed_count = 0;
        let mut failed_count = 0;

        for test in self.tests.values() {
            match test.result {
                Ok(()) => passed_count += 1,
                Err(_) => failed_count += 1,
            }
        }

        (passed_count, failed_count)
    }

    fn add_result(&mut self, name: &str, span: wast::token::Span, result: Result<()>) {
        self.tests.insert(name.to_string(), TestCase { result, _span: span });
    }
}

struct TestCase {
    result: Result<()>,
    _span: wast::token::Span,
}
