fn main() {
    if std::env::var_os("CARGO_FEATURE_CUSTOM_PREFIX").is_some() {
        println!("cargo:rerun-if-env-changed=TINYWASM_C_API_PREFIX");
        if std::env::var_os("TINYWASM_C_API_PREFIX").is_none() {
            println!("cargo:rustc-env=TINYWASM_C_API_PREFIX=tinywasm_");
        }
    }
}
