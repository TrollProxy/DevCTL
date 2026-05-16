mod common;

use std::fs;

#[test]
fn init_config_writes_example() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("devctl.yaml");

    common::devctl()
        .current_dir(temp.path())
        .arg("init-config")
        .assert()
        .success();

    assert!(path.is_file());
    assert!(fs::read_to_string(path).unwrap().contains("commands:"));
}
