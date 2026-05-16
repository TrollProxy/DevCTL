mod common;

use std::fs;

#[test]
fn fmt_dry_run_returns_changes_code_for_json() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("data.json"), "{\"b\":1,\"a\":2}").unwrap();

    common::devctl()
        .current_dir(temp.path())
        .args(["--dry-run", "fmt"])
        .assert()
        .code(2);
}
