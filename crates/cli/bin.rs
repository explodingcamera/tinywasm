use std::str::FromStr;

use argh::FromArgs;
use color_eyre::eyre::Result;
use log::info;
use tinywasm::{self, Module, WasmValue};
mod util;
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

    pretty_env_logger::formatted_builder()
        .filter_level(level)
        .init();

    let cwd = std::env::current_dir()?;

    match args.nested {
        TinyWasmSubcommand::Run(Run { wasm_file, engine }) => {
            let path = cwd.join(wasm_file.clone());
            let module = match wasm_file.ends_with(".wat") {
                true => {
                    let wat = std::fs::read_to_string(path)?;
                    let wasm = wat::wat2wasm(&wat);
                    tinywasm::Module::parse_bytes(&wasm)?
                }
                false => tinywasm::Module::parse_file(path)?,
            };

            match engine {
                Engine::Main => run(module),
            }
        }
    }
}

fn run(module: Module) -> Result<()> {
    let mut store = tinywasm::Store::default();

    let instance = module.instantiate(&mut store)?;
    let func = instance.get_func(&store, "add")?;
    let params = vec![WasmValue::I32(2), WasmValue::I32(2)];
    let res = func.call(&mut store, params)?;
    info!("{res:?}");

    Ok(())
}
