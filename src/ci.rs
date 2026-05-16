use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;
use crate::docker;
use crate::process::{self, CommandSpec};
use crate::project::{self, Project, ProjectKind};
use crate::terraform;
use crate::tools;
use crate::{EXIT_CHANGES, EXIT_ERROR, EXIT_SUCCESS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCheckStatus {
    Passed,
    Skipped,
    Failed,
    Changes,
}

#[derive(Debug, Clone)]
pub struct ProjectCheckItem {
    pub project: PathBuf,
    pub kind: ProjectKind,
    pub check: String,
    pub status: ProjectCheckStatus,
    pub detail: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectCheckReport {
    pub projects: Vec<Project>,
    pub items: Vec<ProjectCheckItem>,
}

impl ProjectCheckReport {
    pub fn passed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ProjectCheckStatus::Passed)
            .count()
    }

    pub fn skipped_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ProjectCheckStatus::Skipped)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ProjectCheckStatus::Failed)
            .count()
    }

    pub fn changes_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ProjectCheckStatus::Changes)
            .count()
    }

    pub fn exit_code(&self) -> i32 {
        if self.failed_count() > 0 {
            EXIT_ERROR
        } else if self.changes_count() > 0 {
            EXIT_CHANGES
        } else {
            EXIT_SUCCESS
        }
    }
}

pub fn run(
    root: &Path,
    config: &Config,
    dry_run: bool,
    workspace: bool,
) -> Result<ProjectCheckReport> {
    let projects = project::discover(root, config, workspace)?;
    let mut items = Vec::new();

    for project in &projects {
        if project.kinds.is_empty() {
            items.push(ProjectCheckItem {
                project: project.root.clone(),
                kind: ProjectKind::Generic,
                check: "detect".to_owned(),
                status: ProjectCheckStatus::Skipped,
                detail: "no known project markers detected".to_owned(),
                suggestion: Some(
                    "Add tools to devctl.yaml, or run from a Terraform, Docker, Rust, Node, Python, Go, or PHP project."
                        .to_owned(),
                ),
            });
            continue;
        }

        for kind in &project.kinds {
            items.extend(run_kind_checks(project, *kind, config, dry_run)?);
        }
    }

    items.sort_by(|left, right| {
        left.project
            .cmp(&right.project)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.check.cmp(&right.check))
    });
    Ok(ProjectCheckReport { projects, items })
}

fn run_kind_checks(
    project: &Project,
    kind: ProjectKind,
    config: &Config,
    dry_run: bool,
) -> Result<Vec<ProjectCheckItem>> {
    let items = match kind {
        ProjectKind::Terraform => terraform_checks(project, config, dry_run)?,
        ProjectKind::Docker => docker_checks(project, config, dry_run)?,
        ProjectKind::Rust => vec![command_check(
            project,
            kind,
            "cargo test",
            "cargo",
            &["test", "--all"],
            dry_run,
        )],
        ProjectKind::Node => vec![command_check(
            project,
            kind,
            "npm test",
            "npm",
            &["test"],
            dry_run,
        )],
        ProjectKind::Python => vec![command_check(
            project,
            kind,
            "pytest",
            "python",
            &["-m", "pytest"],
            dry_run,
        )],
        ProjectKind::Go => vec![command_check(
            project,
            kind,
            "go test",
            "go",
            &["test", "./..."],
            dry_run,
        )],
        ProjectKind::Php => vec![command_check(
            project,
            kind,
            "composer validate",
            "composer",
            &["validate", "--strict"],
            dry_run,
        )],
        ProjectKind::Generic => Vec::new(),
    };
    Ok(items)
}

fn terraform_checks(
    project: &Project,
    config: &Config,
    dry_run: bool,
) -> Result<Vec<ProjectCheckItem>> {
    if dry_run {
        let mut items = vec![
            skipped(
                project,
                ProjectKind::Terraform,
                "terraform validate",
                "dry-run: would run terraform init -backend=false and validate in an isolated sandbox",
            ),
            skipped(
                project,
                ProjectKind::Terraform,
                "terraform plan",
                "dry-run: would run terraform init -backend=false and plan in an isolated sandbox",
            ),
        ];
        if has_terraform_tests(&project.root, config)? {
            items.push(skipped(
                project,
                ProjectKind::Terraform,
                "terraform test",
                "dry-run: would run terraform test in an isolated sandbox",
            ));
        }
        return Ok(items);
    }

    let validate = match terraform::validate(&project.root, config) {
        Ok(report) if report.valid => passed(
            project,
            ProjectKind::Terraform,
            "terraform validate",
            "passed",
        ),
        Ok(report) => ProjectCheckItem {
            project: project.root.clone(),
            kind: ProjectKind::Terraform,
            check: "terraform validate".to_owned(),
            status: ProjectCheckStatus::Failed,
            detail: report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.summary.clone())
                .collect::<Vec<_>>()
                .join("\n"),
            suggestion: Some("Fix Terraform validation diagnostics before planning.".to_owned()),
        },
        Err(error) => failed(
            project,
            ProjectKind::Terraform,
            "terraform validate",
            error.to_string(),
        ),
    };

    let plan = match terraform::plan(&project.root, config, false) {
        Ok(report) if report.success && report.summary.has_changes() => ProjectCheckItem {
            project: project.root.clone(),
            kind: ProjectKind::Terraform,
            check: "terraform plan".to_owned(),
            status: ProjectCheckStatus::Changes,
            detail: format!(
                "plan has changes: +{} ~{} -{}",
                report.summary.add, report.summary.change, report.summary.remove
            ),
            suggestion: Some("Review the Terraform plan before merging.".to_owned()),
        },
        Ok(report) if report.success => passed(
            project,
            ProjectKind::Terraform,
            "terraform plan",
            "no changes",
        ),
        Ok(report) => failed(
            project,
            ProjectKind::Terraform,
            "terraform plan",
            report
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.summary.clone())
                .unwrap_or_else(|| "terraform plan failed".to_owned()),
        ),
        Err(error) => failed(
            project,
            ProjectKind::Terraform,
            "terraform plan",
            error.to_string(),
        ),
    };

    let mut items = vec![validate, plan];
    if has_terraform_tests(&project.root, config)? {
        items.push(match terraform::test(&project.root, config) {
            Ok(report) if report.passed => {
                passed(project, ProjectKind::Terraform, "terraform test", "passed")
            }
            Ok(report) => failed(
                project,
                ProjectKind::Terraform,
                "terraform test",
                report.detail,
            ),
            Err(error) => failed(
                project,
                ProjectKind::Terraform,
                "terraform test",
                error.to_string(),
            ),
        });
    }
    Ok(items)
}

fn docker_checks(
    project: &Project,
    config: &Config,
    dry_run: bool,
) -> Result<Vec<ProjectCheckItem>> {
    let dockerfiles = docker::discover_dockerfiles(&project.root, config)?;
    if dockerfiles.is_empty() {
        return Ok(vec![skipped(
            project,
            ProjectKind::Docker,
            "docker build",
            "no Dockerfile found",
        )]);
    }

    Ok(dockerfiles
        .into_iter()
        .map(|dockerfile| docker_build(project, &dockerfile, dry_run))
        .collect())
}

fn docker_build(project: &Project, dockerfile: &Path, dry_run: bool) -> ProjectCheckItem {
    let check = format!(
        "docker build {}",
        dockerfile
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Dockerfile")
    );

    if dry_run {
        return ProjectCheckItem {
            project: project.root.clone(),
            kind: ProjectKind::Docker,
            check,
            status: ProjectCheckStatus::Skipped,
            detail: format!(
                "dry-run: would run docker build --pull=false -f {} {}",
                dockerfile.display(),
                project.root.display()
            ),
            suggestion: None,
        };
    }

    let Some(docker) = process::which("docker") else {
        return ProjectCheckItem {
            project: project.root.clone(),
            kind: ProjectKind::Docker,
            check,
            status: ProjectCheckStatus::Skipped,
            detail: "Docker CLI is not installed".to_owned(),
            suggestion: Some(
                "Install Docker, or use native language checks configured in devctl.yaml."
                    .to_owned(),
            ),
        };
    };

    let args = vec![
        "build".to_owned(),
        "--pull=false".to_owned(),
        "-f".to_owned(),
        dockerfile.to_string_lossy().to_string(),
        project.root.to_string_lossy().to_string(),
    ];

    output_check(
        project,
        ProjectKind::Docker,
        check,
        docker,
        args,
        BTreeMap::new(),
    )
}

fn command_check(
    project: &Project,
    kind: ProjectKind,
    check: &str,
    command: &str,
    args: &[&str],
    dry_run: bool,
) -> ProjectCheckItem {
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    if dry_run {
        return ProjectCheckItem {
            project: project.root.clone(),
            kind,
            check: check.to_owned(),
            status: ProjectCheckStatus::Skipped,
            detail: format!("dry-run: would run {command} {}", args.join(" ")),
            suggestion: None,
        };
    }

    let Some(program) = process::which(command) else {
        return ProjectCheckItem {
            project: project.root.clone(),
            kind,
            check: check.to_owned(),
            status: ProjectCheckStatus::Skipped,
            detail: format!("missing tool: {command}"),
            suggestion: Some(format!(
                "Install `{command}`, disable this check in devctl.yaml, or rely on another configured tool."
            )),
        };
    };

    output_check(project, kind, check, program, args, BTreeMap::new())
}

fn output_check(
    project: &Project,
    kind: ProjectKind,
    check: impl Into<String>,
    program: PathBuf,
    args: Vec<String>,
    env: BTreeMap<String, String>,
) -> ProjectCheckItem {
    let check = check.into();
    let output = process::capture(&CommandSpec {
        program,
        args,
        cwd: Some(project.root.clone()),
        env,
        sanitized_env: false,
    });

    match output {
        Ok(output) if output.status.success() => passed(project, kind, &check, "passed"),
        Ok(output) => failed(
            project,
            kind,
            &check,
            tools::command_failure_detail(&output.stderr, &output.stdout),
        ),
        Err(error) => failed(project, kind, &check, error.to_string()),
    }
}

fn has_terraform_tests(root: &Path, config: &Config) -> Result<bool> {
    Ok(crate::fs::discover_files(root, &config.effective_ignore())?
        .into_iter()
        .any(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".tftest.hcl"))
        }))
}

fn passed(
    project: &Project,
    kind: ProjectKind,
    check: impl Into<String>,
    detail: impl Into<String>,
) -> ProjectCheckItem {
    ProjectCheckItem {
        project: project.root.clone(),
        kind,
        check: check.into(),
        status: ProjectCheckStatus::Passed,
        detail: detail.into(),
        suggestion: None,
    }
}

fn skipped(
    project: &Project,
    kind: ProjectKind,
    check: impl Into<String>,
    detail: impl Into<String>,
) -> ProjectCheckItem {
    ProjectCheckItem {
        project: project.root.clone(),
        kind,
        check: check.into(),
        status: ProjectCheckStatus::Skipped,
        detail: detail.into(),
        suggestion: None,
    }
}

fn failed(
    project: &Project,
    kind: ProjectKind,
    check: impl Into<String>,
    detail: impl Into<String>,
) -> ProjectCheckItem {
    ProjectCheckItem {
        project: project.root.clone(),
        kind,
        check: check.into(),
        status: ProjectCheckStatus::Failed,
        detail: detail.into(),
        suggestion: None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn dry_run_detects_terraform_and_docker_checks() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("main.tf"), "terraform {}\n").unwrap();
        fs::write(temp.path().join("Dockerfile"), "FROM scratch\n").unwrap();

        let report = run(temp.path(), &Config::default(), true, false).unwrap();

        assert!(
            report
                .items
                .iter()
                .any(|item| item.check == "terraform plan")
        );
        assert!(
            report
                .items
                .iter()
                .any(|item| item.check == "docker build Dockerfile")
        );
    }
}
