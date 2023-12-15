mod testsuite;

use eyre::{eyre, Result};
use testsuite::TestSuite;

#[test]
#[ignore]
fn test_mvp() -> Result<()> {
    let mut test_suite = TestSuite::new();
    test_suite.run(wasm_testsuite::MVP_TESTS)?;

    test_suite.save_csv("./tests/mvp.csv", env!("CARGO_PKG_VERSION").trim_end())?;

    if test_suite.failed() {
        eprintln!("\n\nfailed one or more tests:\n{:#?}", test_suite);
        Err(eyre!("failed one or more tests"))
    } else {
        println!("\n\npassed all tests:\n{:#?}", test_suite);
        Ok(())
    }
}
