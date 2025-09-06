mod testsuite;
use eyre::Result;
use testsuite::TestSuite;
use wasm_testsuite::data::{SpecVersion, spec};

fn main() -> Result<()> {
    if !std::env::args().any(|x| &x == "--enable") {
        println!("Skipping wasm-3 tests, use --enable to run");
        return Ok(());
    }

    TestSuite::set_log_level(log::LevelFilter::Off);

    let mut test_suite = TestSuite::new();
    test_suite.run_files(spec(&SpecVersion::V3))?;
    test_suite.save_csv("./tests/generated/wasm-3.csv", env!("CARGO_PKG_VERSION"))?;
    test_suite.report_status()
}
