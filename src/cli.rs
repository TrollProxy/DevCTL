use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

const EXAMPLES: &str = "\
Examples:
  devctl fmt
  devctl lint
  devctl validate
  devctl docker lint
  devctl check
  devctl check --workspace
  devctl --doctor
  devctl --dry-run fmt
  devctl terraform plan
  devctl terraform plan --detailed
  devctl terraform validate
  devctl run test -- --nocapture
  devctl completions zsh > _devctl
  devctl man > devctl.1";

#[derive(Debug, Parser)]
#[command(
    name = "devctl",
    version,
    about = "A lightweight DevOps control CLI.",
    long_about = "A lightweight DevOps control CLI for recursive formatting, isolated Terraform planning, and configurable team commands.",
    after_help = EXAMPLES,
    arg_required_else_help = false
)]
pub struct Cli {
    #[arg(
        short,
        long,
        global = true,
        value_name = "FILE",
        help = "Load an explicit config file after global defaults"
    )]
    pub config: Option<PathBuf>,

    #[arg(
        short,
        long,
        global = true,
        action = clap::ArgAction::Count,
        help = "Increase log verbosity; repeat for debug/trace"
    )]
    pub verbose: u8,

    #[arg(
        long,
        global = true,
        conflicts_with = "verbose",
        help = "Suppress non-essential output"
    )]
    pub quiet: bool,

    #[arg(
        long,
        global = true,
        help = "Preview changes without modifying files or running commands"
    )]
    pub dry_run: bool,

    #[arg(
        long,
        global = true,
        help = "Check local tools, Docker fallback, and config health"
    )]
    pub doctor: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Auto-format supported files recursively.")]
    Fmt(FmtArgs),

    #[command(about = "Run configured linters recursively.")]
    Lint(LintArgs),

    #[command(about = "Run configured validators.")]
    Validate(ValidateArgs),

    #[command(about = "Run Docker-focused workflows.")]
    Docker(DockerArgs),

    #[command(about = "Run project-aware checks for pre-commit/CI.")]
    Check(CheckArgs),

    #[command(about = "Run safe Terraform workflows.")]
    Terraform(TerraformArgs),

    #[command(about = "Run a custom command from devctl.yaml.")]
    Run(RunArgs),

    #[command(about = "Generate shell completions.")]
    Completions(CompletionsArgs),

    #[command(about = "Generate a roff man page.")]
    Man(ManArgs),

    #[command(name = "init-config", about = "Write an example devctl.yaml.")]
    InitConfig(InitConfigArgs),
}

#[derive(Debug, Args)]
pub struct FmtArgs {
    #[arg(long, help = "Run once per detected project root in the workspace.")]
    pub workspace: bool,
}

#[derive(Debug, Args)]
pub struct LintArgs {
    #[arg(long, help = "Run once per detected project root in the workspace.")]
    pub workspace: bool,
}

#[derive(Debug, Args)]
pub struct ValidateArgs {
    #[arg(long, help = "Run once per detected project root in the workspace.")]
    pub workspace: bool,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(
        long,
        help = "Discover and check each project root in the workspace instead of only the current project."
    )]
    pub workspace: bool,
}

#[derive(Debug, Args)]
pub struct DockerArgs {
    #[command(subcommand)]
    pub command: DockerCommands,
}

#[derive(Debug, Subcommand)]
pub enum DockerCommands {
    #[command(about = "Run hadolint on Dockerfile* files.")]
    Lint(DockerLintArgs),
}

#[derive(Debug, Args)]
pub struct DockerLintArgs {
    #[arg(
        long,
        help = "Apply fixes where the selected Docker linter supports them."
    )]
    pub fix: bool,
}

#[derive(Debug, Args)]
pub struct TerraformArgs {
    #[command(subcommand)]
    pub command: TerraformCommands,
}

#[derive(Debug, Subcommand)]
pub enum TerraformCommands {
    #[command(about = "Run terraform plan in an isolated sandbox.")]
    Plan(TerraformPlanArgs),

    #[command(about = "Run terraform validate in an isolated sandbox.")]
    Validate(TerraformValidateArgs),
}

#[derive(Debug, Args)]
pub struct TerraformPlanArgs {
    #[arg(long, help = "Print raw Terraform JSON UI output after the summary.")]
    pub detailed: bool,

    #[arg(long, value_name = "DIR", help = "Directory to search from.")]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct TerraformValidateArgs {
    #[arg(long, value_name = "DIR", help = "Directory to search from.")]
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(help = "Configured command name from devctl.yaml")]
    pub name: String,

    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "Additional arguments appended to the configured command"
    )]
    pub args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    #[arg(value_enum)]
    pub shell: Shell,
}

#[derive(Debug, Args)]
pub struct ManArgs {}

#[derive(Debug, Args)]
pub struct InitConfigArgs {
    #[arg(value_name = "FILE")]
    pub path: Option<PathBuf>,

    #[arg(long)]
    pub force: bool,
}
