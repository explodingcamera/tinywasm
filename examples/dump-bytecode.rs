use eyre::{Result, bail};
use std::io::Read;
use std::path::Path;
use tinywasm::parser::Parser;
use tinywasm::types::{ExternalKind, ImportKind};

fn read_input(path: &str) -> Result<Vec<u8>> {
    if path == "-" {
        let mut source = String::new();
        std::io::stdin().read_to_string(&mut source)?;
        return Ok(wat::parse_str(source)?);
    }

    let bytes = std::fs::read(path)?;
    let is_wasm = Path::new(path).extension().and_then(|s| s.to_str()) == Some("wasm");
    if is_wasm { Ok(bytes) } else { Ok(wat::parse_bytes(&bytes)?.into_owned()) }
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        bail!("usage: cargo run --example dump-bytecode -- <module.wat|module.wasm|->")
    }

    let wasm = read_input(&args[1])?;
    let module = Parser::new().parse_module_bytes(&wasm)?;

    let imported_func_count =
        module.imports.iter().filter(|import| matches!(import.kind, ImportKind::Function(_))).count() as u32;

    for (func_idx, func) in module.funcs.iter().enumerate() {
        let global_idx = imported_func_count + func_idx as u32;
        let exports = module
            .exports
            .iter()
            .filter(|export| export.kind == ExternalKind::Func && export.index == global_idx)
            .map(|export| export.name.as_ref())
            .collect::<Vec<_>>();

        if exports.is_empty() {
            println!("func[{func_idx}] global={global_idx}");
        } else {
            println!("func[{func_idx}] global={global_idx} exports={exports:?}");
        }

        for (ip, instr) in func.instructions.iter().enumerate() {
            println!("  {ip:04}: {instr:?}");
        }
        println!();
    }

    Ok(())
}
