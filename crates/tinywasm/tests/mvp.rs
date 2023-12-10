use std::{
    collections::HashMap,
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

    wasm_testsuite::MVP_TESTS.iter().for_each(|name| {
        println!("test: {}", name);

        let test_group = test_suite.test_group("mvp");

        let wast = wasm_testsuite::get_test_wast(name).expect("failed to get test wast");
        let wast = std::str::from_utf8(&wast).expect("failed to convert wast to utf8");

        let mut lexer = Lexer::new(&wast);
        lexer.allow_confusing_unicode(true);

        let buf = ParseBuffer::new_with_lexer(lexer).expect("failed to create parse buffer");
        let wast_data = parser::parse::<Wast>(&buf).expect("failed to parse wat");

        for directive in wast_data.directives {
            let span = directive.span();

            use wast::WastDirective::*;
            match directive {
                Wat(QuoteWat::Wat(wast::Wat::Module(module))) => {
                    let module = parse_module(module).map(|_| ());
                    test_group.module_compiles(name, span, module);
                }
                _ => {}
            }
        }
    });

    if test_suite.failed() {
        panic!("failed one or more tests: {:#?}", test_suite);
    }
}

struct TestSuite(HashMap<String, TestGroup>);

impl TestSuite {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn failed(&self) -> bool {
        self.0.values().any(|group| group.failed())
    }

    fn test_group(&mut self, name: &str) -> &mut TestGroup {
        self.0.entry(name.to_string()).or_insert_with(TestGroup::new)
    }
}

impl Debug for TestSuite {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use owo_colors::OwoColorize;
        let mut passed_count = 0;
        let mut failed_count = 0;

        for (group_name, group) in &self.0 {
            writeln!(f, "{}", group_name.bold().underline())?;
            for (test_name, test) in &group.tests {
                writeln!(f, "  {}", test_name.bold())?;
                match test.result {
                    Ok(()) => {
                        writeln!(f, "    Result: {}", "Passed".green())?;
                        passed_count += 1;
                    }
                    Err(_) => {
                        writeln!(f, "    Result: {}", "Failed".red())?;
                        failed_count += 1;
                    }
                }
                writeln!(f, "    Span: {:?}", test.span)?;
            }
        }

        writeln!(f, "\n{}", "Test Summary:".bold().underline())?;
        writeln!(f, "  Total Tests: {}", (passed_count + failed_count))?;
        writeln!(f, "  Passed: {}", passed_count.to_string().green())?;
        writeln!(f, "  Failed: {}", failed_count.to_string().red())?;
        Ok(())
    }
}

struct TestGroup {
    tests: HashMap<String, TestCase>,
}

impl TestGroup {
    fn new() -> Self {
        Self { tests: HashMap::new() }
    }

    fn failed(&self) -> bool {
        self.tests.values().any(|test| test.result.is_err())
    }

    fn module_compiles(&mut self, name: &str, span: wast::token::Span, result: Result<()>) {
        self.tests.insert(name.to_string(), TestCase { result, span });
    }
}

struct TestCase {
    result: Result<()>,
    span: wast::token::Span,
}
