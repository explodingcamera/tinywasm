mod testsuite;
use eyre::{eyre, Result};
use owo_colors::OwoColorize;
use testsuite::TestSuite;

fn main() -> Result<()> {
    let mut test_suite = TestSuite::new();
    test_suite.skip("memory64/array.wast");
    test_suite.skip("memory64/extern.wast");
    test_suite.skip("memory64/global.wast");
    test_suite.skip("memory64/i31.wast");
    test_suite.skip("memory64/ref_null.wast");
    test_suite.skip("memory64/select.wast");
    test_suite.skip("memory64/simd_address.wast");
    test_suite.skip("memory64/simd_lane.wast");
    test_suite.skip("memory64/struct.wast");
    test_suite.skip("memory64/table.wast");

    TestSuite::set_log_level(log::LevelFilter::Off);
    test_suite.run_spec_group(wasm_testsuite::get_proposal_tests("memory64"))?;
    test_suite.save_csv("./tests/generated/wasm-memory64.csv", env!("CARGO_PKG_VERSION"))?;

    if test_suite.failed() {
        println!();
        Err(eyre!(format!("{}:\n{:#?}", "failed one or more tests".red().bold(), test_suite,)))
    } else {
        println!("\n\npassed all tests:\n{test_suite:#?}");
        Ok(())
    }
}
