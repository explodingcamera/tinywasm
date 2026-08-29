use tinywasm_cli::wast_runner::WastRunner;

fn main() -> Result<(), Box<dyn core::error::Error>> {
    let Some(input) = std::env::args().nth(1) else {
        eprintln!("usage: cargo test-wast <wast-file>");
        std::process::exit(2);
    };

    let mut cwd = std::env::current_dir()?;
    if cwd.ends_with("crates/tinywasm/") {
        cwd.pop();
        cwd.pop();
    }

    let arg = cwd.join(input);
    println!("running tests in {:?}", arg);

    let mut test_suite = WastRunner::new();
    test_suite.run_paths(&[arg])?;
    Ok(())
}
