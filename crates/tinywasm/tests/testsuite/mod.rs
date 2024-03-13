#![allow(dead_code)] // rust analyzer doesn't recognize that code is used by tests without harness

use _log as log;
use eyre::Result;
use owo_colors::OwoColorize;
use std::io::{BufRead, Seek, SeekFrom};
use std::{
    collections::BTreeMap,
    fmt::{Debug, Formatter},
    io::BufReader,
};

mod indexmap;
mod run;
mod util;

use serde::{Deserialize, Serialize};

use self::indexmap::IndexMap;

#[derive(Serialize, Deserialize)]
pub struct TestGroupResult {
    pub name: String,
    pub passed: usize,
    pub failed: usize,
}

fn format_linecol(linecol: (usize, usize)) -> String {
    format!("{}:{}", linecol.0 + 1, linecol.1 + 1)
}

pub struct TestSuite(BTreeMap<String, TestGroup>, Vec<String>);

impl TestSuite {
    pub fn skip(&mut self, groups: &[&str]) {
        self.1.extend(groups.iter().map(|s| s.to_string()));
    }

    pub fn set_log_level(level: log::LevelFilter) {
        pretty_env_logger::formatted_builder().filter_level(level).init();
    }

    pub fn print_errors(&self) {
        for (group_name, group) in &self.0 {
            let tests = &group.tests;
            for (test_name, test) in tests.iter() {
                if let Err(e) = &test.result {
                    eprintln!(
                        "{} {} failed: {:?}",
                        link(group_name, &group.file, Some(test.linecol.0 + 1)).bold().underline(),
                        test_name.bold(),
                        e.to_string().bright_red()
                    );
                }
            }
        }
    }

    pub fn new() -> Self {
        Self(BTreeMap::new(), Vec::new())
    }

    pub fn failed(&self) -> bool {
        self.0.values().any(|group| group.stats().1 > 0)
    }

    fn test_group(&mut self, name: &str, file: &str) -> &mut TestGroup {
        self.0.entry(name.to_string()).or_insert_with(|| TestGroup::new(file))
    }

    // create or add to a test result file
    pub fn save_csv(&self, path: &str, version: &str) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new().create(true).append(true).read(true).open(path)?;
        let last_line = BufReader::new(&file).lines().last().transpose()?;

        // Check if the last line starts with the current commit
        if let Some(last) = last_line {
            if last.starts_with(version) {
                // Truncate the file size to remove the last line
                let len_to_truncate = last.len() as i64;
                file.set_len(file.metadata()?.len() - len_to_truncate as u64 - 1)?;
            }
        }

        // Seek to the end of the file for appending
        file.seek(SeekFrom::End(0))?;

        let mut passed = 0;
        let mut failed = 0;

        let mut groups = Vec::new();
        for (name, group) in self.0.iter() {
            let (group_passed, group_failed) = group.stats();
            passed += group_passed;
            failed += group_failed;

            groups.push(TestGroupResult { name: name.to_string(), passed: group_passed, failed: group_failed });
        }

        let groups = serde_json::to_string(&groups)?;
        let line = format!("{},{},{},{}\n", version, passed, failed, groups);
        file.write_all(line.as_bytes()).expect("failed to write to csv file");

        Ok(())
    }
}

fn link(name: &str, file: &str, line: Option<usize>) -> String {
    let (path, name) = match line {
        None => (file.to_string(), name.to_owned()),
        Some(line) => (format!("{}:{}:0", file, line), (format!("{}:{}", name, line))),
    };

    format!("\x1b]8;;file://{}\x1b\\{}\x1b]8;;\x1b\\", path, name)
}

impl Debug for TestSuite {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut total_passed = 0;
        let mut total_failed = 0;

        for (group_name, group) in &self.0 {
            let (group_passed, group_failed) = group.stats();
            total_passed += group_passed;
            total_failed += group_failed;

            writeln!(f, "{}", link(group_name, &group.file, None).bold().underline())?;
            writeln!(f, "  Tests Passed: {}", group_passed.to_string().green())?;

            if group_failed != 0 {
                writeln!(f, "  Tests Failed: {}", group_failed.to_string().red())?;
            }
        }

        writeln!(f, "\n{}", "Total Test Summary:".bold().underline())?;
        writeln!(f, "  Total Tests: {}", (total_passed + total_failed))?;
        writeln!(f, "  Total Passed: {}", total_passed.to_string().green())?;
        writeln!(f, "  Total Failed: {}", total_failed.to_string().red())?;
        Ok(())
    }
}

struct TestGroup {
    tests: IndexMap<String, TestCase>,
    file: String,
}

impl TestGroup {
    fn new(file: &str) -> Self {
        Self { tests: IndexMap::new(), file: file.to_string() }
    }

    fn stats(&self) -> (usize, usize) {
        let mut passed_count = 0;
        let mut failed_count = 0;

        for test in self.tests.values() {
            match test.result {
                Ok(()) => passed_count += 1,
                Err(_) => failed_count += 1,
            }
        }

        (passed_count, failed_count)
    }

    fn add_result(&mut self, name: &str, linecol: (usize, usize), result: Result<()>) {
        self.tests.insert(name.to_string(), TestCase { result, linecol });
    }
}

#[derive(Debug)]
struct TestCase {
    result: Result<()>,
    linecol: (usize, usize),
}
