# devctl

`devctl` is a small, fast DevOps CLI for project hygiene tasks: recursive formatting, isolated Terraform planning, and configured team commands.

## Features

- Recursive `devctl fmt` with `.gitignore` and `.devctlignore` support.
- Pre-commit friendly `devctl check` pipeline for formatting, linting, and validation.
- Config-driven tool registries for formatters, linters, and validators across any language.
- Configurable `devctl lint` for `tflint`, `tfsec`, `hadolint`, optional `checkov`, Python, PHP, Go, Rust, JS/TS, and custom tools.
- `devctl docker lint` with native `hadolint` and Docker image fallback.
- `devctl validate` for sandboxed Terraform validation plus custom validators.
- Built-in JSON formatting and external formatters for Terraform, Rust, TOML, YAML, Markdown, and shell scripts.
- Safe `devctl terraform plan` in a temporary sandbox with `terraform init -backend=false`, `terraform plan -json`, `-refresh=false`, and scrubbed credentials.
- Shipped project `devctl.yaml`, local override support, and global config discovery.
- `devctl run <name>` for config-defined commands without shell evaluation.
- `devctl --doctor` for a friendly system/tooling health report.
- Shell completion and man page generation.

## Install

```bash
cargo install --path .
```

For a compact release binary:

```bash
cargo build --release
```

The release profile enables LTO, symbol stripping, size optimization, and `panic = "abort"`.

## Usage

```bash
devctl --help
devctl fmt
devctl --dry-run fmt
devctl lint
devctl validate
devctl docker lint
devctl docker lint --fix
devctl check
devctl check --workspace
devctl --doctor
devctl terraform plan
devctl terraform plan --detailed
devctl terraform validate
devctl run test
devctl completions bash > devctl.bash
devctl man > devctl.1
```

Exit codes are stable for CI:

- `0`: success
- `1`: error
- `2`: changes detected, such as Terraform plan changes or `fmt --dry-run`

## Configuration

`devctl` loads global config first, then a local override:

- Global: platform config dir `devctl/config.yaml`, and `~/.config/devctl/config.yaml` when present
- Local: nearest `devctl.yaml` in the current directory or one of its parents
- Explicit: `--config path/to/devctl.yaml`

This repository ships a ready-to-use [`devctl.yaml`](devctl.yaml). Use it as the baseline for real projects, then trim or extend tools to match the team's stack. The longer commented template in [`examples/devctl.yaml`](examples/devctl.yaml) explains every supported field.

Create an example config:

```bash
devctl init-config
```

## Doctor

Run a local environment health check:

```bash
devctl --doctor
```

The doctor checks:

- loaded config sources
- Terraform and Docker runtime availability
- every enabled formatter, linter, validator, Docker tool, and custom command
- whether native CLIs are installed and can answer a version probe
- whether Docker fallback is available when native tools are missing

`devctl` always prefers native CLIs first. Docker is only a fallback when configured and available. If Docker is missing or the daemon is stopped, the report points users toward native installs instead of failing cryptically. Missing tools include OS-aware install hints for Windows, macOS, and Linux, plus documentation URLs where a package manager command is not universal.

Tools live in registry sections. Each tool can define `globs`, per-tool `ignore`, `command`, `args`, `check_args`, `extra_args`, `env`, `cwd`, and optional Docker fallback metadata.

```yaml
global:
  ignore: ["**/vendor/**", "**/node_modules/**"]
  docker_fallback: true

formatters:
  python:
    enabled: true
    globs: ["**/*.py"]
    command: ruff
    args: ["format", "--quiet", "{file}"]
    check_args: ["format", "--check", "--quiet", "{file}"]

linters:
  actionlint:
    enabled: true
    globs: [".github/workflows/*.yml", ".github/workflows/*.yaml"]
    command: actionlint
    args: ["{file}"]

validators:
  kubeconform:
    enabled: true
    globs: ["k8s/**/*.yaml"]
    command: kubeconform
    args: ["-strict", "{file}"]
```

Placeholders are `{root}`, `{file}`, `{dir}`, and `{relative}`. Tools run per file when args contain `{file}`, per directory when args contain `{dir}`, and once from the project root otherwise.

## Formatter Behavior

`devctl fmt` is fully registry-driven. Defaults cover Terraform, JSON, Rust, Python, Go, JS/TS, YAML, Markdown, TOML, shell, and disabled PHP. JSON uses the built-in formatter via `command: "builtin:json"`; everything else is an explicit argv-based command from config.

Missing tools are reported as skipped, not as hard failures, so teams can adopt tooling incrementally.

## Linting and Validation

`devctl lint` discovers files with the same ignore-aware traversal as `fmt`. It runs Terraform linters per module directory, Dockerfile linting per Dockerfile, optional Checkov, and any custom configured linters.

Default tools:

- Terraform: `tflint`, `tfsec`
- Docker: `hadolint`
- Optional: `checkov`

Missing tools are shown as skipped with a suggestion. For `hadolint`, `devctl` can fall back to:

```bash
docker run --rm -i hadolint/hadolint < Dockerfile
```

`devctl validate` runs registry validators. The default Terraform validator uses `command: "builtin:terraform-validate"` and keeps the isolated sandbox model from Terraform plan.

For pre-commit or CI, use:

```bash
devctl check
```

`devctl check` now starts with project detection. In the current project it recognizes Terraform, Docker, Rust, Node, Python, Go, and PHP markers, then runs project-specific checks before the generic formatter/linter/validator registries. Examples:

- Terraform: isolated `terraform validate`, isolated `terraform plan`, and `terraform test` when `*.tftest.hcl` files exist
- Docker: `docker build --pull=false` for detected Dockerfiles
- Rust: `cargo test --all`
- Node: `npm test`
- Python: `python -m pytest`
- Go: `go test ./...`
- PHP: `composer validate --strict`

Use `--dry-run` to see what would run without requiring every tool to be installed:

```bash
devctl --dry-run check
```

Use `--workspace` when a repo contains multiple project roots:

```bash
devctl --dry-run check --workspace
devctl check --workspace
```

`fmt`, `lint`, and `validate` also accept `--workspace` to run their registries once per detected project root.

Example `.pre-commit-config.yaml` entry:

```yaml
repos:
  - repo: local
    hooks:
      - id: devctl-check
        name: devctl check
        entry: devctl check
        language: system
        pass_filenames: false
```

## Terraform safety model

`devctl terraform plan` never runs in your source tree. It copies Terraform-relevant files into a temporary directory, strips the child process environment, disables backend initialization, disables refresh, and parses Terraform JSON UI events for a concise report.

This design prevents accidental state writes and avoids passing local cloud credentials into Terraform. Some provider configurations can still fail without credentials; when that happens `devctl` reports the diagnostic and exits with code `1`.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
devctl --doctor
```

## Contributing

Keep changes small and covered by tests. Prefer secure, cross-platform process execution with explicit argv arrays instead of shell strings. New formatters should support dry-run checks whenever the underlying tool provides them.
