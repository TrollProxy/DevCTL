mod common;

use std::{env, fs};

use predicates::prelude::*;

#[test]
fn custom_validator_tool_from_registry_runs_successfully() {
    let temp = tempfile::tempdir().unwrap();
    let devctl = env::var("CARGO_BIN_EXE_devctl").unwrap();
    fs::write(temp.path().join("schema.marker"), "ok\n").unwrap();
    fs::write(
        temp.path().join("devctl.yaml"),
        format!(
            r#"
validators:
  terraform:
    enabled: false
  marker:
    enabled: true
    globs: ["**/*.marker"]
    command: {}
    args: ["--version"]
"#,
            common::yaml_quote(&devctl)
        ),
    )
    .unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("1").and(predicate::str::contains("passed")));
}
