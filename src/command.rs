use std::path::Path;
use std::process::ExitStatus;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::error::DevctlError;
use crate::output::Reporter;
use crate::process::{self, CommandSpec};
use crate::{EXIT_ERROR, EXIT_SUCCESS};

pub fn run_custom(
    name: &str,
    extra_args: &[String],
    config: &Config,
    cwd: &Path,
    dry_run: bool,
    reporter: &Reporter,
) -> Result<i32> {
    let command = config
        .commands
        .get(name)
        .ok_or_else(|| DevctlError::UnknownCommand(name.to_owned()))?;

    if command.command.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "custom command `{name}` has an empty command"
        ));
    }

    let Some(program) = process::which(&command.command) else {
        return Err(DevctlError::MissingTool {
            program: command.command.clone(),
            suggestion: format!(
                "Install `{}` or update commands.{name}.command.",
                command.command
            ),
        }
        .into());
    };

    let mut args = command.args.clone();
    args.extend(extra_args.iter().cloned());
    let cwd = process::resolve_cwd(cwd, &command.cwd);

    if dry_run {
        reporter.info(format!(
            "dry-run: {} {}",
            program.display(),
            args.iter()
                .map(|arg| shell_display(arg))
                .collect::<Vec<_>>()
                .join(" ")
        ));
        return Ok(EXIT_SUCCESS);
    }

    let status = process::status(&CommandSpec {
        program,
        args,
        cwd: Some(cwd),
        env: command.env.clone(),
        sanitized_env: false,
    })
    .with_context(|| format!("failed to run custom command `{name}`"))?;

    Ok(exit_code(status))
}

fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(EXIT_ERROR)
}

fn shell_display(arg: &str) -> String {
    if arg
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_./:=@".contains(character))
    {
        arg.to_owned()
    } else {
        format!("{arg:?}")
    }
}
