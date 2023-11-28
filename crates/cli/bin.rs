use argh::FromArgs;
use color_eyre::eyre::Result;
use tinywasm::{self, Module};

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
    let module = Module::new(wasm)?;
    println!("{:#?}", module);
    Ok(())
}
