# Copilot Instructions

This repository contains `devctl`, a Rust 2024 enterprise DevOps CLI.

Follow these rules for all suggestions:

- Preserve the existing module architecture. Do not collapse everything into `main.rs`.
- Prefer config-driven tool registry changes in `src/tools.rs` and `src/config.rs` over hardcoded per-language behavior.
- Never suggest shell-evaluated commands for user config. Use explicit argv arrays.
- Keep all changes cross-platform for Windows, macOS, and Linux.
- Respect `.gitignore`, `.devctlignore`, `global.ignore`, and per-tool ignores.
- Keep `#![forbid(unsafe_code)]`.
- Avoid new dependencies unless they are small and clearly justified.
- Maintain stable exit codes: `0` success, `1` error/issues, `2` changes detected.
- Add or update integration tests for user-visible CLI behavior.
- Update `examples/devctl.yaml` and `README.md` when config or UX changes.

Before considering a change complete, run:

```bash
cargo fmt --all -- --check
cargo check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cargo build --release
cargo run -- --doctor
```

