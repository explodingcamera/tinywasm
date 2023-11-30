use std::str::FromStr;

use argh::FromArgs;
use color_eyre::eyre::Result;
use tinywasm::{self, WasmValue};
use util::install_tracing;

mod util;

#[derive(FromArgs)]
/// TinyWasm CLI
struct TinyWasmCli {
    #[argh(subcommand)]
    nested: TinyWasmSubcommand,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum TinyWasmSubcommand {
    Run(Run),
}

enum Engine {
    Main,
    Naive,
}

impl FromStr for Engine {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "naive" => Ok(Self::Naive),
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
    install_tracing(None);

    let args: TinyWasmCli = argh::from_env();

    match args.nested {
        TinyWasmSubcommand::Run(Run { wasm_file, engine }) => {
            let wasm = std::fs::read(wasm_file)?;
            match engine {
                Engine::Main => run(&wasm),
                Engine::Naive => run_naive(&wasm),
            }
        }
    }
}

fn run(wasm: &[u8]) -> Result<()> {
    let mut store = tinywasm::Store::default();
    let mut module = tinywasm::Module::try_new(&mut store, wasm)?;
    let instance = tinywasm::ModuleInstance::new(&mut module)?;

    Ok(())
}

fn run_naive(wasm: &[u8]) -> Result<()> {
    let mut module = tinywasm::naive::Module::new(wasm)?;
    let args = [WasmValue::I32(1), WasmValue::I32(2)];
    let res = tinywasm::naive::run(&mut module, "add", &args)?;
    println!("res: {:?}", res);

    let args = [WasmValue::I64(1), WasmValue::I64(2)];
    let res = tinywasm::naive::run(&mut module, "add_64", &args)?;
    println!("res: {:?}", res);

    Ok(())
}
