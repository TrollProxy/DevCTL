mod common;

use std::fs;

use predicates::prelude::*;

#[test]
fn check_workspace_dry_run_reports_multiple_project_roots() {
    let temp = tempfile::tempdir().unwrap();
    let app = temp.path().join("app");
    let infra = temp.path().join("infra");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&infra).unwrap();
    fs::write(app.join("package.json"), "{}\n").unwrap();
    fs::write(infra.join("main.tf"), "terraform {}\n").unwrap();
    fs::write(
        temp.path().join("devctl.yaml"),
        r#"
formatters:
  enabled: false
linters:
  enabled: false
validators:
  enabled: false
"#,
    )
    .unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("--dry-run")
        .arg("check")
        .arg("--workspace")
        .assert()
        .code(0)
        .stdout(predicate::str::contains("app"))
        .stdout(predicate::str::contains("infra"))
        .stdout(predicate::str::contains("npm test"))
        .stdout(predicate::str::contains("terraform plan"));
}
