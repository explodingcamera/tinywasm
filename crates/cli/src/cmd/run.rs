use eyre::{Result, bail};
use tinywasm::types::ExportType;
use tinywasm::{ModuleInstance, Store};

use crate::cli::RunArgs;
use crate::load::load_module;
use crate::output::print_results;
use crate::value_parse::parse_invocation_args;

pub fn run(args: RunArgs) -> Result<()> {
    let module_path = args.module.as_deref().ok_or_else(|| eyre::eyre!("missing module path"))?;
    let loaded = load_module(module_path)?;
    let mut store = Store::new(args.engine.build_engine()?);
    let instance = ModuleInstance::instantiate_no_start(&mut store, &loaded.module, None)?;

    match args.invoke.as_deref() {
        Some(export) => {
            if loaded.module.start_func.is_some() {
                let _ = instance.start(&mut store)?;
            }

            let func_ty = loaded
                .module
                .exports()
                .find_map(|item| match (item.name == export, item.ty) {
                    (true, ExportType::Func(ty)) => Some(ty),
                    _ => None,
                })
                .ok_or_else(|| eyre::eyre!("export is not a function: {export}"))?;
            let func = instance.func_untyped(&store, export)?;
            let params = parse_invocation_args(func_ty, &args.args)?;
            let results = func.call(&mut store, &params)?;
            print_results(&results);
            Ok(())
        }
        None => {
            if instance.start_func(&store)?.is_none() {
                bail!(
                    "module has no start function or `_start` export; use `tinywasm inspect {module_path}` or `tinywasm run --invoke <export> {module_path}`"
                )
            }

            let _ = instance.start(&mut store)?;
            Ok(())
        }
    }
}
