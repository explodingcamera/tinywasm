use std::str::FromStr;
use tinywasm::types::WasmValue;

#[derive(Debug)]
pub struct WasmArg(WasmValue);

pub fn to_wasm_args(args: Vec<WasmArg>) -> Vec<WasmValue> {
    args.into_iter().map(|a| a.into()).collect()
}

impl From<WasmArg> for WasmValue {
    fn from(value: WasmArg) -> Self {
        value.0
    }
}

impl FromStr for WasmArg {
    type Err = String;
    fn from_str(s: &str) -> std::prelude::v1::Result<Self, Self::Err> {
        let [ty, val]: [&str; 2] =
            s.split(':').collect::<Vec<_>>().try_into().map_err(|e| format!("invalid arguments: {:?}", e))?;

        let arg: WasmValue = match ty {
            "i32" => val.parse::<i32>().map_err(|e| format!("invalid argument value for i32: {e:?}"))?.into(),
            "i64" => val.parse::<i64>().map_err(|e| format!("invalid argument value for i64: {e:?}"))?.into(),
            "f32" => val.parse::<f32>().map_err(|e| format!("invalid argument value for f32: {e:?}"))?.into(),
            "f64" => val.parse::<f64>().map_err(|e| format!("invalid argument value for f64: {e:?}"))?.into(),
            t => return Err(format!("Invalid arg type: {}", t)),
        };

        Ok(WasmArg(arg))
    }
}
