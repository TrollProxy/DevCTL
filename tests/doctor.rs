mod common;

use predicates::prelude::*;

#[test]
fn doctor_runs_and_prints_system_table() {
    common::devctl()
        .arg("--doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("devctl doctor"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("tool"))
        .stdout(predicate::str::contains("checks"));
}
