# AGENTS.md

Guidance for AI agents and maintainers working on `devctl`.

`devctl` is a production-oriented Rust 2024 DevOps CLI. Treat this repository like an enterprise CLI used in pre-commit hooks and CI: changes must be small, tested, cross-platform, warning-free, secure, and compatible with the existing architecture.

## Project Goals

- Provide a single fast `devctl` binary for local DevOps workflows.
- Keep startup fast and release binaries small, preferably under 3 MB.
- Support recursive, ignore-aware processing using `.gitignore`, `.devctlignore`, `ignore`, `global.ignore`, and per-tool ignores.
- Make `fmt`, `lint`, `validate`, and `check` extensible through config-driven tool registries.
- Avoid shell evaluation. Always execute commands as explicit argv arrays.
- Stay cross-platform for Windows, macOS, and Linux.

## Architecture

- `src/main.rs`: tiny binary entrypoint and top-level error display.
- `src/lib.rs`: command dispatch, logging setup, exit-code orchestration.
- `src/cli.rs`: Clap derive API and subcommand definitions.
- `src/config.rs`: `devctl.yaml` schema, defaults, global/local merge behavior.
- `src/tools.rs`: generic config-driven formatter/linter/validator registry execution.
- `src/fmt.rs`: `devctl fmt` report adapter over the tool registry.
- `src/lint.rs`: `devctl lint` report adapter plus Docker lint inclusion.
- `src/validate.rs`: `devctl validate` report adapter over the tool registry.
- `src/docker.rs`: Dockerfile discovery and `hadolint` native/Docker fallback.
- `src/terraform.rs`: isolated Terraform plan/validate sandbox and JSON parsing.
- `src/fs.rs`: ignore-aware recursive file discovery and glob helpers.
- `src/process.rs`: safe process execution helpers.
- `src/output.rs`: human-readable colored reports.
- `src/error.rs`: typed application errors.
- `src/command.rs`: `devctl run <name>` custom command execution.
- `tests/cli.rs`: integration tests against the real binary.
- `devctl.yaml`: shipped project default config.
- `examples/devctl.yaml`: full commented example config.

## Command Surface

Primary commands:

```bash
devctl fmt
devctl lint
devctl validate
devctl docker lint
devctl check
devctl check --workspace
devctl --doctor
devctl terraform plan
devctl terraform validate
devctl run <custom>
devctl completions <shell>
devctl man
devctl init-config
```

Global flags:

```bash
--config <FILE>
--verbose / -v
--quiet
--dry-run
```

Stable exit codes:

- `0`: success
- `1`: error or issues detected by lint/validate/check
- `2`: changes detected, such as `fmt --dry-run` or Terraform plan changes

## Doctor

`devctl --doctor` is the preferred first diagnostic for user environment issues. It checks config loading, Terraform, Docker, all enabled registry tools, Docker fallback readiness, and custom commands.

Doctor output should stay grouped, readable, and friendly in narrow terminals. Missing tools should include OS-aware install commands or documentation URLs where possible. Prefer native CLI guidance first, then Docker fallback guidance when configured.

`devctl check` is project-aware. It should detect the current project type by marker files and run CI-style checks before registry fmt/lint/validate checks. Current built-in project checks include Terraform validate/plan/test, Docker build, Rust cargo test, Node npm test, Python pytest, Go tests, and Composer validation. `--workspace` means discover each project root in a multi-project repository and run checks per root.

Native CLIs should always be preferred over Docker fallback. If Docker is unavailable, output should guide the user toward installing the native CLI rather than failing cryptically.

## Configuration Model

The modern tool model is registry-based:

- `global.ignore`
- `global.docker_fallback`
- `formatters`
- `linters`
- `validators`
- `docker.hadolint`

Each registry tool can define:

- `enabled`
- `globs`
- `ignore`
- `command`
- `args`
- `check_args`
- `extra_args`
- `cwd`
- `env`
- `docker_fallback`
- `docker_image`
- `docker_args`

Supported placeholders:

- `{root}`: project root
- `{file}`: matched file target
- `{dir}`: matched file parent directory when running per directory
- `{relative}`: target relative to root

Execution mode is inferred:

- Args containing `{file}` run once per matched file.
- Args containing `{dir}` run once per unique matched directory.
- Args without target placeholders run once from the project root if any files match.

Built-in pseudo commands:

- `builtin:json`: built-in JSON formatter.
- `builtin:terraform-validate`: isolated Terraform validation through `terraform.rs`.

Keep legacy config compatibility unless there is a strong reason to remove it.

## Coding Standards

- Use Rust 2024 idioms.
- Keep `#![forbid(unsafe_code)]`.
- Prefer small modules with clear ownership.
- Use `anyhow` at command boundaries and `thiserror` for reusable typed errors.
- Use `serde` defaults for config compatibility.
- Use `rayon` only where independent work can safely run in parallel.
- Use `ignore` and `globset` for traversal/matching; do not hand-roll recursive walking.
- Never execute user config through a shell.
- Never concatenate command strings for execution.
- Preserve cross-platform path behavior.
- Keep comments useful and sparse.
- Do not introduce new dependencies unless they are genuinely necessary and lightweight.

## Security Rules

- Do not pass cloud credentials into Terraform sandbox commands.
- Keep Terraform workflows isolated with `tempfile`.
- Continue using `terraform init -backend=false`.
- Avoid writing to source trees during Terraform plan/validate.
- Treat config-defined commands as argv arrays, not shell snippets.
- Do not add destructive behavior without explicit user confirmation and tests.
- Docker fallback should be explicit and visible in output.

## Testing Requirements

Before finalizing any code change, run:

```bash
cargo fmt --all -- --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
cargo run -- --doctor
```

On Windows PowerShell:

```powershell
cargo fmt --all -- --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
```

Also check release binary size:

```powershell
(Get-Item .\target\release\devctl.exe).Length
```

For Linux/macOS:

```bash
wc -c target/release/devctl
```

If touching CLI UX, verify:

```bash
devctl --help
devctl fmt --help
devctl lint --help
devctl validate --help
devctl docker lint --help
devctl check --help
devctl --doctor
```

If touching config generation, verify:

```bash
devctl init-config --force
```

Use temp directories in tests. Do not rely on tools like Terraform, Docker, `ruff`, `tflint`, `tfsec`, `hadolint`, or Node/PHP/Go being installed unless the test explicitly stubs or avoids them.

## Test Coverage Expectations

Add or update integration tests when changing:

- CLI subcommands or flags
- exit-code behavior
- config loading or merging
- registry tool matching
- dry-run behavior
- per-tool ignore behavior
- Docker fallback behavior
- Terraform JSON parsing or sandbox behavior
- output summaries used by users or CI

Prefer integration tests in `tests/cli.rs` for user-visible behavior. Unit tests are appropriate for parsing, matching, and internal helpers.

## Development Workflow

1. Inspect existing modules before editing.
2. Keep changes narrowly scoped.
3. Update `examples/devctl.yaml` whenever config schema/default behavior changes.
4. Update `README.md` whenever command behavior or UX changes.
5. Add tests for new behavior.
6. Run the full verification suite.
7. Report commands run and any skipped checks.

## Release Expectations

Release profile is configured in `Cargo.toml`:

```toml
[profile.release]
codegen-units = 1
lto = "fat"
opt-level = "z"
panic = "abort"
strip = "symbols"
```

Do not loosen release profile settings without a measured reason.

## Common Pitfalls

- Do not reintroduce hardcoded formatter/linter logic when `tools.rs` can express it.
- Do not bypass `.gitignore`, `.devctlignore`, global ignores, or per-tool ignores.
- Do not treat missing optional tools as hard failures for default registries.
- Do not make tests depend on external binaries being installed.
- Do not change stable exit-code semantics casually.
- Do not remove shell completion or man page support.
- Do not add broad dependencies that increase binary size significantly.

## Useful Commands

```bash
cargo run -- --help
cargo run -- --dry-run fmt
cargo run -- --dry-run lint
cargo run -- --dry-run validate
cargo run -- --dry-run check
cargo run -- completions bash
cargo run -- man
```

On Windows:

```powershell
.\target\release\devctl.exe --help
.\target\release\devctl.exe --dry-run check
```

## Maintainer Notes

The preferred extension path is config, not new code. Add code only for reusable execution semantics, safe built-ins, or UX/reporting improvements that cannot be expressed through `devctl.yaml`.
