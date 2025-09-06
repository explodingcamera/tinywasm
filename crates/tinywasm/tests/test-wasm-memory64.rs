mod testsuite;
use eyre::Result;
use testsuite::TestSuite;
use wasm_testsuite::data::{Proposal, proposal};

fn main() -> Result<()> {
    TestSuite::set_log_level(log::LevelFilter::Off);
    let mut test_suite = TestSuite::new();

    test_suite.run_files(proposal(&Proposal::Memory64))?;
    test_suite.save_csv("./tests/generated/wasm-memory64.csv", env!("CARGO_PKG_VERSION"))?;
    test_suite.report_status()
}
