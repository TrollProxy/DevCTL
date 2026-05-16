# Security Policy

## Supported Versions

`devctl` is currently pre-1.0. Security fixes should target the default branch unless a maintained release branch exists.

## Reporting a Vulnerability

Please do not open public issues for sensitive security reports. Use your organization's private reporting channel or GitHub private vulnerability reporting if enabled.

Include:

- affected version or commit
- operating system
- command and config involved
- impact
- reproduction steps
- suggested fix, if known

## Security Principles

- No shell evaluation for config-defined commands.
- Terraform plan and validate run in isolated sandboxes.
- Cloud credentials are not intentionally forwarded into Terraform sandbox commands.
- New filesystem-destructive behavior requires explicit user intent and tests.

