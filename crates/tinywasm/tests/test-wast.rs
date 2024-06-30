use std::path::PathBuf;

use eyre::{bail, eyre, Result};
use owo_colors::OwoColorize;
use testsuite::TestSuite;

mod testsuite;

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 || args[1] != "--enable" {
        return Ok(());
    }

    if args.len() < 3 {
        bail!("usage: cargo test-wast <wast-file>")
    };

    // cwd for relative paths, absolute paths are kept as-is
    let cwd = std::env::current_dir()?;

    // if current dir is crates/tinywasm, then we want to go up 2 levels
    let mut wast_file = if cwd.ends_with("crates/tinywasm") { PathBuf::from("../../") } else { PathBuf::from("./") };

    wast_file.push(&args[2]);
    let wast_file = cwd.join(wast_file);

    test_wast(wast_file.to_str().expect("wast_file is not a valid path"))?;
    Ok(())
}

fn test_wast(wast_file: &str) -> Result<()> {
    TestSuite::set_log_level(log::LevelFilter::Debug);

    let args = std::env::args().collect::<Vec<_>>();
    println!("args: {:?}", args);

    let mut test_suite = TestSuite::new();
    println!("running wast file: {}", wast_file);

    test_suite.run_paths(&[wast_file])?;

    if test_suite.failed() {
        println!();
        test_suite.print_errors();
        println!();
        Err(eyre!(format!("{}:\n{:#?}", "failed one or more tests".red().bold(), test_suite,)))
    } else {
        println!("\n\npassed all tests:\n{:#?}", test_suite);
        Ok(())
    }
}
