use std::env;

const TAIL_CALLS_PROBE: &str = r#"
#![no_std]
#![allow(incomplete_features)]
#![feature(explicit_tail_calls)]

#[inline(never)]
pub fn callee(value: u32) -> u32 { value }
pub fn caller(value: u32) -> u32 { become callee(value); }
"#;

const PRESERVE_NONE_PROBE: &str = r#"
#![no_std]
#![allow(incomplete_features)]
#![feature(explicit_tail_calls, rust_preserve_none_cc)]

#[inline(never)]
pub extern "rust-preserve-none" fn callee(value: u32) -> u32 { value }
pub extern "rust-preserve-none" fn caller(value: u32) -> u32 { become callee(value); }
"#;

const ABORT_IMMEDIATE_PROBE: &str = r#"
#![no_std]
#![feature(abort_immediate)]

pub fn trap() -> ! { core::process::abort_immediate() }
"#;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=RUSTC_BOOTSTRAP");
    configure_simd();
    configure_nightly();
}

fn configure_simd() {
    println!("cargo::rustc-check-cfg=cfg(tw_simd_backend, values(\"generic\", \"wasm\", \"x86\"))");

    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo target architecture");
    let features = env::var("CARGO_CFG_TARGET_FEATURE").unwrap_or_default();
    let x86_v3 = ["sse4.2", "avx", "avx2", "bmi1", "bmi2", "fma", "lzcnt", "movbe", "popcnt"]
        .iter()
        .all(|required| features.split(',').any(|enabled| enabled == *required));
    let simd = match arch.as_str() {
        "wasm32" | "wasm64" => "wasm",
        "x86_64" if env::var_os("CARGO_FEATURE_SIMD_INTRINSICS").is_some() && x86_v3 => "x86",
        _ => "generic",
    };
    println!("cargo::rustc-cfg=tw_simd_backend={simd:?}");
}

fn configure_nightly() {
    println!("cargo::rustc-check-cfg=cfg(tw_abort_immediate)");
    println!("cargo::rustc-check-cfg=cfg(tw_tail_calls)");
    println!(
        "cargo::rustc-check-cfg=cfg(tw_tail_abi, values(\"Rust\", \"C-unwind\", \"sysv64-unwind\", \"rust-preserve-none\"))"
    );
    let mut compiler = autocfg::new();
    compiler.set_edition(Some("2024".into()));
    if env::var_os("CARGO_FEATURE_NIGHTLY_ABORT_IMMEDIATE").is_some()
        && compiler.probe_raw(ABORT_IMMEDIATE_PROBE).is_ok()
    {
        println!("cargo::rustc-cfg=tw_abort_immediate");
    }
    if env::var_os("CARGO_FEATURE_NIGHTLY_TAIL_CALLS").is_none() || compiler.probe_raw(TAIL_CALLS_PROBE).is_err() {
        return;
    }

    let arch = env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo target architecture");
    let width = env::var("CARGO_CFG_TARGET_POINTER_WIDTH").expect("Cargo target pointer width");
    let os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo target OS");
    let preserve_none = env::var_os("CARGO_FEATURE_NIGHTLY_RUST_PRESERVE_NONE").is_some()
        && matches!((arch.as_str(), width.as_str()), ("x86_64", "64") | ("aarch64", "64"))
        && compiler.probe_raw(PRESERVE_NONE_PROBE).is_ok();
    let abi = if preserve_none {
        "rust-preserve-none"
    } else {
        match (arch.as_str(), width.as_str(), os.as_str()) {
            ("aarch64", "32", _) => "C-unwind",
            ("x86_64", _, "windows") => "sysv64-unwind",
            _ => "Rust",
        }
    };
    println!("cargo::rustc-cfg=tw_tail_calls");
    println!("cargo::rustc-cfg=tw_tail_abi={abi:?}");
}
