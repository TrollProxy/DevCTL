use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::{Config, CustomCommand, ToolConfig, ToolRegistryConfig};
use crate::process::{self, CommandSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorStatus {
    Ready,
    Fallback,
    Warning,
    Missing,
    Disabled,
    Builtin,
}

#[derive(Debug, Clone)]
pub struct DoctorCheck {
    pub area: String,
    pub tool: String,
    pub command: String,
    pub status: DoctorStatus,
    pub detail: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn count(&self, status: DoctorStatus) -> usize {
        self.checks
            .iter()
            .filter(|check| check.status == status)
            .count()
    }
}

#[derive(Debug, Clone, Copy)]
struct DockerHealth {
    cli_available: bool,
    daemon_available: bool,
}

pub fn run(root: &Path, config: &Config, sources: &[PathBuf]) -> Result<DoctorReport> {
    let docker = docker_health();
    let mut checks = Vec::new();

    checks.push(config_check(sources));
    checks.push(terraform_check(config));
    checks.push(docker_check(docker));

    collect_registry_checks(&mut checks, "formatter", &config.formatters, config, docker);
    collect_registry_checks(&mut checks, "linter", &config.linters, config, docker);
    collect_registry_checks(&mut checks, "validator", &config.validators, config, docker);
    checks.push(tool_check(
        "docker",
        "hadolint",
        &config.docker.hadolint,
        config,
        docker,
    ));

    for (name, command) in &config.commands {
        checks.push(command_check("custom command", name, command));
    }

    checks.sort_by(|left, right| {
        status_rank(left.status)
            .cmp(&status_rank(right.status))
            .then_with(|| left.area.cmp(&right.area))
            .then_with(|| left.tool.cmp(&right.tool))
    });

    let _ = root;
    Ok(DoctorReport { checks })
}

fn collect_registry_checks(
    checks: &mut Vec<DoctorCheck>,
    area: &str,
    registry: &ToolRegistryConfig,
    config: &Config,
    docker: DockerHealth,
) {
    if !registry.enabled {
        checks.push(DoctorCheck {
            area: area.to_owned(),
            tool: "registry".to_owned(),
            command: "-".to_owned(),
            status: DoctorStatus::Disabled,
            detail: "registry disabled".to_owned(),
            suggestion: None,
        });
        return;
    }

    let mut seen = BTreeSet::new();
    for (name, tool) in &registry.tools {
        seen.insert(name.clone());
        checks.push(tool_check(area, name, tool, config, docker));
    }

    for (name, command) in &registry.custom {
        if seen.insert(name.clone()) {
            checks.push(command_check(area, name, command));
        }
    }
}

fn tool_check(
    area: &str,
    name: &str,
    tool: &ToolConfig,
    config: &Config,
    docker: DockerHealth,
) -> DoctorCheck {
    if !tool.enabled {
        return DoctorCheck {
            area: area.to_owned(),
            tool: name.to_owned(),
            command: tool.command.clone(),
            status: DoctorStatus::Disabled,
            detail: "disabled in config".to_owned(),
            suggestion: None,
        };
    }

    if tool.command.starts_with("builtin:") {
        return DoctorCheck {
            area: area.to_owned(),
            tool: name.to_owned(),
            command: tool.command.clone(),
            status: DoctorStatus::Builtin,
            detail: "built into devctl".to_owned(),
            suggestion: None,
        };
    }

    if let Some(program) = process::which(&tool.command) {
        return version_check(area, name, &tool.command, program, &tool.env);
    }

    let fallback_enabled = tool.docker_fallback || config.global.docker_fallback;
    if fallback_enabled && let Some(image) = tool.docker_image.as_deref() {
        if docker.daemon_available {
            return DoctorCheck {
                area: area.to_owned(),
                tool: name.to_owned(),
                command: tool.command.clone(),
                status: DoctorStatus::Fallback,
                detail: format!("native CLI missing; Docker fallback available via {image}"),
                suggestion: Some(install_or_docker_suggestion(&tool.command, image)),
            };
        }
        if docker.cli_available {
            return DoctorCheck {
                area: area.to_owned(),
                tool: name.to_owned(),
                command: tool.command.clone(),
                status: DoctorStatus::Warning,
                detail: format!(
                    "native CLI missing; Docker CLI found but daemon is unavailable for {image}"
                ),
                suggestion: Some(format!(
                    "Start Docker, or install natively: {}",
                    install_hint(&tool.command)
                )),
            };
        }
    }

    DoctorCheck {
        area: area.to_owned(),
        tool: name.to_owned(),
        command: tool.command.clone(),
        status: DoctorStatus::Missing,
        detail: "not found on PATH".to_owned(),
        suggestion: Some(format!(
            "{} Or disable `{name}` in devctl.yaml.",
            install_hint(&tool.command)
        )),
    }
}

fn command_check(area: &str, name: &str, command: &CustomCommand) -> DoctorCheck {
    if command.command.trim().is_empty() {
        return DoctorCheck {
            area: area.to_owned(),
            tool: name.to_owned(),
            command: "-".to_owned(),
            status: DoctorStatus::Warning,
            detail: "command is empty".to_owned(),
            suggestion: Some("Set command or remove this entry from devctl.yaml.".to_owned()),
        };
    }

    if let Some(program) = process::which(&command.command) {
        return version_check(area, name, &command.command, program, &command.env);
    }

    DoctorCheck {
        area: area.to_owned(),
        tool: name.to_owned(),
        command: command.command.clone(),
        status: DoctorStatus::Missing,
        detail: "not found on PATH".to_owned(),
        suggestion: Some(format!(
            "{} Or update `{name}`.",
            install_hint(&command.command)
        )),
    }
}

fn terraform_check(config: &Config) -> DoctorCheck {
    let command = &config.terraform.binary;
    let Some(program) = process::which(command) else {
        return DoctorCheck {
            area: "runtime".to_owned(),
            tool: "terraform".to_owned(),
            command: command.clone(),
            status: DoctorStatus::Missing,
            detail: "not found on PATH".to_owned(),
            suggestion: Some(format!(
                "{} Or set terraform.binary in devctl.yaml.",
                install_hint(command)
            )),
        };
    };

    version_check("runtime", "terraform", command, program, &BTreeMap::new())
}

fn docker_check(docker: DockerHealth) -> DoctorCheck {
    match (docker.cli_available, docker.daemon_available) {
        (true, true) => DoctorCheck {
            area: "runtime".to_owned(),
            tool: "docker".to_owned(),
            command: "docker".to_owned(),
            status: DoctorStatus::Ready,
            detail: "Docker CLI and daemon are available".to_owned(),
            suggestion: None,
        },
        (true, false) => DoctorCheck {
            area: "runtime".to_owned(),
            tool: "docker".to_owned(),
            command: "docker".to_owned(),
            status: DoctorStatus::Warning,
            detail: "Docker CLI found, but daemon is not available".to_owned(),
            suggestion: Some(format!(
                "Start Docker Desktop/service, or install native CLIs. Docker install: {}",
                install_hint("docker")
            )),
        },
        (false, false) => DoctorCheck {
            area: "runtime".to_owned(),
            tool: "docker".to_owned(),
            command: "docker".to_owned(),
            status: DoctorStatus::Missing,
            detail: "Docker is not installed or not on PATH".to_owned(),
            suggestion: Some(format!(
                "Install native CLIs for configured tools, or install Docker for fallbacks: {}",
                install_hint("docker")
            )),
        },
        (false, true) => unreachable!("Docker daemon cannot be checked without Docker CLI"),
    }
}

fn config_check(sources: &[PathBuf]) -> DoctorCheck {
    DoctorCheck {
        area: "config".to_owned(),
        tool: "devctl.yaml".to_owned(),
        command: "-".to_owned(),
        status: DoctorStatus::Ready,
        detail: if sources.is_empty() {
            "using built-in defaults".to_owned()
        } else {
            format!(
                "loaded {}",
                sources
                    .iter()
                    .map(|source| source.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
        suggestion: None,
    }
}

fn version_check(
    area: &str,
    name: &str,
    command: &str,
    program: PathBuf,
    env: &BTreeMap<String, String>,
) -> DoctorCheck {
    let Some(args) = version_args(command) else {
        return DoctorCheck {
            area: area.to_owned(),
            tool: name.to_owned(),
            command: command.to_owned(),
            status: DoctorStatus::Ready,
            detail: format!("installed at {}", program.display()),
            suggestion: None,
        };
    };

    let output = process::capture(&CommandSpec {
        program: program.clone(),
        args,
        cwd: None,
        env: env.clone(),
        sanitized_env: false,
    });

    match output {
        Ok(output) if output.status.success() => DoctorCheck {
            area: area.to_owned(),
            tool: name.to_owned(),
            command: command.to_owned(),
            status: DoctorStatus::Ready,
            detail: first_line(&output.stdout, &output.stderr)
                .unwrap_or_else(|| format!("installed at {}", program.display())),
            suggestion: None,
        },
        Ok(output) => DoctorCheck {
            area: area.to_owned(),
            tool: name.to_owned(),
            command: command.to_owned(),
            status: DoctorStatus::Warning,
            detail: first_line(&output.stderr, &output.stdout)
                .unwrap_or_else(|| "installed, but version probe failed".to_owned()),
            suggestion: Some(
                "The CLI exists, but `devctl --doctor` could not confirm it runs cleanly."
                    .to_owned(),
            ),
        },
        Err(error) => DoctorCheck {
            area: area.to_owned(),
            tool: name.to_owned(),
            command: command.to_owned(),
            status: DoctorStatus::Warning,
            detail: error.to_string(),
            suggestion: Some("The CLI exists, but could not be executed by devctl.".to_owned()),
        },
    }
}

fn docker_health() -> DockerHealth {
    let Some(docker) = process::which("docker") else {
        return DockerHealth {
            cli_available: false,
            daemon_available: false,
        };
    };

    let output = process::capture(&CommandSpec {
        program: docker,
        args: vec!["version".to_owned()],
        cwd: None,
        env: BTreeMap::new(),
        sanitized_env: false,
    });

    DockerHealth {
        cli_available: true,
        daemon_available: output.is_ok_and(|output| output.status.success()),
    }
}

fn version_args(command: &str) -> Option<Vec<String>> {
    let command = Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .trim_end_matches(".exe")
        .to_ascii_lowercase();

    match command.as_str() {
        "go" => Some(vec!["version".to_owned()]),
        "gofmt" => None,
        "terraform" => Some(vec!["version".to_owned()]),
        _ => Some(vec!["--version".to_owned()]),
    }
}

fn first_line(primary: &str, fallback: &str) -> Option<String> {
    primary
        .lines()
        .chain(fallback.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn status_rank(status: DoctorStatus) -> u8 {
    match status {
        DoctorStatus::Missing => 0,
        DoctorStatus::Warning => 1,
        DoctorStatus::Fallback => 2,
        DoctorStatus::Ready => 3,
        DoctorStatus::Builtin => 4,
        DoctorStatus::Disabled => 5,
    }
}

fn install_or_docker_suggestion(command: &str, image: &str) -> String {
    format!(
        "For faster runs install natively: {} Or keep Docker fallback with image `{image}`.",
        install_hint(command)
    )
}

fn install_hint(command: &str) -> String {
    let name = normalized_command(command);
    let package = package_name(name);
    let command_hint = match std::env::consts::OS {
        "windows" => windows_install(package),
        "macos" => format!("brew install {package}"),
        "linux" => linux_install(package),
        _ => format!("install `{package}` from your package manager"),
    };

    match docs_url(name) {
        Some(url) => format!("{command_hint} | docs: {url}"),
        None => command_hint,
    }
}

fn normalized_command(command: &str) -> &str {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .trim_end_matches(".exe")
}

fn package_name(command: &str) -> &str {
    match command {
        "gofmt" | "go" => "go",
        "taplo" => "taplo-cli",
        "docker" => "Docker Desktop",
        "markdownlint" => "markdownlint-cli",
        other => other,
    }
}

fn windows_install(package: &str) -> String {
    match package {
        "Docker Desktop" => "winget install Docker.DockerDesktop".to_owned(),
        "terraform" => "winget install Hashicorp.Terraform".to_owned(),
        "go" => "winget install GoLang.Go".to_owned(),
        "prettier" => "winget install OpenJS.NodeJS.LTS; npm install -g prettier".to_owned(),
        "markdownlint-cli" => {
            "winget install OpenJS.NodeJS.LTS; npm install -g markdownlint-cli".to_owned()
        }
        "ruff" => "winget install astral-sh.ruff".to_owned(),
        "tflint" => "scoop install tflint".to_owned(),
        "tfsec" => "scoop install tfsec".to_owned(),
        "hadolint" => "scoop install hadolint".to_owned(),
        "shfmt" => "scoop install shfmt".to_owned(),
        "taplo-cli" => "cargo install taplo-cli --locked".to_owned(),
        "checkov" => "pipx install checkov".to_owned(),
        "golangci-lint" => "winget install GolangCI.golangci-lint".to_owned(),
        "composer" => "winget install Composer.Composer".to_owned(),
        "php-cs-fixer" => "composer global require friendsofphp/php-cs-fixer".to_owned(),
        "phpstan" => "composer global require phpstan/phpstan".to_owned(),
        other => format!("winget search {other}"),
    }
}

fn linux_install(package: &str) -> String {
    match package {
        "Docker Desktop" => {
            "install Docker Engine: https://docs.docker.com/engine/install/".to_owned()
        }
        "terraform" => {
            "install Terraform: https://developer.hashicorp.com/terraform/install".to_owned()
        }
        "prettier" => "npm install -g prettier".to_owned(),
        "markdownlint-cli" => "npm install -g markdownlint-cli".to_owned(),
        "ruff" => "curl -LsSf https://astral.sh/ruff/install.sh | sh".to_owned(),
        "taplo-cli" => "cargo install taplo-cli --locked".to_owned(),
        "checkov" => "pipx install checkov".to_owned(),
        "php-cs-fixer" => "composer global require friendsofphp/php-cs-fixer".to_owned(),
        "phpstan" => "composer global require phpstan/phpstan".to_owned(),
        other => format!("sudo apt install {other}  # or use your distro package manager"),
    }
}

fn docs_url(command: &str) -> Option<&'static str> {
    match command {
        "docker" => Some("https://docs.docker.com/get-started/get-docker/"),
        "terraform" => Some("https://developer.hashicorp.com/terraform/install"),
        "tflint" => Some("https://github.com/terraform-linters/tflint"),
        "tfsec" => Some("https://github.com/aquasecurity/tfsec"),
        "hadolint" => Some("https://github.com/hadolint/hadolint"),
        "ruff" => Some("https://docs.astral.sh/ruff/installation/"),
        "prettier" => Some("https://prettier.io/docs/install"),
        "shfmt" => Some("https://github.com/mvdan/sh"),
        "taplo" => Some("https://taplo.tamasfe.dev/cli/installation/"),
        "checkov" => Some("https://www.checkov.io/2.Basics/Installing%20Checkov.html"),
        "gofmt" | "go" => Some("https://go.dev/doc/install"),
        "golangci-lint" => Some("https://golangci-lint.run/welcome/install/"),
        "php-cs-fixer" => Some("https://cs.symfony.com/"),
        "phpstan" => Some("https://phpstan.org/user-guide/getting-started"),
        "composer" => Some("https://getcomposer.org/download/"),
        "markdownlint" => Some("https://github.com/igorshubovych/markdownlint-cli"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{install_hint, normalized_command};

    #[test]
    fn install_hints_include_documentation_urls() {
        let hint = install_hint("terraform");

        assert!(hint.contains("terraform") || hint.contains("Terraform"));
        assert!(hint.contains("https://developer.hashicorp.com/terraform/install"));
    }

    #[test]
    fn command_normalization_handles_windows_extensions() {
        assert_eq!(normalized_command("ruff.exe"), "ruff");
    }
}
