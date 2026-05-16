use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;
use crate::tools::{self, ToolKind, ToolStatus};
use crate::{EXIT_ERROR, EXIT_SUCCESS, docker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintStatus {
    Passed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone)]
pub struct LintItem {
    pub linter: String,
    pub target: Option<PathBuf>,
    pub status: LintStatus,
    pub detail: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct LintReport {
    pub items: Vec<LintItem>,
}

impl LintReport {
    pub fn passed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == LintStatus::Passed)
            .count()
    }

    pub fn skipped_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == LintStatus::Skipped)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == LintStatus::Failed)
            .count()
    }

    pub fn has_failures(&self) -> bool {
        self.failed_count() > 0
    }

    pub fn exit_code(&self) -> i32 {
        if self.has_failures() {
            EXIT_ERROR
        } else {
            EXIT_SUCCESS
        }
    }

    pub fn extend(&mut self, other: LintReport) {
        self.items.extend(other.items);
    }
}

pub fn run(root: &Path, config: &Config, dry_run: bool) -> Result<LintReport> {
    let mut generic = config.linters.clone();
    generic.tools.remove("hadolint");

    let mut report = from_tool_report(tools::run_registry(
        root,
        config,
        &generic,
        ToolKind::Linter,
        dry_run,
    )?);
    report.extend(docker::lint_dockerfiles(root, config, dry_run, false)?);
    Ok(report)
}

pub fn from_tool_report(report: tools::ToolReport) -> LintReport {
    LintReport {
        items: report
            .items
            .into_iter()
            .map(|item| LintItem {
                linter: item.tool,
                target: item.target,
                status: match item.status {
                    ToolStatus::Failed => LintStatus::Failed,
                    ToolStatus::Skipped => LintStatus::Skipped,
                    ToolStatus::Changed
                    | ToolStatus::WouldChange
                    | ToolStatus::Unchanged
                    | ToolStatus::Passed => LintStatus::Passed,
                },
                detail: item.detail,
                suggestion: item.suggestion,
            })
            .collect(),
    }
}
