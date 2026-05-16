mod common;

use std::{env, fs};

use predicates::prelude::*;

#[test]
fn custom_validator_failure_returns_error() {
    let temp = tempfile::tempdir().unwrap();
    let devctl = env::var("CARGO_BIN_EXE_devctl").unwrap();
    fs::write(
        temp.path().join("devctl.yaml"),
        format!(
            r#"
validators:
  terraform:
    enabled: false
  custom:
    broken:
      command: {}
      args: ["__definitely_not_a_devctl_command"]
"#,
            common::yaml_quote(&devctl)
        ),
    )
    .unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("validate")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("broken"))
        .stdout(predicate::str::contains("failed"));
}
