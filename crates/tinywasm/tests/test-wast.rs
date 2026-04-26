use std::path::PathBuf;

use eyre::{Result, bail};
use tinywasm_cli::wast_runner::WastRunner;

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 {
        bail!("usage: cargo test-wast <wast-file>")
    };

    let mut cwd = std::env::current_dir()?;
    if cwd.ends_with("crates/tinywasm/") {
        cwd.pop();
        cwd.pop();
    }

    // if its a folder, run all the wast files in the folder
    let arg = PathBuf::from(cwd.clone()).join(&args[1]);
    println!("running tests in {:?}", arg);

    let files = if arg.is_dir() {
        std::fs::read_dir(&arg)?.map(|entry| entry.map(|e| e.path())).collect::<Result<Vec<_>, _>>()?
    } else {
        vec![arg]
    };

    let mut test_suite = WastRunner::new();
    test_suite.run_paths(&files)
}
