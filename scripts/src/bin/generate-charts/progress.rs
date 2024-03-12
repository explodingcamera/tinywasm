use eyre::Result;
use plotters::prelude::*;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

const FONT: &str = "Victor Mono";

pub fn create_progress_chart(name: &str, csv_path: &Path, output_path: &Path) -> Result<()> {
    let file = File::open(csv_path)?;
    let reader = io::BufReader::new(file);

    let mut max_tests = 0;
    let mut data: Vec<u32> = Vec::new();
    let mut versions: Vec<String> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let parts: Vec<&str> = line.split(',').collect();

        if parts.len() > 3 {
            let version = format!("v{}", parts[0]);
            let passed: u32 = parts[1].parse()?;
            let failed: u32 = parts[2].parse()?;
            let total = failed + passed;

            if total > max_tests {
                max_tests = total;
            }

            versions.push(version);
            data.push(passed);
        }
    }

    let root_area = SVGBackend::new(output_path, (1000, 400)).into_drawing_area();
    root_area.fill(&WHITE)?;

    let mut chart = ChartBuilder::on(&root_area)
        .x_label_area_size(45)
        .y_label_area_size(70)
        .margin(10)
        .margin_top(20)
        .caption(name, (FONT, 30.0, FontStyle::Bold))
        .build_cartesian_2d((0..(versions.len() - 1) as u32).into_segmented(), 0..max_tests)?;

    chart
        .configure_mesh()
        .light_line_style(TRANSPARENT)
        .bold_line_style(BLACK.mix(0.3))
        .max_light_lines(10)
        .disable_x_mesh()
        .y_desc("Tests Passed")
        .y_label_style((FONT, 15))
        .x_desc("TinyWasm Version")
        .x_labels((versions.len()).min(4))
        .x_label_style((FONT, 15))
        .x_label_formatter(&|x| {
            let SegmentValue::CenterOf(value) = x else {
                return "".to_string();
            };
            let v = versions.get(*value as usize).unwrap_or(&"".to_string()).to_string();
            format!("{} ({})", v, data[*value as usize])
        })
        .axis_desc_style((FONT, 15, FontStyle::Bold))
        .draw()?;

    chart.draw_series(
        Histogram::vertical(&chart)
            .style(BLUE.mix(0.5).filled())
            .data(data.iter().enumerate().map(|(x, y)| (x as u32, *y))),
    )?;

    root_area.present()?;

    Ok(())
}
