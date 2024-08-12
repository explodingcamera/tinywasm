use std::panic::{self, AssertUnwindSafe};

use eyre::{eyre, Result};
use tinywasm_types::{ModuleInstanceAddr, TinyWasmModule, ValType, WasmValue};
use wast::{core::AbstractHeapType, QuoteWat};

pub fn try_downcast_panic(panic: Box<dyn std::any::Any + Send>) -> String {
    let info = panic.downcast_ref::<panic::PanicHookInfo>().or(None).map(ToString::to_string).clone();
    let info_string = panic.downcast_ref::<String>().cloned();
    let info_str = panic.downcast::<&str>().ok().map(|s| *s);

    info.unwrap_or(info_str.unwrap_or(&info_string.unwrap_or("unknown panic".to_owned())).to_string())
}

pub fn exec_fn_instance(
    instance: Option<&ModuleInstanceAddr>,
    store: &mut tinywasm::Store,
    name: &str,
    args: &[tinywasm_types::WasmValue],
) -> Result<Vec<tinywasm_types::WasmValue>, tinywasm::Error> {
    let Some(instance) = instance else {
        return Err(tinywasm::Error::Other("no instance found".to_string()));
    };

    let Some(instance) = store.get_module_instance(*instance) else {
        return Err(tinywasm::Error::Other("no instance found".to_string()));
    };

    let func = instance.exported_func_untyped(store, name)?;
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
    instance.exported_func_untyped(&store, name)?.call(&mut store, args)
}

pub fn catch_unwind_silent<F: FnOnce() -> R, R>(f: F) -> std::thread::Result<R> {
    let prev_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let result = panic::catch_unwind(AssertUnwindSafe(f));
    panic::set_hook(prev_hook);
    result
}

pub fn encode_quote_wat(module: QuoteWat) -> (Option<String>, Vec<u8>) {
    match module {
        QuoteWat::QuoteModule(_, quoted_wat) => {
            let wat = quoted_wat
                .iter()
                .map(|(_, s)| std::str::from_utf8(s).expect("failed to convert wast to utf8"))
                .collect::<Vec<_>>()
                .join("\n");

            let lexer = wast::lexer::Lexer::new(&wat);
            let buf = wast::parser::ParseBuffer::new_with_lexer(lexer).expect("failed to create parse buffer");
            let mut wat_data = wast::parser::parse::<wast::Wat>(&buf).expect("failed to parse wat");
            (None, wat_data.encode().expect("failed to encode module"))
        }
        QuoteWat::Wat(mut wat) => {
            let wast::Wat::Module(ref module) = wat else {
                unimplemented!("Not supported");
            };
            (module.id.map(|id| id.name().to_string()), wat.encode().expect("failed to encode module"))
        }
        _ => unimplemented!("Not supported"),
    }
}

pub fn parse_module_bytes(bytes: &[u8]) -> Result<TinyWasmModule> {
    let parser = tinywasm_parser::Parser::new();
    Ok(parser.parse_module_bytes(bytes)?)
}

pub fn convert_wastargs(args: Vec<wast::WastArg>) -> Result<Vec<tinywasm_types::WasmValue>> {
    args.into_iter().map(|a| wastarg2tinywasmvalue(a)).collect()
}

pub fn convert_wastret<'a>(args: impl Iterator<Item = wast::WastRet<'a>>) -> Result<Vec<tinywasm_types::WasmValue>> {
    args.map(|a| wastret2tinywasmvalue(a)).collect()
}

fn wastarg2tinywasmvalue(arg: wast::WastArg) -> Result<tinywasm_types::WasmValue> {
    let wast::WastArg::Core(arg) = arg else {
        return Err(eyre!("unsupported arg type: Component"));
    };

    use wast::core::WastArgCore::{RefExtern, RefNull, F32, F64, I32, I64};
    Ok(match arg {
        F32(f) => WasmValue::F32(f32::from_bits(f.bits)),
        F64(f) => WasmValue::F64(f64::from_bits(f.bits)),
        I32(i) => WasmValue::I32(i),
        I64(i) => WasmValue::I64(i),
        RefExtern(v) => WasmValue::RefExtern(v),
        RefNull(t) => match t {
            wast::core::HeapType::Abstract { shared: false, ty: AbstractHeapType::Func } => {
                WasmValue::RefNull(ValType::RefFunc)
            }
            wast::core::HeapType::Abstract { shared: false, ty: AbstractHeapType::Extern } => {
                WasmValue::RefNull(ValType::RefExtern)
            }
            _ => return Err(eyre!("unsupported arg type: refnull: {:?}", t)),
        },
        v => return Err(eyre!("unsupported arg type: {:?}", v)),
    })
}

fn wastret2tinywasmvalue(ret: wast::WastRet) -> Result<tinywasm_types::WasmValue> {
    let wast::WastRet::Core(ret) = ret else {
        return Err(eyre!("unsupported arg type"));
    };

    use wast::core::WastRetCore::{RefExtern, RefFunc, RefNull, F32, F64, I32, I64};
    Ok(match ret {
        F32(f) => nanpattern2tinywasmvalue(f)?,
        F64(f) => nanpattern2tinywasmvalue(f)?,
        I32(i) => WasmValue::I32(i),
        I64(i) => WasmValue::I64(i),
        RefNull(t) => match t {
            Some(wast::core::HeapType::Abstract { shared: false, ty: AbstractHeapType::Func }) => {
                WasmValue::RefNull(ValType::RefFunc)
            }
            Some(wast::core::HeapType::Abstract { shared: false, ty: AbstractHeapType::Extern }) => {
                WasmValue::RefNull(ValType::RefExtern)
            }
            _ => return Err(eyre!("unsupported arg type: refnull: {:?}", t)),
        },
        RefExtern(v) => match v {
            Some(v) => WasmValue::RefExtern(v),
            _ => return Err(eyre!("unsupported arg type: refextern: {:?}", v)),
        },
        RefFunc(v) => match v {
            Some(wast::token::Index::Num(n, _)) => WasmValue::RefFunc(n),
            _ => return Err(eyre!("unsupported arg type: reffunc: {:?}", v)),
        },
        a => return Err(eyre!("unsupported arg type {:?}", a)),
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
impl FloatToken for wast::token::F32 {
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
impl FloatToken for wast::token::F64 {
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
    use wast::core::NanPattern::{ArithmeticNan, CanonicalNan, Value};
    Ok(match arg {
        CanonicalNan => T::canonical_nan(),
        ArithmeticNan => T::arithmetic_nan(),
        Value(v) => v.value(),
    })
}
