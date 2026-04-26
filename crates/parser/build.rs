use std::env;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(parallel_parser)");

    if env::var("CARGO_FEATURE_PARALLEL").is_err() {
        return;
    }

    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if !arch.starts_with("wasm") && os != "unknown" && os != "none" {
        println!("cargo:rustc-cfg=parallel_parser");
    }
}
