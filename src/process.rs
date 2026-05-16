use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub sanitized_env: bool,
}

#[derive(Debug, Clone)]
pub struct CapturedOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            sanitized_env: false,
        }
    }
}

pub fn capture(spec: &CommandSpec) -> Result<CapturedOutput> {
    let mut command = build_command(spec);
    let output = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to execute {}", spec.program.display()))?;

    Ok(CapturedOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn capture_with_stdin(spec: &CommandSpec, stdin: &[u8]) -> Result<CapturedOutput> {
    let mut command = build_command(spec);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to execute {}", spec.program.display()))?;

    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin
            .write_all(stdin)
            .with_context(|| format!("failed to write stdin for {}", spec.program.display()))?;
    }

    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to wait for {}", spec.program.display()))?;

    Ok(CapturedOutput {
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

pub fn status(spec: &CommandSpec) -> Result<ExitStatus> {
    build_command(spec)
        .status()
        .with_context(|| format!("failed to execute {}", spec.program.display()))
}

pub fn which(program: &str) -> Option<PathBuf> {
    which::which(program).ok()
}

pub fn resolve_cwd(cwd: &Path, configured: &Option<PathBuf>) -> PathBuf {
    match configured {
        Some(path) if path.is_absolute() => path.clone(),
        Some(path) => cwd.join(path),
        None => cwd.to_path_buf(),
    }
}

fn build_command(spec: &CommandSpec) -> Command {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);

    if spec.sanitized_env {
        command.env_clear();
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }

    command
}
