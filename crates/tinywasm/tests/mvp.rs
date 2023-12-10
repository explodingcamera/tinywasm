use std::{
    collections::BTreeMap,
    fmt::{Debug, Formatter},
};

use tinywasm::{Error, Result};
use tinywasm_types::TinyWasmModule;
use wast::{
    lexer::Lexer,
    parser::{self, ParseBuffer},
    QuoteWat, Wast,
};

fn parse_module(mut module: wast::core::Module) -> Result<TinyWasmModule, Error> {
    let parser = tinywasm_parser::Parser::new();
    Ok(parser.parse_module_bytes(module.encode().expect("failed to encode module"))?)
}

#[test]
#[ignore]
fn test_mvp() {
    let mut test_suite = TestSuite::new();

    wasm_testsuite::MVP_TESTS.iter().for_each(|group| {
        println!("test: {}", group);

        let test_group = test_suite.test_group(group);

        let wast = wasm_testsuite::get_test_wast(group).expect("failed to get test wast");
        let wast = std::str::from_utf8(&wast).expect("failed to convert wast to utf8");

        let mut lexer = Lexer::new(&wast);
        // we need to allow confusing unicode characters since they are technically valid wasm
        lexer.allow_confusing_unicode(true);

        let buf = ParseBuffer::new_with_lexer(lexer).expect("failed to create parse buffer");
        let wast_data = parser::parse::<Wast>(&buf).expect("failed to parse wat");

        for (i, directive) in wast_data.directives.into_iter().enumerate() {
            let span = directive.span();

            use wast::WastDirective::*;
            let name = format!("{}-{}", group, i);
            match directive {
                // TODO: needs to support more binary sections
                Wat(QuoteWat::Wat(wast::Wat::Module(module))) => {
                    let module = std::panic::catch_unwind(|| parse_module(module));
                    test_group.add_result(
                        &format!("{}-parse", name),
                        span,
                        match module {
                            Ok(Ok(_)) => Ok(()),
                            Ok(Err(e)) => Err(e),
                            Err(e) => Err(Error::Other(format!("failed to parse module: {:?}", e))),
                        },
                    );
                }
                // these all pass already :)
                AssertMalformed {
                    span,
                    module: QuoteWat::Wat(wast::Wat::Module(module)),
                    message,
                } => {
                    println!("  assert_malformed: {}", message);
                    let res = std::panic::catch_unwind(|| parse_module(module).map(|_| ()));

                    test_group.add_result(
                        &format!("{}-malformed", name),
                        span,
                        match res {
                            Ok(Ok(_)) => Err(Error::Other("expected module to be malformed".to_string())),
                            Err(_) | Ok(Err(_)) => Ok(()),
                        },
                    );
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
        panic!("failed one or more tests: {:#?}", test_suite);
    } else {
        println!("passed all tests: {:#?}", test_suite);
    }
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
        self.tests.insert(name.to_string(), TestCase { result, span });
    }
}

struct TestCase {
    result: Result<()>,
    span: wast::token::Span,
}
