use std::panic::{self, AssertUnwindSafe};

use eyre::{eyre, Result};
use tinywasm_types::{TinyWasmModule, WasmValue};

pub fn try_downcast_panic(panic: Box<dyn std::any::Any + Send>) -> String {
    let info = panic
        .downcast_ref::<panic::PanicInfo>()
        .or(None)
        .map(|p| p.to_string())
        .clone();
    let info_string = panic.downcast_ref::<String>().cloned();
    let info_str = panic.downcast::<&str>().ok().map(|s| *s);

    info.unwrap_or(
        info_str
            .unwrap_or(&info_string.unwrap_or("unknown panic".to_owned()))
            .to_string(),
    )
}

pub fn exec_fn_instance(
    instance: Option<&tinywasm::ModuleInstance>,
    store: &mut tinywasm::Store,
    name: &str,
    args: &[tinywasm_types::WasmValue],
) -> Result<Vec<tinywasm_types::WasmValue>, tinywasm::Error> {
    let Some(instance) = instance else {
        return Err(tinywasm::Error::Other("no instance found".to_string()));
    };

    let func = instance.exported_func_by_name(store, name)?;
    func.call(store, args)
}

pub fn exec_fn(
    module: Option<&TinyWasmModule>,
    name: &str,
    args: &[tinywasm_types::WasmValue],
    imports: Option<tinywasm::Imports>,
) -> Result<Vec<tinywasm_types::WasmValue>, tinywasm::Error> {
    let Some(module) = module else {
        return Err(tinywasm::Error::Other("no module found".to_string()));
    };

    let mut store = tinywasm::Store::new();
    let module = tinywasm::Module::from(module);
    let instance = module.instantiate(&mut store, imports)?;
    instance.exported_func_by_name(&store, name)?.call(&mut store, args)
}

pub fn catch_unwind_silent<F: FnOnce() -> R, R>(f: F) -> std::thread::Result<R> {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    panic::set_hook(prev_hook);
    result
}

pub fn parse_module_bytes(bytes: &[u8]) -> Result<TinyWasmModule> {
    let parser = tinywasm_parser::Parser::new();
    Ok(parser.parse_module_bytes(bytes)?)
}

pub fn wastarg2tinywasmvalue(arg: wast::WastArg) -> Result<tinywasm_types::WasmValue> {
    let wast::WastArg::Core(arg) = arg else {
        return Err(eyre!("unsupported arg type"));
    };

    use wast::core::WastArgCore::*;
    Ok(match arg {
        F32(f) => WasmValue::F32(f32::from_bits(f.bits)),
        F64(f) => WasmValue::F64(f64::from_bits(f.bits)),
        I32(i) => WasmValue::I32(i),
        I64(i) => WasmValue::I64(i),
        _ => return Err(eyre!("unsupported arg type")),
    })
}

pub fn wastret2tinywasmvalue(arg: wast::WastRet) -> Result<tinywasm_types::WasmValue> {
    let wast::WastRet::Core(arg) = arg else {
        return Err(eyre!("unsupported arg type"));
    };

    use wast::core::WastRetCore::*;
    Ok(match arg {
        F32(f) => nanpattern2tinywasmvalue(f)?,
        F64(f) => nanpattern2tinywasmvalue(f)?,
        I32(i) => WasmValue::I32(i),
        I64(i) => WasmValue::I64(i),
        _ => return Err(eyre!("unsupported arg type")),
    })
}

enum Bits {
    U32(u32),
    U64(u64),
}
trait FloatToken {
    fn bits(&self) -> Bits;
    fn canonical_nan() -> WasmValue;
    fn arithmetic_nan() -> WasmValue;
    fn value(&self) -> WasmValue {
        match self.bits() {
            Bits::U32(v) => WasmValue::F32(f32::from_bits(v)),
            Bits::U64(v) => WasmValue::F64(f64::from_bits(v)),
        }
    }
}
impl FloatToken for wast::token::Float32 {
    fn bits(&self) -> Bits {
        Bits::U32(self.bits)
    }

    fn canonical_nan() -> WasmValue {
        WasmValue::F32(f32::NAN)
    }

    fn arithmetic_nan() -> WasmValue {
        WasmValue::F32(f32::NAN)
    }
}
impl FloatToken for wast::token::Float64 {
    fn bits(&self) -> Bits {
        Bits::U64(self.bits)
    }

    fn canonical_nan() -> WasmValue {
        WasmValue::F64(f64::NAN)
    }

    fn arithmetic_nan() -> WasmValue {
        WasmValue::F64(f64::NAN)
    }
}

fn nanpattern2tinywasmvalue<T>(arg: wast::core::NanPattern<T>) -> Result<tinywasm_types::WasmValue>
where
    T: FloatToken,
{
    use wast::core::NanPattern::*;
    Ok(match arg {
        CanonicalNan => T::canonical_nan(),
        ArithmeticNan => T::arithmetic_nan(),
        Value(v) => v.value(),
    })
}
