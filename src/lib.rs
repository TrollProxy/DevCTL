#![forbid(unsafe_code)]

pub mod ci;
pub mod cli;
pub mod command;
pub mod config;
pub mod docker;
pub mod doctor;
pub mod error;
pub mod fmt;
pub mod fs;
pub mod lint;
pub mod output;
pub mod process;
pub mod project;
pub mod terraform;
pub mod tools;
pub mod validate;

use std::ffi::OsString;
use std::io;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};

use crate::cli::{Cli, Commands, DockerCommands, TerraformCommands};
use crate::output::Reporter;

pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_ERROR: i32 = 1;
pub const EXIT_CHANGES: i32 = 2;

pub fn run_from<I, T>(args: I) -> Result<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::parse_from(args);
    init_tracing(cli.verbose, cli.quiet);

    let cwd = std::env::current_dir()?;
    let reporter = Reporter::new(cli.quiet);
    if cli.doctor {
        let loaded = config::load(cli.config.as_deref(), &cwd)?;
        let report = doctor::run(&cwd, &loaded.config, &loaded.sources)?;
        reporter.print_doctor_report(&report);
        return Ok(EXIT_SUCCESS);
    }

    let Some(command) = cli.command else {
        let mut command = Cli::command();
        command.print_help()?;
        println!();
        return Ok(EXIT_SUCCESS);
    };

    match command {
        Commands::Completions(args) => {
            let mut command = Cli::command();
            clap_complete::generate(args.shell, &mut command, "devctl", &mut io::stdout());
            Ok(EXIT_SUCCESS)
        }
        Commands::Man(_) => {
            let command = Cli::command();
            clap_mangen::Man::new(command).render(&mut io::stdout())?;
            Ok(EXIT_SUCCESS)
        }
        Commands::InitConfig(args) => {
            let path = args.path.unwrap_or_else(|| cwd.join("devctl.yaml"));
            config::write_example(&path, args.force)?;
            reporter.success(format!("wrote {}", path.display()));
            Ok(EXIT_SUCCESS)
        }
        Commands::Fmt(args) => {
            let loaded = config::load(cli.config.as_deref(), &cwd)?;
            let report = run_format(&cwd, &loaded.config, cli.dry_run, args.workspace)?;
            reporter.print_format_report(&report, cli.dry_run);
            Ok(report.exit_code(cli.dry_run))
        }
        Commands::Lint(args) => {
            let loaded = config::load(cli.config.as_deref(), &cwd)?;
            let report = run_lint(&cwd, &loaded.config, cli.dry_run, args.workspace)?;
            reporter.print_lint_report("lint", &report);
            Ok(report.exit_code())
        }
        Commands::Validate(args) => {
            let loaded = config::load(cli.config.as_deref(), &cwd)?;
            let report = run_validate(&cwd, &loaded.config, cli.dry_run, args.workspace)?;
            reporter.print_validation_report(&report);
            Ok(report.exit_code())
        }
        Commands::Docker(docker_args) => match docker_args.command {
            DockerCommands::Lint(args) => {
                let loaded = config::load(cli.config.as_deref(), &cwd)?;
                let report = docker::lint(&cwd, &loaded.config, cli.dry_run, args.fix)?;
                reporter.print_lint_report("docker lint", &report);
                Ok(report.exit_code())
            }
        },
        Commands::Check(args) => {
            let loaded = config::load(cli.config.as_deref(), &cwd)?;
            let project_report = ci::run(&cwd, &loaded.config, cli.dry_run, args.workspace)?;
            reporter.print_project_check_report(&project_report);

            let fmt_report = run_format(&cwd, &loaded.config, cli.dry_run, false)?;
            reporter.print_format_report(&fmt_report, cli.dry_run);

            let lint_report = run_lint(&cwd, &loaded.config, cli.dry_run, false)?;
            reporter.print_lint_report("lint", &lint_report);

            let validation_report = run_validate(&cwd, &loaded.config, cli.dry_run, false)?;
            reporter.print_validation_report(&validation_report);

            Ok(check_exit_code(
                fmt_report.exit_code(cli.dry_run),
                lint_report.exit_code(),
                validation_report.exit_code(),
                project_report.exit_code(),
            ))
        }
        Commands::Terraform(terraform) => match terraform.command {
            TerraformCommands::Plan(args) => {
                let loaded = config::load(cli.config.as_deref(), &cwd)?;
                let report = terraform::plan(
                    args.dir.as_deref().unwrap_or(&cwd),
                    &loaded.config,
                    args.detailed,
                )?;
                reporter.print_plan_report(&report, args.detailed);
                Ok(report.exit_code())
            }
            TerraformCommands::Validate(args) => {
                let loaded = config::load(cli.config.as_deref(), &cwd)?;
                let report =
                    terraform::validate(args.dir.as_deref().unwrap_or(&cwd), &loaded.config)?;
                reporter.print_validate_report(&report);
                Ok(report.exit_code())
            }
        },
        Commands::Run(args) => {
            let loaded = config::load(cli.config.as_deref(), &cwd)?;
            command::run_custom(
                &args.name,
                &args.args,
                &loaded.config,
                &cwd,
                cli.dry_run,
                &reporter,
            )
        }
    }
}

fn workspace_roots(
    cwd: &std::path::Path,
    config: &config::Config,
    workspace: bool,
) -> Result<Vec<std::path::PathBuf>> {
    if !workspace {
        return Ok(vec![cwd.to_path_buf()]);
    }

    Ok(project::discover(cwd, config, true)?
        .into_iter()
        .map(|project| project.root)
        .collect())
}

fn run_format(
    cwd: &std::path::Path,
    config: &config::Config,
    dry_run: bool,
    workspace: bool,
) -> Result<fmt::FormatReport> {
    let mut report = fmt::FormatReport::default();
    for root in workspace_roots(cwd, config, workspace)? {
        report.items.extend(fmt::run(&root, config, dry_run)?.items);
    }
    Ok(report)
}

fn run_lint(
    cwd: &std::path::Path,
    config: &config::Config,
    dry_run: bool,
    workspace: bool,
) -> Result<lint::LintReport> {
    let mut report = lint::LintReport::default();
    for root in workspace_roots(cwd, config, workspace)? {
        report.extend(lint::run(&root, config, dry_run)?);
    }
    Ok(report)
}

fn run_validate(
    cwd: &std::path::Path,
    config: &config::Config,
    dry_run: bool,
    workspace: bool,
) -> Result<validate::ValidationReport> {
    let mut report = validate::ValidationReport::default();
    for root in workspace_roots(cwd, config, workspace)? {
        report
            .items
            .extend(validate::run(&root, config, dry_run)?.items);
    }
    Ok(report)
}

fn check_exit_code(fmt_code: i32, lint_code: i32, validate_code: i32, project_code: i32) -> i32 {
    if [fmt_code, lint_code, validate_code, project_code].contains(&EXIT_ERROR) {
        EXIT_ERROR
    } else if [fmt_code, project_code].contains(&EXIT_CHANGES) {
        EXIT_CHANGES
    } else {
        EXIT_SUCCESS
    }
}

fn init_tracing(verbose: u8, quiet: bool) {
    let default_level = if quiet {
        "error"
    } else {
        match verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    };

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(FmtSpan::NONE)
        .with_target(false)
        .without_time()
        .try_init();
}
