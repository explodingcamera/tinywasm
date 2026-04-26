use eyre::Result;

use crate::cli::CompileArgs;
use crate::load::{default_twasm_output_path, load_compilable_module, write_output_bytes};

pub fn run(args: CompileArgs) -> Result<()> {
    let module = load_compilable_module(&args.input)?;
    let twasm = module.serialize_twasm()?;
    let output = match args.output {
        Some(output) => output,
        None => default_twasm_output_path(&args.input)?,
    };

    write_output_bytes(&output, &twasm, args.force)
}
