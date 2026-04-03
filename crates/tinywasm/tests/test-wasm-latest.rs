mod testsuite;
use eyre::Result;
use testsuite::TestSuite;
use wasm_testsuite::data::{SpecVersion, spec};

fn main() -> Result<()> {
    if !std::env::args().any(|x| &x == "--enable") {
        println!("Skipping wasm-latest tests, use --enable to run");
        return Ok(());
    }

    TestSuite::set_log_level(log::LevelFilter::Off);

    let mut test_suite = TestSuite::new();
    test_suite.run_files(spec(&SpecVersion::Latest))?;
    test_suite.save_csv("./tests/generated/wasm-latest.csv", env!("CARGO_PKG_VERSION"))?;
    test_suite.report_status()
}
