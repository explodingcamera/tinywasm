mod charts;
use eyre::Result;

fn main() -> Result<()> {
    generate_charts()
}

fn generate_charts() -> Result<()> {
    // Create a line chart
    charts::create_progress_chart(
        std::path::Path::new("./tests/generated/mvp.csv"),
        std::path::Path::new("./tests/generated/progress-mvp.svg"),
    )?;

    Ok(())
}
