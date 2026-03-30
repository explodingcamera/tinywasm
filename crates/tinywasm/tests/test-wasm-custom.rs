mod testsuite;
use eyre::Result;
use testsuite::TestSuite;

fn main() -> Result<()> {
    TestSuite::set_log_level(log::LevelFilter::Off);

    let custom_dir = std::path::Path::new("./tests/wasm-custom");
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(custom_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "wast"))
        .collect();
    files.sort();

    let mut test_suite = TestSuite::new();
    test_suite.run_paths(&files)?;
    test_suite.report_status()
}
