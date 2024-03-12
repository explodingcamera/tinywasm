mod progress;
use std::{path::PathBuf, str::FromStr};

use eyre::Result;

fn main() -> Result<()> {
    generate_charts()
}

fn generate_charts() -> Result<()> {
    let results_dir = PathBuf::from_str("../crates/tinywasm/tests/generated")?;

    // check if the folder exists
    if !results_dir.exists() {
        return Err(eyre::eyre!(
            "This script should be run from the root of the project, and the test results should be generated first."
        ));
    }

    progress::create_progress_chart(
        "WebAssembly 1.0 Test Suite",
        &results_dir.join("mvp.csv"),
        &results_dir.join("progress-mvp.svg"),
    )?;
    println!("created progress chart: {}", results_dir.join("progress-mvp.svg").display());

    progress::create_progress_chart(
        "WebAssembly 2.0 Test Suite",
        &results_dir.join("2.0.csv"),
        &results_dir.join("progress-2.0.svg"),
    )?;
    println!("created progress chart: {}", results_dir.join("progress-2.0.svg").display());

    Ok(())
}
