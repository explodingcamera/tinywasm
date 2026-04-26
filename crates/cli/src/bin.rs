use clap::Parser;
use eyre::Result;
use tinywasm_cli::{Cli, run_cli};

fn main() -> Result<()> {
    let cli = Cli::parse();
    pretty_env_logger::formatted_builder().filter_level(cli.log_level.into()).init();
    run_cli(cli)
}
