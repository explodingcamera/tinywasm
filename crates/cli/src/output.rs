use tinywasm::types::{
    ExportType, FuncType, GlobalType, ImportType, MemoryArch, MemoryType, TableType, WasmType, WasmValue,
};

pub fn print_results(results: &[WasmValue]) {
    match results {
        [] => {}
        [value] => println!("{}", format_value(value)),
        values => {
            let formatted = values.iter().map(format_value).collect::<Vec<_>>().join(", ");
            println!("[{formatted}]");
        }
    }
}

pub fn format_value(value: &WasmValue) -> String {
    format!("{value:?}")
}

pub fn color_enabled() -> bool {
    use std::io::IsTerminal;

    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

pub fn format_wasm_type(ty: WasmType) -> &'static str {
    match ty {
        WasmType::I32 => "i32",
        WasmType::I64 => "i64",
        WasmType::F32 => "f32",
        WasmType::F64 => "f64",
        WasmType::V128 => "v128",
        WasmType::RefFunc => "funcref",
        WasmType::RefExtern => "externref",
    }
}

pub fn format_func_type(ty: &FuncType) -> String {
    let params = ty.params().iter().map(|ty| format_wasm_type(*ty)).collect::<Vec<_>>().join(", ");
    let results = ty.results().iter().map(|ty| format_wasm_type(*ty)).collect::<Vec<_>>().join(", ");

    if ty.results().is_empty() { format!("({params})") } else { format!("({params}) -> ({results})") }
}

pub fn format_memory_type(ty: &MemoryType) -> String {
    let arch = match ty.arch() {
        MemoryArch::I32 => "i32",
        MemoryArch::I64 => "i64",
    };
    let max = if ty.page_count_max() == ty.page_count_initial() && ty.max_size() == ty.initial_size() {
        ty.page_count_initial().to_string()
    } else {
        ty.page_count_max().to_string()
    };

    format!("memory[{arch}] initial={} max={} page_size={}", ty.page_count_initial(), max, ty.page_size())
}

pub fn format_table_type(ty: &TableType) -> String {
    let max = ty.size_max.map(|v| v.to_string()).unwrap_or_else(|| "unbounded".to_string());
    format!("table[{}] initial={} max={max}", format_wasm_type(ty.element_type), ty.size_initial)
}

pub fn format_global_type(ty: &GlobalType) -> String {
    let mutability = if ty.mutable { "mut" } else { "const" };
    format!("global[{mutability} {}]", format_wasm_type(ty.ty))
}

pub fn format_export_type(ty: ExportType<'_>) -> String {
    match ty {
        ExportType::Func(ty) => format!("func {}", format_func_type(ty)),
        ExportType::Memory(ty) => format_memory_type(ty),
        ExportType::Table(ty) => format_table_type(ty),
        ExportType::Global(ty) => format_global_type(ty),
    }
}

pub fn format_import_type(ty: ImportType<'_>) -> String {
    match ty {
        ImportType::Func(ty) => format!("func {}", format_func_type(ty)),
        ImportType::Memory(ty) => format_memory_type(ty),
        ImportType::Table(ty) => format_table_type(ty),
        ImportType::Global(ty) => format_global_type(ty),
    }
}
