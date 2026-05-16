use std::path::{Path, PathBuf};

use owo_colors::OwoColorize;

use crate::ci::{ProjectCheckReport, ProjectCheckStatus};
use crate::doctor::{DoctorReport, DoctorStatus};
use crate::fmt::{FormatReport, FormatStatus};
use crate::lint::{LintReport, LintStatus};
use crate::terraform::{PlanReport, ValidateReport};
use crate::validate::{ValidationReport, ValidationStatus};

#[derive(Debug, Clone, Copy)]
pub struct Reporter {
    quiet: bool,
}

impl Reporter {
    pub fn new(quiet: bool) -> Self {
        Self { quiet }
    }

    pub fn success(&self, message: impl AsRef<str>) {
        if !self.quiet {
            println!("{} {}", "ok".green().bold(), message.as_ref());
        }
    }

    pub fn info(&self, message: impl AsRef<str>) {
        if !self.quiet {
            println!("{}", message.as_ref());
        }
    }

    pub fn print_format_report(&self, report: &FormatReport, dry_run: bool) {
        if self.quiet {
            return;
        }

        section("devctl fmt");
        let action = if dry_run { "checked" } else { "formatted" };
        summary(&[
            ("mode", action.to_owned()),
            ("targets", report.items.len().to_string()),
            ("fixed", report.fixed_count().green().to_string()),
            ("would fix", report.would_fix_count().yellow().to_string()),
            ("unchanged", report.unchanged_count().white().to_string()),
            ("skipped", report.skipped_count().blue().to_string()),
            ("failed", report.failed_count().red().to_string()),
        ]);

        let rows: Vec<_> = report
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item.status,
                    FormatStatus::Fixed | FormatStatus::WouldFix | FormatStatus::Failed
                )
            })
            .collect();

        if rows.is_empty() {
            quiet_line("No formatter changes or failures.");
            return;
        }

        table_header();
        for item in rows {
            let status = match item.status {
                FormatStatus::Fixed => status("fixed", StatusTone::Good),
                FormatStatus::WouldFix => status("would fix", StatusTone::Warn),
                FormatStatus::Failed => status("failed", StatusTone::Bad),
                FormatStatus::Unchanged | FormatStatus::Skipped => continue,
            };
            table_row(&status, &item.formatter, &pretty_path(&item.path));
            if item.status == FormatStatus::Failed {
                detail_line(&item.detail, StatusTone::Bad);
            }
        }
    }

    pub fn print_plan_report(&self, report: &PlanReport, detailed: bool) {
        if self.quiet {
            return;
        }

        section("terraform plan");
        summary(&[
            ("root", pretty_path(&report.root)),
            (
                "status",
                if report.success {
                    if report.summary.has_changes() {
                        "succeeded with changes".yellow().bold().to_string()
                    } else {
                        "succeeded".green().bold().to_string()
                    }
                } else {
                    "failed".red().bold().to_string()
                },
            ),
            ("add", report.summary.add.green().to_string()),
            ("change", report.summary.change.yellow().to_string()),
            ("destroy", report.summary.remove.red().to_string()),
        ]);

        if report.success {
            for change in report.changes.iter().take(25) {
                println!(
                    "  {} {:<18} {}",
                    action_marker(&change.action),
                    change.action,
                    change.address
                );
            }
            if report.changes.len() > 25 {
                quiet_line(format!("... {} more changes", report.changes.len() - 25));
            }
        } else {
            for diagnostic in &report.diagnostics {
                detail_line(
                    format!("{}: {}", diagnostic.severity, diagnostic.summary),
                    StatusTone::Bad,
                );
                if let Some(detail) = &diagnostic.detail {
                    detail_line(detail, StatusTone::Plain);
                }
            }
            if let Some(suggestion) = &report.suggestion {
                suggestion_line(suggestion);
            }
        }

        if detailed && !report.raw_output.trim().is_empty() {
            println!();
            section("raw terraform output");
            println!("{}", report.raw_output.trim());
        }
    }

    pub fn print_validate_report(&self, report: &ValidateReport) {
        if self.quiet {
            return;
        }

        section("terraform validate");
        summary(&[
            ("root", pretty_path(&report.root)),
            (
                "status",
                if report.valid {
                    "passed".green().bold().to_string()
                } else {
                    "failed".red().bold().to_string()
                },
            ),
        ]);

        for diagnostic in &report.diagnostics {
            let tone = if diagnostic.severity.eq_ignore_ascii_case("warning") {
                StatusTone::Warn
            } else {
                StatusTone::Bad
            };
            detail_line(
                format!("{}: {}", diagnostic.severity, diagnostic.summary),
                tone,
            );
            if let Some(detail) = &diagnostic.detail {
                detail_line(detail, StatusTone::Plain);
            }
        }
    }

    pub fn print_lint_report(&self, label: &str, report: &LintReport) {
        if self.quiet {
            return;
        }

        section(format!("devctl {label}"));
        summary(&[
            ("targets", report.items.len().to_string()),
            ("passed", report.passed_count().green().to_string()),
            ("skipped", report.skipped_count().blue().to_string()),
            ("failed", report.failed_count().red().to_string()),
        ]);

        let rows: Vec<_> = report
            .items
            .iter()
            .filter(|item| item.status != LintStatus::Passed)
            .collect();

        if rows.is_empty() {
            quiet_line("No lint issues.");
            return;
        }

        table_header();
        for item in rows {
            let marker = match item.status {
                LintStatus::Passed => continue,
                LintStatus::Skipped => status("skipped", StatusTone::Skip),
                LintStatus::Failed => status("failed", StatusTone::Bad),
            };
            table_row(&marker, &item.linter, &display_target(&item.target));
            detail_line(
                &item.detail,
                if item.status == LintStatus::Failed {
                    StatusTone::Bad
                } else {
                    StatusTone::Plain
                },
            );
            if let Some(suggestion) = &item.suggestion {
                suggestion_line(suggestion);
            }
        }
    }

    pub fn print_validation_report(&self, report: &ValidationReport) {
        if self.quiet {
            return;
        }

        section("devctl validate");
        summary(&[
            ("targets", report.items.len().to_string()),
            ("passed", report.passed_count().green().to_string()),
            ("skipped", report.skipped_count().blue().to_string()),
            ("failed", report.failed_count().red().to_string()),
        ]);

        let rows: Vec<_> = report
            .items
            .iter()
            .filter(|item| item.status != ValidationStatus::Passed)
            .collect();

        if rows.is_empty() {
            quiet_line("No validation issues.");
            return;
        }

        table_header();
        for item in rows {
            let marker = match item.status {
                ValidationStatus::Passed => continue,
                ValidationStatus::Skipped => status("skipped", StatusTone::Skip),
                ValidationStatus::Failed => status("failed", StatusTone::Bad),
            };
            table_row(&marker, &item.validator, &display_target(&item.target));
            detail_line(
                &item.detail,
                if item.status == ValidationStatus::Failed {
                    StatusTone::Bad
                } else {
                    StatusTone::Plain
                },
            );
            if let Some(suggestion) = &item.suggestion {
                suggestion_line(suggestion);
            }
        }
    }

    pub fn print_project_check_report(&self, report: &ProjectCheckReport) {
        if self.quiet {
            return;
        }

        section("devctl project checks");
        summary(&[
            ("projects", report.projects.len().to_string()),
            ("checks", report.items.len().to_string()),
            ("passed", report.passed_count().green().to_string()),
            ("changes", report.changes_count().yellow().to_string()),
            ("skipped", report.skipped_count().blue().to_string()),
            ("failed", report.failed_count().red().to_string()),
        ]);

        if report.projects.is_empty() {
            quiet_line("No projects detected.");
            return;
        }

        println!();
        println!(
            "  {}  {}",
            "project".bright_black(),
            "detected".bright_black()
        );
        for project in &report.projects {
            let detected = if project.kinds.is_empty() {
                "none".bright_black().to_string()
            } else {
                project.kind_labels().white().to_string()
            };
            println!("  {:<32}  {}", pretty_path(&project.root), detected);
        }

        if report.items.is_empty() {
            quiet_line("No project-specific checks were needed.");
            return;
        }

        println!();
        println!(
            "  {}  {}  {}  {}",
            "status".bright_black(),
            "project".bright_black(),
            "check".bright_black(),
            "detail".bright_black()
        );
        for item in &report.items {
            let marker = match item.status {
                ProjectCheckStatus::Passed => status("passed", StatusTone::Good),
                ProjectCheckStatus::Skipped => status("skipped", StatusTone::Skip),
                ProjectCheckStatus::Failed => status("failed", StatusTone::Bad),
                ProjectCheckStatus::Changes => status("changes", StatusTone::Warn),
            };
            let detail_tone = match item.status {
                ProjectCheckStatus::Passed => StatusTone::Good,
                ProjectCheckStatus::Skipped => StatusTone::Plain,
                ProjectCheckStatus::Failed => StatusTone::Bad,
                ProjectCheckStatus::Changes => StatusTone::Warn,
            };
            println!(
                "  {marker:<18}  {:<32}  {:<24}  {}",
                pretty_path(&item.project),
                item.check,
                color_line(&item.detail, detail_tone)
            );
            if let Some(suggestion) = &item.suggestion {
                suggestion_line(suggestion);
            }
        }
    }

    pub fn print_doctor_report(&self, report: &DoctorReport) {
        if self.quiet {
            return;
        }

        section("devctl doctor");
        summary(&[
            ("checks", report.checks.len().to_string()),
            (
                "ready",
                report.count(DoctorStatus::Ready).green().to_string(),
            ),
            (
                "fallback",
                report.count(DoctorStatus::Fallback).yellow().to_string(),
            ),
            (
                "warning",
                report.count(DoctorStatus::Warning).yellow().to_string(),
            ),
            (
                "missing",
                report.count(DoctorStatus::Missing).red().to_string(),
            ),
            (
                "disabled",
                report.count(DoctorStatus::Disabled).blue().to_string(),
            ),
        ]);

        doctor_group(
            "Action needed",
            report,
            |status| matches!(status, DoctorStatus::Missing | DoctorStatus::Warning),
            true,
        );
        doctor_group(
            "Available through fallback",
            report,
            |status| status == DoctorStatus::Fallback,
            true,
        );
        doctor_group(
            "Ready",
            report,
            |status| matches!(status, DoctorStatus::Ready | DoctorStatus::Builtin),
            false,
        );
        doctor_group(
            "Disabled",
            report,
            |status| status == DoctorStatus::Disabled,
            false,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusTone {
    Good,
    Warn,
    Bad,
    Skip,
    Plain,
}

fn section(title: impl AsRef<str>) {
    println!();
    println!("{}", title.as_ref().cyan().bold());
    println!("{}", "-".repeat(title.as_ref().len()).bright_black());
}

fn summary(parts: &[(&str, String)]) {
    let rendered = parts
        .iter()
        .map(|(label, value)| format!("{} {}", label.bright_black(), value))
        .collect::<Vec<_>>()
        .join("  |  ");
    println!("  {rendered}");
}

fn table_header() {
    println!();
    println!(
        "  {}  {}  {}",
        "status".bright_black(),
        "tool".bright_black(),
        "target".bright_black()
    );
}

fn table_row(status: &str, tool: &str, target: &str) {
    println!("  {status:<18}  {tool:<18}  {target}");
}

fn detail_line(detail: impl AsRef<str>, tone: StatusTone) {
    for line in detail.as_ref().lines() {
        let line = match tone {
            StatusTone::Good => line.green().to_string(),
            StatusTone::Warn => line.yellow().to_string(),
            StatusTone::Bad => line.red().to_string(),
            StatusTone::Skip => line.blue().to_string(),
            StatusTone::Plain => line.white().to_string(),
        };
        println!("  {:<18}  {}", "", line);
    }
}

fn suggestion_line(suggestion: &str) {
    println!(
        "  {:<18}  {} {}",
        "",
        "suggestion:".yellow().bold(),
        suggestion
    );
}

fn color_line(detail: &str, tone: StatusTone) -> String {
    match tone {
        StatusTone::Good => detail.green().to_string(),
        StatusTone::Warn => detail.yellow().to_string(),
        StatusTone::Bad => detail.red().to_string(),
        StatusTone::Skip => detail.blue().to_string(),
        StatusTone::Plain => detail.white().to_string(),
    }
}

fn quiet_line(message: impl AsRef<str>) {
    println!("  {}", message.as_ref().bright_black());
}

fn status(label: &str, tone: StatusTone) -> String {
    let padded = format!("{label:<10}");
    match tone {
        StatusTone::Good => padded.green().to_string(),
        StatusTone::Warn => padded.yellow().to_string(),
        StatusTone::Bad => padded.red().to_string(),
        StatusTone::Skip => padded.blue().to_string(),
        StatusTone::Plain => padded.white().to_string(),
    }
}

fn doctor_status(doctor_status: DoctorStatus) -> String {
    match doctor_status {
        DoctorStatus::Ready => status("ready", StatusTone::Good),
        DoctorStatus::Fallback => status("fallback", StatusTone::Warn),
        DoctorStatus::Warning => status("warning", StatusTone::Warn),
        DoctorStatus::Missing => status("missing", StatusTone::Bad),
        DoctorStatus::Disabled => status("disabled", StatusTone::Skip),
        DoctorStatus::Builtin => status("built-in", StatusTone::Good),
    }
}

fn doctor_group(
    title: &str,
    report: &DoctorReport,
    filter: impl Fn(DoctorStatus) -> bool,
    show_suggestions: bool,
) {
    let rows = report
        .checks
        .iter()
        .filter(|check| filter(check.status))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return;
    }

    println!();
    println!("  {}", title.bold());
    println!(
        "  {}  {}  {}  {}",
        "status".bright_black(),
        "tool".bright_black(),
        "command".bright_black(),
        "detail".bright_black()
    );

    for check in rows {
        let tool = format!("{}/{}", check.area, check.tool);
        println!(
            "  {:<18}  {:<24}  {:<22}  {}",
            doctor_status(check.status),
            tool,
            check.command,
            color_doctor_detail(&check.detail, check.status),
        );
        if show_suggestions && let Some(suggestion) = &check.suggestion {
            println!("  {:<18}  {}", "", suggestion.yellow());
        }
    }
}

fn color_doctor_detail(detail: &str, status: DoctorStatus) -> String {
    match status {
        DoctorStatus::Missing => detail.red().to_string(),
        DoctorStatus::Warning | DoctorStatus::Fallback => detail.yellow().to_string(),
        DoctorStatus::Disabled => detail.blue().to_string(),
        DoctorStatus::Ready | DoctorStatus::Builtin => detail.white().to_string(),
    }
}

fn action_marker(action: &str) -> String {
    match action {
        "create" => "+".green().bold().to_string(),
        "update" | "read" => "~".yellow().bold().to_string(),
        "delete" => "-".red().bold().to_string(),
        "replace" | "delete,create" | "create,delete" => "+/-".yellow().bold().to_string(),
        _ => "?".white().to_string(),
    }
}

fn display_target(target: &Option<PathBuf>) -> String {
    target
        .as_ref()
        .map(|path| pretty_path(path))
        .unwrap_or_else(|| "project".to_owned())
}

fn pretty_path(path: &Path) -> String {
    let path_display = clean_path_display(path);
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_display = clean_path_display(&cwd);
        if path_display.eq_ignore_ascii_case(&cwd_display) {
            return ".".to_owned();
        }
        let prefix = format!("{cwd_display}{}", std::path::MAIN_SEPARATOR);
        if path_display
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase())
        {
            return path_display[prefix.len()..].to_owned();
        }
    }
    path_display
}

fn clean_path_display(path: &Path) -> String {
    let display = path.display().to_string();
    display
        .strip_prefix(r"\\?\")
        .unwrap_or(display.as_str())
        .to_owned()
}
