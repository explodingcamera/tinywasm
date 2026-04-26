use anstream::println;
use eyre::Result;
use owo_colors::OwoColorize;
use tinywasm::types::{ExternalKind, ImportKind};

use crate::cli::ModuleInputArgs;
use crate::load::load_module;

pub fn run(args: ModuleInputArgs) -> Result<()> {
    let loaded = load_module(&args.module)?;
    let module = loaded.module;

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

        let header = format!("func[{func_idx}]").blue().bold().to_string();
        if exports.is_empty() {
            println!("{header}");
        } else {
            println!("{header} {}", format!("exports={}", format!("{exports:?}").cyan()).bright_black());
        }

        for (ip, instr) in func.instructions.iter().enumerate() {
            let instr = print_instr(instr);
            println!("  {}: {}", print_ip(ip), instr);
        }
        println!();
    }

    Ok(())
}

fn print_ip(ip: usize) -> String {
    let s = format!("{ip:04}");
    let first_non_zero = s.find(|c| c != '0').unwrap_or(s.len() - 1);

    format!(
        "{}{}",
        &s[..first_non_zero].to_string().bright_black().dimmed(),
        &s[first_non_zero..].to_string().bright_black()
    )
}

fn print_instr(instr: &tinywasm::types::Instruction) -> String {
    let instr = format!("{instr:?}");
    let Some(split) = instr.find(['(', ' ', '{']) else {
        return instr.bold().to_string();
    };

    let (name, rest) = instr.split_at(split);

    let rest = rest
        .replace('(', &"(".bright_black().to_string())
        .replace(')', &")".bright_black().to_string())
        .replace('{', &"{".bright_black().to_string())
        .replace('}', &"}".bright_black().to_string())
        .replace(',', &",".bright_black().to_string())
        .replace(':', &":".bright_black().to_string());

    format!("{}{}", name.bold(), rest)
}
