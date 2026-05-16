use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;

use crate::config::{Config, ToolConfig};
use crate::fs as fs_util;
use crate::lint::{LintItem, LintReport, LintStatus};
use crate::process::{self, CommandSpec};
use crate::tools;

pub fn lint(root: &Path, config: &Config, dry_run: bool, fix: bool) -> Result<LintReport> {
    lint_dockerfiles(root, config, dry_run, fix)
}

pub fn lint_dockerfiles(
    root: &Path,
    config: &Config,
    dry_run: bool,
    fix: bool,
) -> Result<LintReport> {
    if !config.docker.enabled {
        return Ok(LintReport {
            items: vec![LintItem {
                linter: "hadolint".to_owned(),
                target: Some(root.to_path_buf()),
                status: LintStatus::Skipped,
                detail: "Docker linting is disabled".to_owned(),
                suggestion: None,
            }],
        });
    }

    let dockerfiles = discover_dockerfiles(root, config)?;
    if dockerfiles.is_empty() {
        return Ok(LintReport::default());
    }

    let mut items: Vec<_> = dockerfiles
        .par_iter()
        .map(|dockerfile| lint_one(dockerfile, hadolint_config(config), root, dry_run, fix))
        .collect();
    items.sort_by(|left, right| left.target.cmp(&right.target));
    Ok(LintReport { items })
}

pub fn discover_dockerfiles(root: &Path, config: &Config) -> Result<Vec<PathBuf>> {
    let mut files: Vec<_> = fs_util::discover_files(root, &config.effective_ignore())?
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "Dockerfile" || name.starts_with("Dockerfile."))
        })
        .collect();
    files.sort();
    Ok(files)
}

fn lint_one(
    dockerfile: &Path,
    tool: &ToolConfig,
    root: &Path,
    dry_run: bool,
    fix: bool,
) -> LintItem {
    if !tool.enabled {
        return LintItem {
            linter: "hadolint".to_owned(),
            target: Some(dockerfile.to_path_buf()),
            status: LintStatus::Skipped,
            detail: "hadolint is disabled".to_owned(),
            suggestion: None,
        };
    }

    if let Some(program) = process::which(&tool.command) {
        return run_native(program, dockerfile, tool, root, dry_run, fix);
    }

    if tool.docker_fallback
        && let Some(image) = tool.docker_image.as_deref()
        && let Some(docker) = process::which("docker")
    {
        return run_docker_fallback(docker, dockerfile, tool, image, dry_run, fix);
    }

    missing_tool_item(
        "hadolint",
        Some(dockerfile.to_path_buf()),
        &tool.command,
        tool.docker_image.as_deref(),
    )
}

fn run_native(
    program: PathBuf,
    dockerfile: &Path,
    tool: &ToolConfig,
    root: &Path,
    dry_run: bool,
    fix: bool,
) -> LintItem {
    let args = render_args(&tool.args, &tool.extra_args, root, dockerfile);
    if dry_run {
        return LintItem {
            linter: "hadolint".to_owned(),
            target: Some(dockerfile.to_path_buf()),
            status: LintStatus::Skipped,
            detail: format!(
                "dry-run: would run {} {}",
                program.display(),
                args.join(" ")
            ),
            suggestion: None,
        };
    }

    let output = process::capture(&CommandSpec {
        program,
        args,
        cwd: Some(root.to_path_buf()),
        env: Default::default(),
        sanitized_env: false,
    });

    lint_result(dockerfile, output, fix)
}

fn run_docker_fallback(
    docker: PathBuf,
    dockerfile: &Path,
    tool: &ToolConfig,
    image: &str,
    dry_run: bool,
    fix: bool,
) -> LintItem {
    let mut args = vec![
        "run".to_owned(),
        "--rm".to_owned(),
        "-i".to_owned(),
        image.to_owned(),
    ];
    args.extend(tool.extra_args.clone());

    if dry_run {
        return LintItem {
            linter: "hadolint".to_owned(),
            target: Some(dockerfile.to_path_buf()),
            status: LintStatus::Skipped,
            detail: format!("dry-run: would run docker fallback {image}"),
            suggestion: None,
        };
    }

    let input = match fs::read(dockerfile) {
        Ok(input) => input,
        Err(error) => {
            return LintItem {
                linter: "hadolint".to_owned(),
                target: Some(dockerfile.to_path_buf()),
                status: LintStatus::Failed,
                detail: format!("could not read Dockerfile: {error}"),
                suggestion: None,
            };
        }
    };

    let output = process::capture_with_stdin(
        &CommandSpec {
            program: docker,
            args,
            cwd: dockerfile.parent().map(Path::to_path_buf),
            env: Default::default(),
            sanitized_env: false,
        },
        &input,
    );

    lint_result(dockerfile, output, fix)
}

fn lint_result(
    dockerfile: &Path,
    output: Result<crate::process::CapturedOutput>,
    fix: bool,
) -> LintItem {
    match output {
        Ok(output) if output.status.success() => LintItem {
            linter: "hadolint".to_owned(),
            target: Some(dockerfile.to_path_buf()),
            status: LintStatus::Passed,
            detail: fix_detail(fix, "passed"),
            suggestion: None,
        },
        Ok(output) => LintItem {
            linter: "hadolint".to_owned(),
            target: Some(dockerfile.to_path_buf()),
            status: LintStatus::Failed,
            detail: tools::command_failure_detail(&output.stderr, &output.stdout),
            suggestion: None,
        },
        Err(error) => LintItem {
            linter: "hadolint".to_owned(),
            target: Some(dockerfile.to_path_buf()),
            status: LintStatus::Failed,
            detail: error.to_string(),
            suggestion: None,
        },
    }
}

fn hadolint_config(config: &Config) -> &ToolConfig {
    &config.docker.hadolint
}

fn render_args(args: &[String], extra_args: &[String], root: &Path, target: &Path) -> Vec<String> {
    args.iter()
        .chain(extra_args.iter())
        .map(|arg| {
            let relative = target.strip_prefix(root).unwrap_or(target);
            arg.replace("{root}", &root.to_string_lossy())
                .replace("{dir}", &target.to_string_lossy())
                .replace("{file}", &target.to_string_lossy())
                .replace("{relative}", &relative.to_string_lossy())
        })
        .collect()
}

fn missing_tool_item(
    linter: impl Into<String>,
    target: Option<PathBuf>,
    command: &str,
    docker_image: Option<&str>,
) -> LintItem {
    LintItem {
        linter: linter.into(),
        target,
        status: LintStatus::Skipped,
        detail: format!("missing tool: {command}"),
        suggestion: docker_image.map(|image| {
            format!(
                "Install `{command}` natively, or install/start Docker to run `docker run --rm -i {image} < Dockerfile`."
            )
        }),
    }
}

fn fix_detail(fix: bool, detail: &str) -> String {
    if fix {
        format!("{detail}; hadolint does not currently provide automatic fixes")
    } else {
        detail.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn detects_dockerfile_names() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("Dockerfile"), "FROM scratch\n").unwrap();
        fs::write(temp.path().join("Dockerfile.api"), "FROM scratch\n").unwrap();
        fs::write(temp.path().join("not-dockerfile"), "FROM scratch\n").unwrap();

        let files = discover_dockerfiles(temp.path(), &Config::default()).unwrap();

        assert_eq!(files.len(), 2);
    }
}
