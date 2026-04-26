use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::Path;

use eyre::{Context, Result, bail};
use tinywasm::Module;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputFormat {
    Wasm,
    Wat,
    Twasm,
}

pub struct LoadedModule {
    pub module: Module,
    pub format: InputFormat,
}

pub fn load_module(input: &str) -> Result<LoadedModule> {
    let bytes = read_input_bytes(input)?;
    load_module_from_bytes(input, &bytes)
}

pub fn load_compilable_module(input: &str) -> Result<Module> {
    let loaded = load_module(input)?;
    if loaded.format == InputFormat::Twasm {
        bail!("input is already a twasm archive; use `run`, `dump`, or `inspect` instead")
    }
    Ok(loaded.module)
}

pub fn default_twasm_output_path(input: &str) -> Result<String> {
    if input == "-" {
        bail!("--output is required when compiling from stdin")
    }

    let path = Path::new(input);
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("module");
    let output = path.with_file_name(format!("{stem}.twasm"));
    Ok(output.to_string_lossy().into_owned())
}

pub fn write_output_bytes(output: &str, bytes: &[u8], force: bool) -> Result<()> {
    if output == "-" {
        std::io::stdout().write_all(bytes)?;
        std::io::stdout().flush()?;
        return Ok(());
    }

    let path = Path::new(output);
    if path.exists() && !force {
        bail!("output file already exists: {output}; pass --force to overwrite")
    }

    std::fs::write(path, bytes).with_context(|| format!("failed to write output file `{output}`"))?;
    Ok(())
}

fn load_module_from_bytes(input: &str, bytes: &[u8]) -> Result<LoadedModule> {
    if bytes.starts_with(b"TWAS") {
        let module = Module::try_from_twasm(bytes).with_context(|| format!("failed to read twasm input `{input}`"))?;
        return Ok(LoadedModule { module, format: InputFormat::Twasm });
    }

    #[cfg(feature = "wat")]
    if input != "-" && has_extension(input, "wat") {
        let wasm = wat::parse_bytes(bytes).with_context(|| format!("failed to parse WAT input `{input}`"))?;
        let module =
            tinywasm::parse_bytes(&wasm).with_context(|| format!("failed to parse Wasm generated from `{input}`"))?;
        return Ok(LoadedModule { module, format: InputFormat::Wat });
    }

    #[cfg(not(feature = "wat"))]
    if input != "-" && has_extension(input, "wat") {
        bail!("wat support is not enabled in this build")
    }

    #[cfg(feature = "wat")]
    if input == "-"
        && let Ok(wasm) = wat::parse_bytes(bytes)
    {
        let module = tinywasm::parse_bytes(&wasm).context("failed to parse Wasm generated from stdin WAT input")?;
        return Ok(LoadedModule { module, format: InputFormat::Wat });
    }

    let module = tinywasm::parse_bytes(bytes).with_context(|| format!("failed to parse Wasm input `{input}`"))?;
    Ok(LoadedModule { module, format: InputFormat::Wasm })
}

fn read_input_bytes(input: &str) -> Result<Vec<u8>> {
    if input == "-" {
        let mut bytes = Vec::new();
        std::io::stdin().read_to_end(&mut bytes).context("failed to read stdin")?;
        return Ok(bytes);
    }

    std::fs::read(input).with_context(|| format!("failed to read input `{input}`"))
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path).extension().and_then(OsStr::to_str) == Some(extension)
}
