use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use tempfile::TempDir;

use crate::config::Config;
use crate::error::DevctlError;
use crate::fs as fs_util;
use crate::process::{self, CommandSpec};
use crate::{EXIT_CHANGES, EXIT_ERROR, EXIT_SUCCESS};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeSummary {
    pub add: u64,
    pub change: u64,
    pub remove: u64,
}

impl ChangeSummary {
    pub fn has_changes(&self) -> bool {
        self.add > 0 || self.change > 0 || self.remove > 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedChange {
    pub address: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: String,
    pub summary: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlanReport {
    pub root: PathBuf,
    pub summary: ChangeSummary,
    pub changes: Vec<PlannedChange>,
    pub diagnostics: Vec<Diagnostic>,
    pub raw_output: String,
    pub success: bool,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ValidateReport {
    pub root: PathBuf,
    pub valid: bool,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct TestReport {
    pub root: PathBuf,
    pub passed: bool,
    pub detail: String,
}

impl PlanReport {
    pub fn exit_code(&self) -> i32 {
        if !self.success {
            EXIT_ERROR
        } else if self.summary.has_changes() {
            EXIT_CHANGES
        } else {
            EXIT_SUCCESS
        }
    }
}

impl ValidateReport {
    pub fn exit_code(&self) -> i32 {
        if self.valid { EXIT_SUCCESS } else { EXIT_ERROR }
    }
}

pub fn plan(search_from: &Path, config: &Config, _detailed: bool) -> Result<PlanReport> {
    let terraform = resolve_terraform(config)?;
    ensure_version(&terraform, config)?;
    let root = find_terraform_root(search_from, config)?;
    let sandbox = create_sandbox(&root, config)?;

    let init = run_terraform(
        &terraform,
        &["init", "-backend=false", "-input=false", "-no-color"],
        sandbox.path(),
    )?;
    if !init.status.success() {
        return Ok(PlanReport {
            root,
            summary: ChangeSummary::default(),
            changes: Vec::new(),
            diagnostics: vec![Diagnostic {
                severity: "error".to_owned(),
                summary: "terraform init failed".to_owned(),
                detail: Some(first_non_empty(&init.stderr, &init.stdout)),
            }],
            raw_output: format!("{}\n{}", init.stdout, init.stderr),
            success: false,
            suggestion: Some("Check provider configuration and Terraform syntax.".to_owned()),
        });
    }

    let output = run_terraform(
        &terraform,
        &[
            "plan",
            "-json",
            "-detailed-exitcode",
            "-input=false",
            "-refresh=false",
            "-lock=false",
            "-no-color",
        ],
        sandbox.path(),
    )?;
    let parsed = parse_plan_json(&output.stdout);
    let exit_code = output.status.code().unwrap_or(EXIT_ERROR);
    let success = matches!(exit_code, EXIT_SUCCESS | EXIT_CHANGES);

    Ok(PlanReport {
        root,
        summary: parsed.summary,
        changes: parsed.changes,
        diagnostics: if parsed.diagnostics.is_empty() && !success {
            vec![Diagnostic {
                severity: "error".to_owned(),
                summary: "terraform plan failed".to_owned(),
                detail: Some(first_non_empty(&output.stderr, &output.stdout)),
            }]
        } else {
            parsed.diagnostics
        },
        raw_output: format!("{}\n{}", output.stdout, output.stderr),
        success,
        suggestion: (!success).then(|| {
            "Run `terraform validate` locally, then check missing variables/providers.".to_owned()
        }),
    })
}

pub fn validate(search_from: &Path, config: &Config) -> Result<ValidateReport> {
    validate_with_args(search_from, config, &[])
}

pub fn test(search_from: &Path, config: &Config) -> Result<TestReport> {
    let terraform = resolve_terraform(config)?;
    ensure_version(&terraform, config)?;
    let root = find_terraform_root(search_from, config)?;
    let sandbox = create_sandbox(&root, config)?;

    let init = run_terraform(
        &terraform,
        &["init", "-backend=false", "-input=false", "-no-color"],
        sandbox.path(),
    )?;
    if !init.status.success() {
        return Ok(TestReport {
            root,
            passed: false,
            detail: format!(
                "terraform init failed: {}",
                first_non_empty(&init.stderr, &init.stdout)
            ),
        });
    }

    let output = run_terraform(
        &terraform,
        &["test", "-no-color", "-verbose=false"],
        sandbox.path(),
    )?;
    Ok(TestReport {
        root,
        passed: output.status.success(),
        detail: if output.status.success() {
            "passed".to_owned()
        } else {
            first_non_empty(&output.stderr, &output.stdout)
        },
    })
}

pub fn validate_with_args(
    search_from: &Path,
    config: &Config,
    extra_args: &[String],
) -> Result<ValidateReport> {
    let terraform = resolve_terraform(config)?;
    ensure_version(&terraform, config)?;
    let root = find_terraform_root(search_from, config)?;
    let sandbox = create_sandbox(&root, config)?;

    let init = run_terraform(
        &terraform,
        &["init", "-backend=false", "-input=false", "-no-color"],
        sandbox.path(),
    )?;
    if !init.status.success() {
        return Ok(ValidateReport {
            root,
            valid: false,
            diagnostics: vec![Diagnostic {
                severity: "error".to_owned(),
                summary: "terraform init failed".to_owned(),
                detail: Some(first_non_empty(&init.stderr, &init.stdout)),
            }],
        });
    }

    let mut validate_args = vec![
        "validate".to_owned(),
        "-json".to_owned(),
        "-no-color".to_owned(),
    ];
    validate_args.extend(extra_args.iter().cloned());
    let output = run_terraform_owned(&terraform, &validate_args, sandbox.path())?;
    let parsed: TerraformValidateJson =
        serde_json::from_str(&output.stdout).unwrap_or_else(|_| TerraformValidateJson {
            valid: output.status.success(),
            diagnostics: vec![TerraformDiagnosticJson {
                severity: "error".to_owned(),
                summary: "terraform validate failed".to_owned(),
                detail: Some(first_non_empty(&output.stderr, &output.stdout)),
            }],
        });

    Ok(ValidateReport {
        root,
        valid: parsed.valid && output.status.success(),
        diagnostics: parsed
            .diagnostics
            .into_iter()
            .map(Diagnostic::from)
            .collect(),
    })
}

pub fn parse_plan_json(text: &str) -> ParsedPlan {
    let mut parsed = ParsedPlan::default();

    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<TerraformPlanEvent>(line) else {
            continue;
        };

        if let Some(diagnostic) = event.diagnostic {
            parsed.diagnostics.push(diagnostic.into());
        }

        match event.event_type.as_deref() {
            Some("change_summary") => {
                if let Some(changes) = event.changes {
                    parsed.summary = ChangeSummary {
                        add: changes.add,
                        change: changes.change,
                        remove: changes.remove,
                    };
                }
            }
            Some("planned_change") => {
                if let Some(change) = event.change.and_then(|change| change.into_planned_change()) {
                    parsed.changes.push(change);
                }
            }
            _ => {}
        }
    }

    if !parsed.summary.has_changes() && !parsed.changes.is_empty() {
        for change in &parsed.changes {
            match change.action.as_str() {
                "create" => parsed.summary.add += 1,
                "update" | "read" => parsed.summary.change += 1,
                "delete" => parsed.summary.remove += 1,
                "replace" => {
                    parsed.summary.add += 1;
                    parsed.summary.remove += 1;
                }
                _ => {}
            }
        }
    }

    parsed
}

fn resolve_terraform(config: &Config) -> Result<PathBuf> {
    process::which(&config.terraform.binary).ok_or_else(|| {
        DevctlError::MissingTool {
            program: config.terraform.binary.clone(),
            suggestion: "Install Terraform or set terraform.binary in devctl.yaml.".to_owned(),
        }
        .into()
    })
}

fn ensure_version(terraform: &Path, config: &Config) -> Result<()> {
    let Some(expected) = &config.terraform.version else {
        return Ok(());
    };

    let output = run_terraform(terraform, &["version", "-json"], Path::new("."))?;
    if !output.status.success() {
        return Ok(());
    }

    let parsed: serde_json::Value = serde_json::from_str(&output.stdout)?;
    let actual = parsed
        .get("terraform_version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();

    if actual == *expected || actual.starts_with(&format!("{expected}.")) {
        return Ok(());
    }

    Err(DevctlError::TerraformVersion {
        expected: expected.clone(),
        actual,
    }
    .into())
}

fn run_terraform(
    terraform: &Path,
    args: &[&str],
    cwd: &Path,
) -> Result<crate::process::CapturedOutput> {
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    run_terraform_owned(terraform, &args, cwd)
}

fn run_terraform_owned(
    terraform: &Path,
    args: &[String],
    cwd: &Path,
) -> Result<crate::process::CapturedOutput> {
    let mut env = BTreeMap::new();
    env.insert("CHECKPOINT_DISABLE".to_owned(), "1".to_owned());
    env.insert("TF_IN_AUTOMATION".to_owned(), "1".to_owned());
    env.insert("TF_INPUT".to_owned(), "0".to_owned());

    process::capture(&CommandSpec {
        program: terraform.to_path_buf(),
        args: args.to_vec(),
        cwd: Some(cwd.to_path_buf()),
        env,
        sanitized_env: true,
    })
}

fn find_terraform_root(search_from: &Path, config: &Config) -> Result<PathBuf> {
    let start = if search_from.is_file() {
        search_from.parent().unwrap_or(Path::new("."))
    } else {
        search_from
    };
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());

    for ancestor in start.ancestors() {
        if contains_direct_tf_file(ancestor)? {
            return Ok(ancestor.to_path_buf());
        }
    }

    let files = fs_util::discover_files(&start, &config.effective_ignore())?;
    let mut directories: Vec<_> = files
        .into_iter()
        .filter(|file| {
            file.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("tf"))
        })
        .filter_map(|file| file.parent().map(Path::to_path_buf))
        .collect();
    directories.sort_by_key(|path| path.components().count());
    directories.dedup();

    Ok(directories.into_iter().next().unwrap_or(start))
}

fn contains_direct_tf_file(directory: &Path) -> Result<bool> {
    if !directory.is_dir() {
        return Ok(false);
    }

    for entry in fs::read_dir(directory)
        .with_context(|| format!("could not read {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("tf"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn create_sandbox(root: &Path, config: &Config) -> Result<TempDir> {
    let temp = tempfile::Builder::new()
        .prefix("devctl-terraform-")
        .tempdir()?;
    for source in fs_util::discover_files(root, &config.effective_ignore())? {
        if !is_terraform_relevant(&source) {
            continue;
        }
        let relative = source
            .strip_prefix(root)
            .with_context(|| format!("{} is outside {}", source.display(), root.display()))?;
        let destination = temp.path().join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "could not copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }
    Ok(temp)
}

fn is_terraform_relevant(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    matches!(
        extension.as_str(),
        "tf" | "tfvars" | "hcl" | "json" | "tpl" | "tftpl"
    ) && !name.ends_with(".tfstate")
        && !name.ends_with(".tfstate.backup")
}

fn first_non_empty(stderr: &str, stdout: &str) -> String {
    stderr
        .lines()
        .chain(stdout.lines())
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no details were emitted")
        .trim()
        .to_owned()
}

#[derive(Debug, Clone, Default)]
pub struct ParsedPlan {
    pub summary: ChangeSummary,
    pub changes: Vec<PlannedChange>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Deserialize)]
struct TerraformPlanEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    changes: Option<TerraformChangeSummaryJson>,
    change: Option<TerraformPlannedChangeJson>,
    diagnostic: Option<TerraformDiagnosticJson>,
}

#[derive(Debug, Deserialize)]
struct TerraformChangeSummaryJson {
    #[serde(default)]
    add: u64,
    #[serde(default)]
    change: u64,
    #[serde(default)]
    remove: u64,
}

#[derive(Debug, Deserialize)]
struct TerraformPlannedChangeJson {
    resource: Option<TerraformResourceJson>,
    action: Option<String>,
    actions: Option<Vec<String>>,
}

impl TerraformPlannedChangeJson {
    fn into_planned_change(self) -> Option<PlannedChange> {
        let address = self.resource?.addr?;
        let action = self
            .action
            .or_else(|| self.actions.map(|actions| actions.join(",")))?;
        Some(PlannedChange { address, action })
    }
}

#[derive(Debug, Deserialize)]
struct TerraformResourceJson {
    addr: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TerraformDiagnosticJson {
    severity: String,
    summary: String,
    detail: Option<String>,
}

impl From<TerraformDiagnosticJson> for Diagnostic {
    fn from(value: TerraformDiagnosticJson) -> Self {
        Self {
            severity: value.severity,
            summary: value.summary,
            detail: value.detail,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TerraformValidateJson {
    valid: bool,
    #[serde(default)]
    diagnostics: Vec<TerraformDiagnosticJson>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plan_change_summary_and_changes() {
        let json = r#"
{"type":"planned_change","change":{"resource":{"addr":"null_resource.example"},"action":"create"}}
{"type":"change_summary","changes":{"add":1,"change":0,"remove":0,"operation":"plan"}}
"#;

        let parsed = parse_plan_json(json);

        assert_eq!(parsed.summary.add, 1);
        assert_eq!(parsed.changes[0].address, "null_resource.example");
        assert_eq!(parsed.changes[0].action, "create");
    }

    #[test]
    fn terraform_relevant_files_exclude_state() {
        assert!(is_terraform_relevant(Path::new("main.tf")));
        assert!(is_terraform_relevant(Path::new(".terraform.lock.hcl")));
        assert!(!is_terraform_relevant(Path::new("terraform.tfstate")));
    }
}
