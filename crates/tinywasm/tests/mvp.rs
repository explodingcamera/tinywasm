mod charts;
mod testsuite;

use eyre::{eyre, Result};
use testsuite::TestSuite;

#[test]
#[ignore]
fn generate_charts() -> Result<()> {
    // Create a line chart
    charts::create_progress_chart(
        std::path::Path::new("./tests/mvp.csv"),
        std::path::Path::new("./tests/progress-mvp.svg"),
    )?;

    // // Create a bar chart
    // charts::create_bar_chart(
    //     std::path::Path::new("./tests/mvp.csv"),
    //     std::path::Path::new("./tests/mvp_bar_chart.png"),
    // )?;

    Ok(())
}

#[test]
#[ignore]
fn test_mvp() -> Result<()> {
    let mut test_suite = TestSuite::new();
    test_suite.run(wasm_testsuite::MVP_TESTS)?;

    test_suite.save_csv("./tests/mvp.csv", env!("CARGO_PKG_VERSION"))?;

    if test_suite.failed() {
        eprintln!("\n\nfailed one or more tests:\n{:#?}", test_suite);
        Err(eyre!("failed one or more tests"))
    } else {
        println!("\n\npassed all tests:\n{:#?}", test_suite);
        Ok(())
    }
}
