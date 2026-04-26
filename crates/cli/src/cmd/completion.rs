use std::io;

use clap::CommandFactory;
use eyre::Result;

use crate::cli::{Cli, CompletionArgs};

pub fn run(args: CompletionArgs) -> Result<()> {
    let mut cmd = Cli::command();
    clap_complete::generate(args.shell, &mut cmd, "tinywasm", &mut io::stdout());
    Ok(())
}
