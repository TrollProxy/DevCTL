mod common;

use std::{env, fs};

use predicates::prelude::*;

#[test]
fn native_tool_is_used_even_when_docker_fallback_is_configured() {
    let temp = tempfile::tempdir().unwrap();
    let devctl = env::var("CARGO_BIN_EXE_devctl").unwrap();
    fs::write(temp.path().join("demo.txt"), "hello\n").unwrap();
    fs::write(
        temp.path().join("devctl.yaml"),
        format!(
            r#"
global:
  docker_fallback: true
linters:
  tflint:
    enabled: false
  tfsec:
    enabled: false
  python:
    enabled: false
  native-demo:
    enabled: true
    globs: ["**/*.txt"]
    command: {}
    args: ["--version"]
    docker_fallback: true
    docker_image: example/missing
docker:
  enabled: false
"#,
            common::yaml_quote(&devctl)
        ),
    )
    .unwrap();

    common::devctl()
        .current_dir(temp.path())
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}
