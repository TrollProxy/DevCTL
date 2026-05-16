mod common;

use std::fs;

use predicates::prelude::*;

#[test]
fn formatter_tool_respects_per_tool_ignore() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("ignored.json");
    fs::write(&file, "{\"b\":1,\"a\":2}").unwrap();
    fs::write(
        temp.path().join("devctl.yaml"),
        r#"
formatters:
  json:
    ignore: ["**/*.json"]
"#,
    )
    .unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("fmt")
        .assert()
        .success()
        .stdout(predicate::str::contains("skipped"));

    assert_eq!(fs::read_to_string(file).unwrap(), "{\"b\":1,\"a\":2}");
}
