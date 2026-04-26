use clap::{
    Args, Parser, Subcommand, ValueEnum,
    builder::{
        Styles,
        styling::{AnsiColor, Effects},
    },
};
use clap_complete::Shell;

use crate::engine_flags::EngineFlags;

// based on https://github.com/crate-ci/clap-cargo/blob/master/src/style.rs
const STYLES: Styles = Styles::styled()
    .header(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::BrightGreen.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::BrightCyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default())
    .error(AnsiColor::BrightRed.on_default().effects(Effects::BOLD))
    .valid(AnsiColor::BrightCyan.on_default().effects(Effects::BOLD))
    .invalid(AnsiColor::Yellow.on_default());

#[derive(Parser)]
#[command(
    name = "tinywasm",
    about = "TinyWasm CLI",
    styles = STYLES,
    version,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true
)]
pub struct Cli {
    #[arg(long, global = true, value_enum, default_value_t = LogLevel::Info)]
    pub log_level: LogLevel,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub run: RunArgs,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a module
    Run(RunArgs),
    /// Compile a Wasm/WAT module to a .twasm archive
    Compile(CompileArgs),
    /// Dump lowered TinyWasm bytecode
    Dump(ModuleInputArgs),
    /// Inspect imports and exports
    Inspect(ModuleInputArgs),
    #[cfg(feature = "wast")]
    /// Execute WebAssembly spec scripts (.wast)
    Wast(WastArgs),
    /// Generate shell completions
    Completion(CompletionArgs),
}

#[derive(Args, Clone)]
pub struct RunArgs {
    /// Module path, or `-` to read from stdin
    pub module: Option<String>,

    /// Invoke a named export instead of the default entrypoint
    #[arg(long)]
    pub invoke: Option<String>,

    #[command(flatten)]
    pub engine: EngineFlags,

    /// Arguments passed to the invoked Wasm function
    #[arg(trailing_var_arg = true)]
    pub args: Vec<String>,
}

#[derive(Args, Clone)]
pub struct CompileArgs {
    /// Input module path, or `-` to read from stdin
    pub input: String,

    /// Output path, or `-` to write to stdout
    #[arg(short, long)]
    pub output: Option<String>,

    /// Overwrite the output file if it already exists
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args, Clone)]
pub struct ModuleInputArgs {
    /// Module path, or `-` to read from stdin
    pub module: String,
}

#[derive(Args, Clone)]
pub struct CompletionArgs {
    pub shell: Shell,
}

#[cfg(feature = "wast")]
#[derive(Args, Clone)]
pub struct WastArgs {
    /// WAST files or directories containing .wast files
    #[arg(required = true)]
    pub paths: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for log::LevelFilter {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Trace => log::LevelFilter::Trace,
            LogLevel::Debug => log::LevelFilter::Debug,
            LogLevel::Info => log::LevelFilter::Info,
            LogLevel::Warn => log::LevelFilter::Warn,
            LogLevel::Error => log::LevelFilter::Error,
        }
    }
}
