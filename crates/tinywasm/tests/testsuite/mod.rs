#![allow(dead_code)]

use eyre::{Result, eyre};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use tinywasm_cli::wast_runner::{GroupResult, TestFile as RunnerTestFile, WastRunner};

#[derive(Serialize, Deserialize)]
pub struct TestGroupResult {
    pub name: String,
    pub passed: usize,
    pub failed: usize,
}

pub struct TestSuite {
    runner: WastRunner,
}

impl TestSuite {
    pub fn set_log_level(level: log::LevelFilter) {
        WastRunner::set_log_level(level);
    }

    pub fn new() -> Self {
        Self { runner: WastRunner::new() }
    }

    pub fn run_paths(&mut self, tests: &[std::path::PathBuf]) -> Result<()> {
        let mut files = Vec::new();
        for path in tests {
            if path.is_dir() {
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "wast") {
                        files.push(path);
                    }
                }
            } else {
                files.push(path.clone());
            }
        }
        files.sort();

        let runner_files = files
            .iter()
            .map(|path| {
                let contents = std::fs::read_to_string(path)?;
                let name = path.to_string_lossy().into_owned();
                Ok((name, contents))
            })
            .collect::<Result<Vec<_>>>()?;

        self.runner.run_files(runner_files.iter().map(|(name, contents)| RunnerTestFile {
            name: name.clone(),
            parent: name.clone(),
            contents,
        }))
    }

    pub fn run_files<'a>(&mut self, tests: impl IntoIterator<Item = wasm_testsuite::data::TestFile<'a>>) -> Result<()> {
        self.runner.run_files(tests.into_iter().map(|file| RunnerTestFile {
            name: file.name().to_string(),
            parent: file.parent().to_string(),
            contents: file.raw(),
        }))
    }

    pub fn print_errors(&self) {
        self.runner.print_errors();
    }

    pub fn report_status(&self) -> Result<()> {
        if self.runner.failed() {
            println!();
            Err(eyre!(format!("{}:\n{self}", "failed one or more tests".red().bold())))
        } else {
            println!("{self}");
            Ok(())
        }
    }

    pub fn save_csv(&self, path: &str, version: &str) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new().create(true).append(true).read(true).open(path)?;
        let last_line = BufReader::new(&file).lines().last().transpose()?;

        if let Some(last) = last_line
            && last.starts_with(version)
        {
            let len_to_truncate = last.len() as i64;
            file.set_len(file.metadata()?.len() - len_to_truncate as u64 - 1)?;
        }

        file.seek(SeekFrom::End(0))?;

        let mut passed = 0;
        let mut failed = 0;
        let mut groups = Vec::new();

        for group in self.runner.group_results() {
            passed += group.passed;
            failed += group.failed;
            groups.push(TestGroupResult { name: group.name, passed: group.passed, failed: group.failed });
        }

        let groups = serde_json::to_string(&groups)?;
        let line = format!("{version},{passed},{failed},{groups}\n");
        file.write_all(line.as_bytes())?;
        Ok(())
    }

    fn group_results(&self) -> Vec<GroupResult> {
        self.runner.group_results()
    }
}

impl Default for TestSuite {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for TestSuite {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut total_passed = 0;
        let mut total_failed = 0;

        for group in self.group_results() {
            total_passed += group.passed;
            total_failed += group.failed;

            writeln!(f, "{}", group.name.bold().underline())?;
            writeln!(f, "  Tests Passed: {}", group.passed.to_string().green())?;
            if group.failed != 0 {
                writeln!(f, "  Tests Failed: {}", group.failed.to_string().red())?;
            }
        }

        writeln!(f, "\n{}", "Total Test Summary:".bold().underline())?;
        writeln!(f, "  Total Tests: {}", total_passed + total_failed)?;
        writeln!(f, "  Total Passed: {}", total_passed.to_string().green())?;
        writeln!(f, "  Total Failed: {}", total_failed.to_string().red())?;
        Ok(())
    }
}
