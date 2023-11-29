use argh::FromArgs;
use color_eyre::eyre::Result;
use tinywasm::{self, Module, WasmValue};
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

#[derive(FromArgs)]
/// run a wasm file
#[argh(subcommand, name = "run")]
struct Run {
    /// wasm file to run
    #[argh(positional)]
    wasm_file: String,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    install_tracing(None);

    let args: TinyWasmCli = argh::from_env();

    match args.nested {
        TinyWasmSubcommand::Run(Run { wasm_file }) => {
            let wasm = std::fs::read(wasm_file).unwrap();
            run(&wasm)?;
            Ok(())
        }
    }
}

fn run(wasm: &[u8]) -> Result<()> {
    let mut module = Module::new(wasm)?;
    let args = [WasmValue::I32(1), WasmValue::I32(2)];
    let res = tinywasm::naive_runtime::run(&mut module, "add", &args)?;
    println!("res: {:?}", res);

    let args = [WasmValue::I64(1), WasmValue::I64(2)];
    let res = tinywasm::naive_runtime::run(&mut module, "add_64", &args)?;
    println!("res: {:?}", res);

    Ok(())
}
