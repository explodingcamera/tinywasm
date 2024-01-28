mod charts;
use eyre::Result;

fn main() -> Result<()> {
    generate_charts()
}

fn generate_charts() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 2 || args[1] != "--enable" {
        return Ok(());
    }

    charts::create_progress_chart(
        "WebAssembly 1.0 Test Suite",
        std::path::Path::new("./tests/generated/mvp.csv"),
        std::path::Path::new("./tests/generated/progress-mvp.svg"),
    )?;

    println!("created progress chart: ./tests/generated/progress-mvp.svg");

    charts::create_progress_chart(
        "WebAssembly 2.0 Test Suite",
        std::path::Path::new("./tests/generated/2.0.csv"),
        std::path::Path::new("./tests/generated/progress-2.0.svg"),
    )?;

    println!("created progress chart: ./tests/generated/progress-2.0.svg");

    Ok(())
}
