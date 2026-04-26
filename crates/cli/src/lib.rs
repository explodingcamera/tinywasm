pub mod cli;
pub mod cmd;
pub mod engine_flags;
pub mod load;
pub mod output;
pub mod value_parse;
#[cfg(feature = "wast")]
pub mod wast_runner;

use clap::CommandFactory;
use eyre::Result;

pub use cli::{Cli, Commands};

pub fn run_cli(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Commands::Run(args)) => cmd::run::run(args),
        Some(Commands::Compile(args)) => cmd::compile::run(args),
        Some(Commands::Dump(args)) => cmd::dump::run(args),
        Some(Commands::Inspect(args)) => cmd::inspect::run(args),
        #[cfg(feature = "wast")]
        Some(Commands::Wast(args)) => cmd::wast::run(args),
        Some(Commands::Completion(args)) => cmd::completion::run(args),
        None => match cli.run.module.as_deref() {
            Some(_) => cmd::run::run(cli.run),
            None => {
                let mut cmd = Cli::command();
                cmd.print_help()?;
                println!();
                Ok(())
            }
        },
    }
}
