mod testsuite;
use eyre::{eyre, Result};
use owo_colors::OwoColorize;
use testsuite::TestSuite;

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 || args[1] != "--enable" {
        return Ok(());
    }

    test_mvp()
}

fn test_mvp() -> Result<()> {
    let mut test_suite = TestSuite::new();

    TestSuite::set_log_level(log::LevelFilter::Off);
    test_suite.run_spec_group(wasm_testsuite::MVP_TESTS)?;
    test_suite.save_csv("./tests/generated/mvp.csv", env!("CARGO_PKG_VERSION"))?;

    if test_suite.failed() {
        println!();
        Err(eyre!(format!("{}:\n{:#?}", "failed one or more tests".red().bold(), test_suite,)))
    } else {
        println!("\n\npassed all tests:\n{:#?}", test_suite);
        Ok(())
    }
}
