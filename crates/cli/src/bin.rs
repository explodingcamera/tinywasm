use std::str::FromStr;

use argh::FromArgs;
use args::WasmArg;
use color_eyre::eyre::Result;
use log::{debug, info};
use tinywasm::{types::WasmValue, Module};

use crate::args::to_wasm_args;
mod args;
mod util;

#[cfg(feature = "wat")]
mod wat;

#[derive(FromArgs)]
/// TinyWasm CLI
struct TinyWasmCli {
    #[argh(subcommand)]
    nested: TinyWasmSubcommand,

    /// log level
    #[argh(option, short = 'l', default = "\"info\".to_string()")]
    log_level: String,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum TinyWasmSubcommand {
    Run(Run),
}

enum Engine {
    Main,
}

impl FromStr for Engine {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "main" => Ok(Self::Main),
            _ => Err(format!("unknown engine: {}", s)),
        }
    }
}

#[derive(FromArgs)]
/// run a wasm file
#[argh(subcommand, name = "run")]
struct Run {
    /// wasm file to run
    #[argh(positional)]
    wasm_file: String,

    /// function to run
    #[argh(option, short = 'f')]
    func: Option<String>,

    /// arguments to pass to the wasm file
    #[argh(option, short = 'a')]
    args: Vec<WasmArg>,

    /// engine to use
    #[argh(option, short = 'e', default = "Engine::Main")]
    engine: Engine,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let args: TinyWasmCli = argh::from_env();
    let level = match args.log_level.as_str() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        "info" => log::LevelFilter::Info,
        _ => log::LevelFilter::Info,
    };

    pretty_env_logger::formatted_builder().filter_level(level).init();

    let cwd = std::env::current_dir()?;

    match args.nested {
        TinyWasmSubcommand::Run(Run { wasm_file, engine, args, func }) => {
            debug!("args: {:?}", args);

            let path = cwd.join(wasm_file.clone());
            let module = match wasm_file.ends_with(".wat") {
                #[cfg(feature = "wat")]
                true => {
                    let wat = std::fs::read_to_string(path)?;
                    let wasm = wat::wat2wasm(&wat);
                    tinywasm::Module::parse_bytes(&wasm)?
                }
                #[cfg(not(feature = "wat"))]
                true => return Err(color_eyre::eyre::eyre!("wat support is not enabled in this build")),
                false => tinywasm::Module::parse_file(path)?,
            };

            match engine {
                Engine::Main => run(module, func, to_wasm_args(args)),
            }
        }
    }
}

fn run(module: Module, func: Option<String>, args: Vec<WasmValue>) -> Result<()> {
    let mut store = tinywasm::Store::default();
    let instance = module.instantiate(&mut store, None)?;

    if let Some(func) = func {
        let func = instance.exported_func_untyped(&store, &func)?;
        let res = func.call(&mut store, &args)?;
        info!("{res:?}");
    }

    Ok(())
}
