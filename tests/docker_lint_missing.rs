mod common;

use std::fs;

use predicates::prelude::*;

#[test]
fn docker_lint_skips_missing_hadolint_without_fallback() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("Dockerfile"), "FROM scratch\n").unwrap();
    fs::write(
        temp.path().join("devctl.yaml"),
        r#"
docker:
  hadolint:
    command: definitely-missing-hadolint
    docker_fallback: false
"#,
    )
    .unwrap();

    common::devctl()
        .current_dir(temp.path())
        .args(["docker", "lint"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hadolint"))
        .stdout(predicate::str::contains("skipped"));
}
