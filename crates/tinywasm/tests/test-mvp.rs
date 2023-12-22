mod testsuite;
use eyre::{eyre, Result};
use testsuite::TestSuite;

fn main() -> Result<()> {
    test_mvp()
}

fn test_mvp() -> Result<()> {
    let mut test_suite = TestSuite::new();

    test_suite.run(wasm_testsuite::MVP_TESTS)?;
    test_suite.save_csv("./tests/generated/mvp.csv", env!("CARGO_PKG_VERSION"))?;

    if test_suite.failed() {
        eprintln!("\n\nfailed one or more tests:\n{:#?}", test_suite);
        Err(eyre!("failed one or more tests"))
    } else {
        println!("\n\npassed all tests:\n{:#?}", test_suite);
        Ok(())
    }
}
