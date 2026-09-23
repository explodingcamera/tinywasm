//! Produces binary fixtures for the native C integration test.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::path::PathBuf::from(std::env::args_os().nth(1).expect("output directory"));
    std::fs::create_dir_all(&directory)?;
    let module = wat::parse_str(
        r#"
        (module
          (import "host" "same" (func $a (param i32) (result i32)))
          (import "host" "same" (func $b (param i32) (result i32)))
          (memory (export "memory") 1 2)
          (global (export "global") (mut i32) (i32.const 7))
          (table (export "table") 2 4 funcref)
          (func $inc (export "inc") (param i32) (result i32)
            local.get 0 i32.const 1 i32.add)
          (elem (i32.const 0) func $inc)
          (func (export "run") (param i32) (result i32)
            local.get 0 call $a local.get 0 call $b i32.add)
          (func (export "reenter") (param i32) (result i32) local.get 0 call $a)
          (func (export "trap_host") (param i32) (result i32) local.get 0 call $b)
          (func (export "trap") unreachable))
    "#,
    )?;
    std::fs::write(directory.join("api.wasm"), module)?;
    for (name, source) in [
        ("simd", "(module (func (export \"v\") (result v128) v128.const i32x4 0 0 0 0))"),
        ("memory64", "(module (memory (export \"m\") i64 1))"),
        ("start", "(module (func $start unreachable) (start $start))"),
    ] {
        std::fs::write(directory.join(format!("{name}.wasm")), wat::parse_str(source)?)?;
    }
    Ok(())
}
