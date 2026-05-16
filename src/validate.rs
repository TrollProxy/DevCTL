use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;
use crate::tools::{self, ToolKind, ToolStatus};
use crate::{EXIT_ERROR, EXIT_SUCCESS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStatus {
    Passed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone)]
pub struct ValidationItem {
    pub validator: String,
    pub target: Option<PathBuf>,
    pub status: ValidationStatus,
    pub detail: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub items: Vec<ValidationItem>,
}

impl ValidationReport {
    pub fn passed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ValidationStatus::Passed)
            .count()
    }

    pub fn skipped_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ValidationStatus::Skipped)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == ValidationStatus::Failed)
            .count()
    }

    pub fn exit_code(&self) -> i32 {
        if self.failed_count() > 0 {
            EXIT_ERROR
        } else {
            EXIT_SUCCESS
        }
    }
}

pub fn run(root: &Path, config: &Config, dry_run: bool) -> Result<ValidationReport> {
    from_tool_report(tools::run_registry(
        root,
        config,
        &config.validators,
        ToolKind::Validator,
        dry_run,
    )?)
}

fn from_tool_report(report: tools::ToolReport) -> Result<ValidationReport> {
    Ok(ValidationReport {
        items: report
            .items
            .into_iter()
            .map(|item| ValidationItem {
                validator: item.tool,
                target: item.target,
                status: match item.status {
                    ToolStatus::Failed => ValidationStatus::Failed,
                    ToolStatus::Skipped => ValidationStatus::Skipped,
                    ToolStatus::Changed
                    | ToolStatus::WouldChange
                    | ToolStatus::Unchanged
                    | ToolStatus::Passed => ValidationStatus::Passed,
                },
                detail: item.detail,
                suggestion: item.suggestion,
            })
            .collect(),
    })
}
