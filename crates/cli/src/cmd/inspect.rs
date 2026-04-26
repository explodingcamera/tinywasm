use eyre::Result;

use crate::cli::ModuleInputArgs;
use crate::load::load_module;
use crate::output::{format_export_type, format_import_type};
use anstream::println;
use owo_colors::OwoColorize;

pub fn run(args: ModuleInputArgs) -> Result<()> {
    let loaded = load_module(&args.module)?;
    let module = loaded.module;

    println!("{}", "Imports".bold());
    let mut import_count = 0usize;
    for import in module.imports() {
        import_count += 1;
        println!("  {}.{}: {}", import.module.blue(), import.name.cyan(), format_import_type(import.ty).yellow());
    }
    if import_count == 0 {
        println!("  {}", "(none)".yellow());
    }

    println!();
    println!("{}", "Exports".bold());
    let mut export_count = 0usize;
    for export in module.exports() {
        export_count += 1;
        println!("  {}: {}", export.name.green(), format_export_type(export.ty).yellow());
    }
    if export_count == 0 {
        println!("  {}", "(none)".yellow());
    }

    Ok(())
}
