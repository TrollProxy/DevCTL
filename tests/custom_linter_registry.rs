mod common;

use std::{env, fs};

use predicates::prelude::*;

#[test]
fn custom_linter_tool_from_registry_runs_successfully() {
    let temp = tempfile::tempdir().unwrap();
    let devctl = env::var("CARGO_BIN_EXE_devctl").unwrap();
    fs::write(temp.path().join("demo.txt"), "hello\n").unwrap();
    fs::write(
        temp.path().join("devctl.yaml"),
        format!(
            r#"
linters:
  tflint:
    enabled: false
  tfsec:
    enabled: false
  python:
    enabled: false
  custom-text:
    enabled: true
    globs: ["**/*.txt"]
    command: {}
    args: ["--version"]
docker:
  enabled: false
"#,
            common::yaml_quote(&devctl)
        ),
    )
    .unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("1").and(predicate::str::contains("passed")));
}
