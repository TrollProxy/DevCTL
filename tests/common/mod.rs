use assert_cmd::Command;

pub fn devctl() -> Command {
    Command::cargo_bin("devctl").unwrap()
}

#[allow(dead_code)]
pub fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
