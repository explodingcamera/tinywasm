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

    // Create a line chart
    charts::create_progress_chart(
        std::path::Path::new("./tests/generated/mvp.csv"),
        std::path::Path::new("./tests/generated/progress-mvp.svg"),
    )?;

    println!("created progress chart: ./tests/generated/progress-mvp.svg");

    Ok(())
}
