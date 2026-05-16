mod common;

use std::fs;

use predicates::prelude::*;

#[test]
fn check_dry_run_returns_changes_code_for_json() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data.json"), "{\"b\":1,\"a\":2}").unwrap();

    common::devctl()
        .current_dir(temp.path())
        .args(["--dry-run", "check"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("fmt"))
        .stdout(predicate::str::contains("lint"))
        .stdout(predicate::str::contains("validate"));
}
