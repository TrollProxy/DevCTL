mod common;

use std::fs;

#[test]
fn fmt_formats_json_files() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("data.json");
    fs::write(&file, "{\"b\":1,\"a\":2}").unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("fmt")
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(file).unwrap(),
        "{\n  \"a\": 2,\n  \"b\": 1\n}\n"
    );
}
