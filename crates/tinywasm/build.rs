use std::env;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!(
        "cargo::rustc-check-cfg=cfg(tail_abi, values(\"Rust\", \"C-unwind\", \"sysv64-unwind\", \"rust-preserve-none\"))"
    );
    println!("cargo::rustc-check-cfg=cfg(simd_backend, values(\"generic\", \"wasm\", \"x86\"))");

    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo target architecture");
    let features = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    let x86_v3 = ["sse4.2", "avx", "avx2", "bmi1", "bmi2", "fma", "lzcnt", "movbe", "popcnt"]
        .iter()
        .all(|required| features.split(',').any(|enabled| enabled == *required));
    let simd = match arch.as_str() {
        "wasm32" | "wasm64" => "wasm",
        "x86_64" if env::var_os("CARGO_FEATURE_SIMD_X86").is_some() && x86_v3 => "x86",
        _ => "generic",
    };
    println!("cargo::rustc-cfg=simd_backend={simd:?}");

    if env::var_os("CARGO_FEATURE_NIGHTLY_TAIL_CALLS").is_none() {
        return;
    }

    let width = env::var("CARGO_CFG_TARGET_POINTER_WIDTH").expect("Cargo target pointer width");
    let os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo target OS");
    let preserve_none = env::var_os("CARGO_FEATURE_NIGHTLY_RUST_PRESERVE_NONE").is_some();
    let abi = match (arch.as_str(), width.as_str(), os.as_str(), preserve_none) {
        ("aarch64", "32", _, _) => "C-unwind",
        ("x86_64", _, _, true) | ("aarch64", "64", _, true) => "rust-preserve-none",
        ("x86_64", _, "windows", _) => "sysv64-unwind",
        _ => "Rust",
    };
    println!("cargo::rustc-cfg=tail_abi={abi:?}");
}
