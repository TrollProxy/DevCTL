use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;
use crate::tools::{self, ToolKind, ToolStatus};
use crate::{EXIT_CHANGES, EXIT_ERROR, EXIT_SUCCESS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatStatus {
    Fixed,
    WouldFix,
    Unchanged,
    Skipped,
    Failed,
}

#[derive(Debug, Clone)]
pub struct FormatItem {
    pub path: PathBuf,
    pub formatter: String,
    pub status: FormatStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Default)]
pub struct FormatReport {
    pub items: Vec<FormatItem>,
}

impl FormatReport {
    pub fn fixed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == FormatStatus::Fixed)
            .count()
    }

    pub fn would_fix_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == FormatStatus::WouldFix)
            .count()
    }

    pub fn skipped_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == FormatStatus::Skipped)
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == FormatStatus::Failed)
            .count()
    }

    pub fn unchanged_count(&self) -> usize {
        self.items
            .iter()
            .filter(|item| item.status == FormatStatus::Unchanged)
            .count()
    }

    pub fn exit_code(&self, dry_run: bool) -> i32 {
        if self.failed_count() > 0 {
            EXIT_ERROR
        } else if dry_run && self.would_fix_count() > 0 {
            EXIT_CHANGES
        } else {
            EXIT_SUCCESS
        }
    }
}

pub fn run(cwd: &Path, config: &Config, dry_run: bool) -> Result<FormatReport> {
    let report = tools::run_registry(
        cwd,
        config,
        &config.formatters,
        ToolKind::Formatter,
        dry_run,
    )?;
    Ok(FormatReport {
        items: report
            .items
            .into_iter()
            .map(|item| FormatItem {
                path: item.target.unwrap_or_else(|| cwd.to_path_buf()),
                formatter: item.tool,
                status: match item.status {
                    ToolStatus::Changed => FormatStatus::Fixed,
                    ToolStatus::WouldChange => FormatStatus::WouldFix,
                    ToolStatus::Unchanged | ToolStatus::Passed => FormatStatus::Unchanged,
                    ToolStatus::Skipped => FormatStatus::Skipped,
                    ToolStatus::Failed => FormatStatus::Failed,
                },
                detail: item.detail,
            })
            .collect(),
    })
}
