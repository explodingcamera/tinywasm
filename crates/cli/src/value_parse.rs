use eyre::{Result, bail};
use tinywasm::types::{FuncType, WasmType, WasmValue};

use crate::output::format_wasm_type;

pub fn parse_invocation_args(ty: &FuncType, args: &[String]) -> Result<Vec<WasmValue>> {
    if args.len() != ty.params().len() {
        bail!("wrong number of arguments: expected {}, got {}", ty.params().len(), args.len())
    }

    ty.params().iter().enumerate().map(|(idx, param_ty)| parse_arg(idx, *param_ty, &args[idx])).collect()
}

fn parse_arg(index: usize, ty: WasmType, value: &str) -> Result<WasmValue> {
    let parsed = match ty {
        WasmType::I32 => value.parse::<i32>().map(WasmValue::from).map_err(|e| format_error(index, ty, value, e))?,
        WasmType::I64 => value.parse::<i64>().map(WasmValue::from).map_err(|e| format_error(index, ty, value, e))?,
        WasmType::F32 => value.parse::<f32>().map(WasmValue::from).map_err(|e| format_error(index, ty, value, e))?,
        WasmType::F64 => value.parse::<f64>().map(WasmValue::from).map_err(|e| format_error(index, ty, value, e))?,
        WasmType::V128 => value.parse::<i128>().map(WasmValue::from).map_err(|e| format_error(index, ty, value, e))?,
        WasmType::RefFunc | WasmType::RefExtern => {
            bail!(
                "unsupported CLI argument type at position {}: {}; use the embedding API for reference values",
                index + 1,
                format_wasm_type(ty)
            )
        }
    };

    Ok(parsed)
}

fn format_error(index: usize, ty: WasmType, value: &str, error: impl core::fmt::Display) -> eyre::Report {
    eyre::eyre!("failed to parse argument {} as {} from `{value}`: {error}", index + 1, format_wasm_type(ty))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_args() {
        let ty = FuncType::new(&[WasmType::I32, WasmType::F64], &[WasmType::I32]);
        let args = vec!["1".to_string(), "2.5".to_string()];
        let parsed = parse_invocation_args(&ty, &args).unwrap();
        assert_eq!(parsed, vec![WasmValue::I32(1), WasmValue::F64(2.5)]);
    }

    #[test]
    fn rejects_wrong_arity() {
        let ty = FuncType::new(&[WasmType::I32], &[]);
        let err = parse_invocation_args(&ty, &[]).unwrap_err();
        assert!(err.to_string().contains("wrong number of arguments"));
    }
}
