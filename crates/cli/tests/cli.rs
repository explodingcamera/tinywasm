use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn write_module(dir: &tempfile::TempDir, name: &str, source: &str) -> String {
    let path = dir.path().join(name);
    fs::write(&path, source).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn run_invoke_accepts_positional_args() {
    let dir = tempdir().unwrap();
    let module = write_module(
        &dir,
        "add.wat",
        r#"(module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add))"#,
    );

    Command::cargo_bin("tinywasm")
        .unwrap()
        .args(["run", "--invoke", "add", &module, "1", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("i32(3)"));
}

#[test]
fn compile_and_run_twasm() {
    let dir = tempdir().unwrap();
    let input = write_module(
        &dir,
        "add.wat",
        r#"(module
            (func (export "add") (param i32 i32) (result i32)
                local.get 0
                local.get 1
                i32.add))"#,
    );
    let output = dir.path().join("add.twasm");

    Command::cargo_bin("tinywasm")
        .unwrap()
        .args(["compile", &input, "-o", output.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin("tinywasm")
        .unwrap()
        .args(["run", "--invoke", "add", output.to_str().unwrap(), "3", "4"])
        .assert()
        .success()
        .stdout(predicate::str::contains("i32(7)"));
}

#[test]
fn bare_run_requires_default_entrypoint() {
    let dir = tempdir().unwrap();
    let module = write_module(&dir, "add.wat", r#"(module (func (export "add") (result i32) i32.const 1))"#);

    Command::cargo_bin("tinywasm")
        .unwrap()
        .arg(&module)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no start function or `_start` export"));
}

#[test]
fn inspect_lists_exports() {
    let dir = tempdir().unwrap();
    let module = write_module(
        &dir,
        "inspect.wat",
        r#"(module
            (memory (export "memory") 1)
            (func (export "answer") (result i32) i32.const 42))"#,
    );

    Command::cargo_bin("tinywasm").unwrap().args(["inspect", &module]).assert().success().stdout(
        predicate::str::contains("answer: func () -> (i32)").and(predicate::str::contains("memory: memory[i32]")),
    );
}

#[test]
fn dump_prints_lowered_instructions() {
    let dir = tempdir().unwrap();
    let module = write_module(&dir, "dump.wat", r#"(module (func (export "noop")))"#);

    Command::cargo_bin("tinywasm")
        .unwrap()
        .args(["dump", &module])
        .assert()
        .success()
        .stdout(predicate::str::contains("func[0]").and(predicate::str::contains("0000:")));
}

#[test]
fn run_accepts_wat_from_stdin() {
    Command::cargo_bin("tinywasm")
        .unwrap()
        .args(["run", "--invoke", "add", "-", "8", "9"])
        .write_stdin(
            r#"(module
                (func (export "add") (param i32 i32) (result i32)
                    local.get 0
                    local.get 1
                    i32.add))"#,
        )
        .assert()
        .success()
        .stdout(predicate::str::contains("i32(17)"));
}

#[test]
fn wast_command_runs_simple_spec_script() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("simple.wast");
    fs::write(
        &path,
        "(module (func (export \"add\") (result i32) i32.const 1))\n(assert_return (invoke \"add\") (i32.const 1))",
    )
    .unwrap();

    Command::cargo_bin("tinywasm")
        .unwrap()
        .args(["wast", path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Tests Passed:"));
}
