mod testsuite;
use eyre::{eyre, Result};
use owo_colors::OwoColorize;
use testsuite::TestSuite;

fn main() -> Result<()> {
    let mut test_suite = TestSuite::new();

    TestSuite::set_log_level(log::LevelFilter::Off);
    test_suite.run_spec_group(wasm_testsuite::get_proposal_tests("annotations"))?;
    test_suite.save_csv("./tests/generated/wasm-annotations.csv", env!("CARGO_PKG_VERSION"))?;

    if test_suite.failed() {
        println!();
        Err(eyre!(format!("{}:\n{:#?}", "failed one or more tests".red().bold(), test_suite,)))
    } else {
        println!("\n\npassed all tests:\n{test_suite:#?}");
        Ok(())
    }
}
