mod common;

use predicates::prelude::*;

#[test]
fn help_lists_enterprise_subcommands_and_doctor() {
    common::devctl()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("terraform"))
        .stdout(predicate::str::contains("lint"))
        .stdout(predicate::str::contains("validate"))
        .stdout(predicate::str::contains("docker"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("--doctor"))
        .stdout(predicate::str::contains("completions"))
        .stdout(predicate::str::contains("init-config"));
}
