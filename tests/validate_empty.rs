mod common;

use predicates::prelude::*;

#[test]
fn validate_skips_when_no_terraform_files_exist() {
    let temp = tempfile::tempdir().unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("no matching files"));
}
