use std::path::PathBuf;

use eyre::Result;

use crate::cli::WastArgs;
use crate::wast_runner::WastRunner;

pub fn run(args: WastArgs) -> Result<()> {
    let paths = args.paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    let mut runner = WastRunner::new();
    runner.run_paths(&paths)
}
