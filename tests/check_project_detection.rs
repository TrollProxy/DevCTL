mod common;

use std::fs;

use predicates::prelude::*;

#[test]
fn check_dry_run_reports_project_specific_checks() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("main.tf"), "terraform {}\n").unwrap();
    fs::write(temp.path().join("Dockerfile"), "FROM scratch\n").unwrap();
    fs::write(
        temp.path().join("devctl.yaml"),
        r#"
formatters:
  enabled: false
linters:
  enabled: false
validators:
  enabled: false
docker:
  enabled: false
"#,
    )
    .unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("--dry-run")
        .arg("check")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("devctl project checks"))
        .stdout(predicate::str::contains("terraform plan"))
        .stdout(predicate::str::contains("docker build Dockerfile"));
}
