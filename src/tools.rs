use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;

use crate::config::{Config, CustomCommand, ToolConfig, ToolRegistryConfig};
use crate::fs as fs_util;
use crate::process::{self, CommandSpec};
use crate::terraform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Formatter,
    Linter,
    Validator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Changed,
    WouldChange,
    Unchanged,
    Passed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ToolItem {
    pub tool: String,
    pub target: Option<PathBuf>,
    pub status: ToolStatus,
    pub detail: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolReport {
    pub items: Vec<ToolItem>,
}

#[derive(Debug, Clone, Copy)]
struct Execution {
    kind: ToolKind,
    dry_run: bool,
}

pub fn run_registry(
    root: &Path,
    config: &Config,
    registry: &ToolRegistryConfig,
    kind: ToolKind,
    dry_run: bool,
) -> Result<ToolReport> {
    if !registry.enabled {
        return Ok(ToolReport {
            items: vec![ToolItem {
                tool: label(kind).to_owned(),
                target: Some(root.to_path_buf()),
                status: ToolStatus::Skipped,
                detail: format!("{} registry is disabled", label(kind)),
                suggestion: None,
            }],
        });
    }

    let mut items: Vec<_> = registry
        .tools
        .par_iter()
        .map(|(name, tool)| run_tool(root, config, name, tool, kind, dry_run))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    let mut legacy_items: Vec<_> = registry
        .custom
        .par_iter()
        .map(|(name, command)| run_custom_command(root, name, command, kind, dry_run))
        .collect();
    items.append(&mut legacy_items);

    items.sort_by(|left, right| {
        left.tool
            .cmp(&right.tool)
            .then_with(|| left.target.cmp(&right.target))
    });

    Ok(ToolReport { items })
}

pub fn run_named_tool(
    root: &Path,
    config: &Config,
    registry: &ToolRegistryConfig,
    name: &str,
    kind: ToolKind,
    dry_run: bool,
) -> Result<ToolReport> {
    let Some(tool) = registry.tools.get(name) else {
        return Ok(ToolReport {
            items: vec![ToolItem {
                tool: name.to_owned(),
                target: Some(root.to_path_buf()),
                status: ToolStatus::Skipped,
                detail: "tool is not configured".to_owned(),
                suggestion: Some(format!("Add `{name}` to devctl.yaml.")),
            }],
        });
    };

    Ok(ToolReport {
        items: run_tool(root, config, name, tool, kind, dry_run)?,
    })
}

pub fn command_failure_detail(stderr: &str, stdout: &str) -> String {
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if detail.is_empty() {
        "command reported issues".to_owned()
    } else {
        detail.lines().take(8).collect::<Vec<_>>().join("\n")
    }
}

fn run_tool(
    root: &Path,
    config: &Config,
    name: &str,
    tool: &ToolConfig,
    kind: ToolKind,
    dry_run: bool,
) -> Result<Vec<ToolItem>> {
    if !tool.enabled {
        return Ok(Vec::new());
    }
    if tool.command.trim().is_empty() {
        return Ok(vec![skipped(name, Some(root), "command is empty", None)]);
    }

    let matches = matching_files(root, config, tool)?;
    if matches.is_empty() {
        return Ok(vec![skipped(name, Some(root), "no matching files", None)]);
    }

    let targets = execution_targets(root, &matches, tool);
    Ok(targets
        .par_iter()
        .map(|target| run_target(root, config, name, tool, kind, dry_run, target))
        .collect())
}

fn run_target(
    root: &Path,
    config: &Config,
    name: &str,
    tool: &ToolConfig,
    kind: ToolKind,
    dry_run: bool,
    target: &Path,
) -> ToolItem {
    let execution = Execution { kind, dry_run };

    if tool.command == "builtin:json" {
        return run_builtin_json(name, kind, dry_run, target);
    }
    if tool.command == "builtin:terraform-validate" {
        return run_builtin_terraform_validate(name, config, dry_run, target, tool);
    }

    let template_args = if kind == ToolKind::Formatter && dry_run && !tool.check_args.is_empty() {
        &tool.check_args
    } else {
        &tool.args
    };

    let Some(program) = process::which(&tool.command) else {
        return run_docker_or_missing(root, config, name, tool, execution, target, template_args);
    };

    if kind == ToolKind::Formatter && dry_run && tool.check_args.is_empty() {
        return skipped(
            name,
            Some(target),
            "dry-run check_args are not configured",
            None,
        );
    }

    if dry_run && kind != ToolKind::Formatter {
        return skipped(
            name,
            Some(target),
            format!(
                "dry-run: would run {} {}",
                program.display(),
                render_args(template_args, &tool.extra_args, root, target).join(" ")
            ),
            None,
        );
    }

    let output = process::capture(&CommandSpec {
        program,
        args: render_args(template_args, &tool.extra_args, root, target),
        cwd: Some(resolve_tool_cwd(root, tool, target)),
        env: tool.env.clone(),
        sanitized_env: false,
    });

    item_from_output(name, target, kind, dry_run, output)
}

fn run_docker_or_missing(
    root: &Path,
    config: &Config,
    name: &str,
    tool: &ToolConfig,
    execution: Execution,
    target: &Path,
    template_args: &[String],
) -> ToolItem {
    let docker_enabled = tool.docker_fallback || config.global.docker_fallback;
    if docker_enabled
        && let Some(image) = tool.docker_image.as_deref()
        && let Some(docker) = process::which("docker")
    {
        let mut args = vec![
            "run".to_owned(),
            "--rm".to_owned(),
            "-v".to_owned(),
            format!("{}:/work", root.display()),
            "-w".to_owned(),
            "/work".to_owned(),
        ];
        args.extend(tool.docker_args.iter().cloned());
        args.push(image.to_owned());
        args.extend(render_container_args(
            template_args,
            &tool.extra_args,
            root,
            target,
        ));

        if execution.dry_run {
            return skipped(
                name,
                Some(target),
                format!("dry-run: would run docker fallback {image}"),
                None,
            );
        }

        let output = process::capture(&CommandSpec {
            program: docker,
            args,
            cwd: Some(root.to_path_buf()),
            env: tool.env.clone(),
            sanitized_env: false,
        });
        return item_from_output(name, target, execution.kind, false, output);
    }

    skipped(
        name,
        Some(target),
        format!("missing tool: {}", tool.command),
        if docker_enabled {
            match tool.docker_image.as_deref() {
                Some(image) => Some(format!(
                    "Install `{}` natively for this environment, or install/start Docker to use `{image}`.",
                    tool.command
                )),
                None => Some(format!(
                    "Install `{}` natively or disable `{name}`.",
                    tool.command
                )),
            }
        } else {
            Some(format!(
                "Install `{}` natively or disable `{name}`.",
                tool.command
            ))
        },
    )
}

fn run_custom_command(
    root: &Path,
    name: &str,
    command: &CustomCommand,
    kind: ToolKind,
    dry_run: bool,
) -> ToolItem {
    if command.command.trim().is_empty() {
        return skipped(name, Some(root), "command is empty", None);
    }

    let Some(program) = process::which(&command.command) else {
        return skipped(
            name,
            Some(root),
            format!("missing tool: {}", command.command),
            Some(format!(
                "Install `{}` or update devctl.yaml.",
                command.command
            )),
        );
    };

    if dry_run {
        return skipped(
            name,
            Some(root),
            format!(
                "dry-run: would run {} {}",
                program.display(),
                command.args.join(" ")
            ),
            None,
        );
    }

    let output = process::capture(&CommandSpec {
        program,
        args: command.args.clone(),
        cwd: Some(process::resolve_cwd(root, &command.cwd)),
        env: command.env.clone(),
        sanitized_env: false,
    });

    item_from_output(name, root, kind, false, output)
}

fn run_builtin_json(name: &str, kind: ToolKind, dry_run: bool, target: &Path) -> ToolItem {
    if kind != ToolKind::Formatter {
        return failed(name, target, "builtin:json can only be used as a formatter");
    }

    match format_json(target, dry_run) {
        Ok(JsonFormatResult::Changed) if dry_run => ToolItem {
            tool: name.to_owned(),
            target: Some(target.to_path_buf()),
            status: ToolStatus::WouldChange,
            detail: "would format".to_owned(),
            suggestion: None,
        },
        Ok(JsonFormatResult::Changed) => ToolItem {
            tool: name.to_owned(),
            target: Some(target.to_path_buf()),
            status: ToolStatus::Changed,
            detail: "formatted".to_owned(),
            suggestion: None,
        },
        Ok(JsonFormatResult::Unchanged) => ToolItem {
            tool: name.to_owned(),
            target: Some(target.to_path_buf()),
            status: ToolStatus::Unchanged,
            detail: "already formatted".to_owned(),
            suggestion: None,
        },
        Err(error) => failed(name, target, error.to_string()),
    }
}

fn run_builtin_terraform_validate(
    name: &str,
    config: &Config,
    dry_run: bool,
    target: &Path,
    tool: &ToolConfig,
) -> ToolItem {
    if dry_run {
        return skipped(
            name,
            Some(target),
            "dry-run: would run terraform validate in an isolated sandbox",
            None,
        );
    }

    match terraform::validate_with_args(target, config, &tool.extra_args) {
        Ok(report) if report.valid => ToolItem {
            tool: name.to_owned(),
            target: Some(report.root),
            status: ToolStatus::Passed,
            detail: "passed".to_owned(),
            suggestion: None,
        },
        Ok(report) => ToolItem {
            tool: name.to_owned(),
            target: Some(report.root),
            status: ToolStatus::Failed,
            detail: report
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    diagnostic
                        .detail
                        .as_ref()
                        .map(|detail| format!("{}: {detail}", diagnostic.summary))
                        .unwrap_or_else(|| diagnostic.summary.clone())
                })
                .collect::<Vec<_>>()
                .join("\n"),
            suggestion: Some("Check Terraform syntax, variables, and providers.".to_owned()),
        },
        Err(error) => failed(name, target, error.to_string()),
    }
}

fn item_from_output(
    name: &str,
    target: &Path,
    kind: ToolKind,
    dry_run: bool,
    output: Result<crate::process::CapturedOutput>,
) -> ToolItem {
    match output {
        Ok(output) if output.status.success() && kind == ToolKind::Formatter && dry_run => {
            let changed = !output.stdout.trim().is_empty();
            ToolItem {
                tool: name.to_owned(),
                target: Some(target.to_path_buf()),
                status: if changed {
                    ToolStatus::WouldChange
                } else {
                    ToolStatus::Unchanged
                },
                detail: if changed {
                    "would format".to_owned()
                } else {
                    "already formatted".to_owned()
                },
                suggestion: None,
            }
        }
        Ok(output) if output.status.success() && kind == ToolKind::Formatter => ToolItem {
            tool: name.to_owned(),
            target: Some(target.to_path_buf()),
            status: ToolStatus::Changed,
            detail: "formatted".to_owned(),
            suggestion: None,
        },
        Ok(output) if output.status.success() => ToolItem {
            tool: name.to_owned(),
            target: Some(target.to_path_buf()),
            status: ToolStatus::Passed,
            detail: "passed".to_owned(),
            suggestion: None,
        },
        Ok(output) if kind == ToolKind::Formatter && dry_run => ToolItem {
            tool: name.to_owned(),
            target: Some(target.to_path_buf()),
            status: ToolStatus::WouldChange,
            detail: command_failure_detail(&output.stderr, &output.stdout),
            suggestion: None,
        },
        Ok(output) => failed(
            name,
            target,
            command_failure_detail(&output.stderr, &output.stdout),
        ),
        Err(error) => failed(name, target, error.to_string()),
    }
}

fn matching_files(root: &Path, config: &Config, tool: &ToolConfig) -> Result<Vec<PathBuf>> {
    let mut ignore = config.effective_ignore();
    ignore.extend(tool.ignore.iter().cloned());
    ignore.sort();
    ignore.dedup();

    let matcher = compile_globs(&tool.globs)?;
    Ok(fs_util::discover_files(root, &ignore)?
        .into_iter()
        .filter(|file| {
            let relative = file.strip_prefix(root).unwrap_or(file);
            matcher.is_match(fs_util::path_to_slash(relative))
        })
        .collect())
}

fn compile_globs(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    if patterns.is_empty() {
        builder.add(Glob::new("**/*")?);
    } else {
        for pattern in patterns {
            builder.add(Glob::new(pattern)?);
        }
    }
    Ok(builder.build()?)
}

fn execution_targets(root: &Path, files: &[PathBuf], tool: &ToolConfig) -> Vec<PathBuf> {
    if tool.command == "builtin:json"
        || contains_placeholder(&tool.args, "{file}")
        || contains_placeholder(&tool.check_args, "{file}")
    {
        return files.to_vec();
    }
    if contains_placeholder(&tool.args, "{dir}") || contains_placeholder(&tool.check_args, "{dir}")
    {
        let mut dirs = BTreeSet::new();
        for file in files {
            if let Some(parent) = file.parent() {
                dirs.insert(parent.to_path_buf());
            }
        }
        return dirs.into_iter().collect();
    }
    vec![root.to_path_buf()]
}

fn contains_placeholder(args: &[String], placeholder: &str) -> bool {
    args.iter().any(|arg| arg.contains(placeholder))
}

fn render_args(args: &[String], extra_args: &[String], root: &Path, target: &Path) -> Vec<String> {
    args.iter()
        .chain(extra_args.iter())
        .map(|arg| render_arg(arg, root, target))
        .collect()
}

fn render_arg(arg: &str, root: &Path, target: &Path) -> String {
    let relative = target.strip_prefix(root).unwrap_or(target);
    arg.replace("{root}", &root.to_string_lossy())
        .replace("{dir}", &target.to_string_lossy())
        .replace("{file}", &target.to_string_lossy())
        .replace("{relative}", &relative.to_string_lossy())
}

fn render_container_args(
    args: &[String],
    extra_args: &[String],
    root: &Path,
    target: &Path,
) -> Vec<String> {
    let relative = target.strip_prefix(root).unwrap_or(target);
    let relative = fs_util::path_to_slash(relative);
    let container_target = if relative.is_empty() {
        "/work".to_owned()
    } else {
        format!("/work/{relative}")
    };

    args.iter()
        .chain(extra_args.iter())
        .map(|arg| {
            arg.replace("{root}", "/work")
                .replace("{dir}", &container_target)
                .replace("{file}", &container_target)
                .replace("{relative}", &relative)
        })
        .collect()
}

fn resolve_tool_cwd(root: &Path, tool: &ToolConfig, target: &Path) -> PathBuf {
    match &tool.cwd {
        Some(cwd) if cwd.is_absolute() => cwd.clone(),
        Some(cwd) => root.join(cwd),
        None if contains_placeholder(&tool.args, "{dir}")
            || contains_placeholder(&tool.check_args, "{dir}") =>
        {
            target.to_path_buf()
        }
        None => root.to_path_buf(),
    }
}

fn format_json(path: &Path, dry_run: bool) -> Result<JsonFormatResult> {
    let original =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&original)
        .with_context(|| format!("invalid JSON in {}", path.display()))?;
    let mut formatted = serde_json::to_string_pretty(&value)?;
    formatted.push('\n');

    if formatted == original {
        return Ok(JsonFormatResult::Unchanged);
    }
    if !dry_run {
        fs::write(path, formatted)
            .with_context(|| format!("could not write {}", path.display()))?;
    }
    Ok(JsonFormatResult::Changed)
}

fn label(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Formatter => "formatter",
        ToolKind::Linter => "linter",
        ToolKind::Validator => "validator",
    }
}

fn skipped(
    name: &str,
    target: Option<&Path>,
    detail: impl Into<String>,
    suggestion: Option<String>,
) -> ToolItem {
    ToolItem {
        tool: name.to_owned(),
        target: target.map(Path::to_path_buf),
        status: ToolStatus::Skipped,
        detail: detail.into(),
        suggestion,
    }
}

fn failed(name: &str, target: &Path, detail: impl Into<String>) -> ToolItem {
    ToolItem {
        tool: name.to_owned(),
        target: Some(target.to_path_buf()),
        status: ToolStatus::Failed,
        detail: detail.into(),
        suggestion: None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JsonFormatResult {
    Changed,
    Unchanged,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_args_use_workdir_paths() {
        let root = Path::new("repo");
        let target = Path::new("repo").join("src").join("main.py");
        let args = render_container_args(&["{file}".to_owned()], &[], root, &target);

        assert_eq!(args, vec!["/work/src/main.py"]);
    }
}
