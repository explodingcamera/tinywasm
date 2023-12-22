use eyre::{bail, Result};
use testsuite::TestSuite;

mod testsuite;

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 {
        bail!("usage: cargo test-wast <wast-file>")
    }

    let wast_file = &args[1];
    test_wast(wast_file)?;
    Ok(())
}

fn test_wast(wast_file: &str) -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    println!("args: {:?}", args);

    let mut test_suite = TestSuite::new();
    test_suite.run(&[wast_file])?;

    if test_suite.failed() {
        eprintln!("\n\nfailed one or more tests:\n{:#?}", test_suite);
        bail!("failed one or more tests")
    } else {
        println!("\n\npassed all tests:\n{:#?}", test_suite);
        Ok(())
    }
}
