use std::io;

use anyhow::Result;
use clap::CommandFactory;

use crate::cli::{Cli, CompletionArgs};

pub fn run(args: CompletionArgs) -> Result<()> {
    let mut cmd = Cli::command();
    clap_complete::generate(args.shell, &mut cmd, "tinywasm", &mut io::stdout());
    Ok(())
}
