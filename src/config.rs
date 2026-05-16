use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::{BaseDirs, ProjectDirs};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::error::DevctlError;

pub const EXAMPLE_CONFIG: &str = include_str!("../examples/devctl.yaml");

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub sources: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    #[serde(alias = "ignore_paths")]
    pub ignore: Vec<String>,
    pub global: GlobalConfig,
    pub terraform: TerraformConfig,
    pub format: FormatConfig,
    pub formatters: ToolRegistryConfig,
    pub linters: ToolRegistryConfig,
    pub validators: ToolRegistryConfig,
    pub docker: DockerConfig,
    pub commands: BTreeMap<String, CustomCommand>,
}

impl Config {
    pub fn effective_ignore(&self) -> Vec<String> {
        let mut ignore = self.ignore.clone();
        ignore.extend(self.global.ignore.iter().cloned());
        ignore.sort();
        ignore.dedup();
        ignore
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalConfig {
    pub ignore: Vec<String>,
    pub docker_fallback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TerraformConfig {
    pub binary: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatConfig {
    pub enabled: bool,
    pub terraform: FormatterSwitch,
    pub yaml: FormatterSwitch,
    pub json: FormatterSwitch,
    pub rust: FormatterSwitch,
    pub markdown: FormatterSwitch,
    pub toml: FormatterSwitch,
    pub shell: FormatterSwitch,
    pub custom: BTreeMap<String, CustomFormatter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatterSwitch {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomFormatter {
    pub enabled: bool,
    pub extensions: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub check_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolRegistryConfig {
    pub enabled: bool,
    pub custom: BTreeMap<String, CustomCommand>,
    #[serde(flatten)]
    pub tools: BTreeMap<String, ToolConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DockerConfig {
    pub enabled: bool,
    pub hadolint: ToolConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolConfig {
    pub enabled: bool,
    pub globs: Vec<String>,
    pub ignore: Vec<String>,
    pub command: String,
    pub args: Vec<String>,
    pub check_args: Vec<String>,
    pub extra_args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub docker_fallback: bool,
    pub docker_image: Option<String>,
    pub docker_args: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomCommand {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            ignore: vec![
                ".git/**".to_owned(),
                ".terraform/**".to_owned(),
                "target/**".to_owned(),
                "node_modules/**".to_owned(),
            ],
            global: GlobalConfig::default(),
            terraform: TerraformConfig::default(),
            format: FormatConfig::default(),
            formatters: default_formatters(),
            linters: default_linters(),
            validators: default_validators(),
            docker: DockerConfig::default(),
            commands: BTreeMap::new(),
        }
    }
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            ignore: Vec::new(),
            docker_fallback: true,
        }
    }
}

impl Default for TerraformConfig {
    fn default() -> Self {
        Self {
            binary: "terraform".to_owned(),
            version: None,
        }
    }
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            terraform: FormatterSwitch::default(),
            yaml: FormatterSwitch::default(),
            json: FormatterSwitch::default(),
            rust: FormatterSwitch::default(),
            markdown: FormatterSwitch::default(),
            toml: FormatterSwitch::default(),
            shell: FormatterSwitch::default(),
            custom: BTreeMap::new(),
        }
    }
}

impl Default for FormatterSwitch {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for CustomFormatter {
    fn default() -> Self {
        Self {
            enabled: true,
            extensions: Vec::new(),
            command: String::new(),
            args: Vec::new(),
            check_args: Vec::new(),
        }
    }
}

impl Default for ToolRegistryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            custom: BTreeMap::new(),
            tools: BTreeMap::new(),
        }
    }
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hadolint: ToolConfig {
                enabled: true,
                globs: dockerfile_globs().into_iter().map(str::to_owned).collect(),
                ignore: Vec::new(),
                command: "hadolint".to_owned(),
                args: vec!["{file}".to_owned()],
                check_args: Vec::new(),
                extra_args: Vec::new(),
                cwd: None,
                env: BTreeMap::new(),
                docker_fallback: true,
                docker_image: Some("hadolint/hadolint".to_owned()),
                docker_args: Vec::new(),
            },
        }
    }
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            globs: Vec::new(),
            ignore: Vec::new(),
            command: String::new(),
            args: Vec::new(),
            check_args: Vec::new(),
            extra_args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            docker_fallback: false,
            docker_image: None,
            docker_args: Vec::new(),
        }
    }
}

fn default_formatters() -> ToolRegistryConfig {
    registry([
        tool(
            "terraform",
            true,
            ["**/*.tf", "**/*.tfvars"],
            "terraform",
            ["fmt", "{dir}"],
            ["fmt", "-check", "-diff", "{dir}"],
        ),
        tool("json", true, ["**/*.json"], "builtin:json", [], []),
        tool(
            "rust",
            true,
            ["**/*.rs"],
            "rustfmt",
            ["{file}"],
            ["--check", "{file}"],
        ),
        tool(
            "python",
            true,
            ["**/*.py"],
            "ruff",
            ["format", "--quiet", "{file}"],
            ["format", "--check", "--quiet", "{file}"],
        ),
        tool(
            "go",
            true,
            ["**/*.go"],
            "gofmt",
            ["-w", "{file}"],
            ["-l", "{file}"],
        ),
        tool(
            "javascript",
            true,
            [
                "**/*.js",
                "**/*.jsx",
                "**/*.ts",
                "**/*.tsx",
                "**/*.css",
                "**/*.html",
            ],
            "prettier",
            ["--write", "{file}"],
            ["--check", "{file}"],
        ),
        tool(
            "yaml",
            true,
            ["**/*.yaml", "**/*.yml"],
            "prettier",
            ["--write", "{file}"],
            ["--check", "{file}"],
        ),
        tool(
            "markdown",
            true,
            ["**/*.md", "**/*.markdown"],
            "prettier",
            ["--write", "{file}"],
            ["--check", "{file}"],
        ),
        tool(
            "toml",
            true,
            ["**/*.toml"],
            "taplo",
            ["fmt", "{file}"],
            ["fmt", "--check", "{file}"],
        ),
        tool(
            "shell",
            true,
            ["**/*.sh", "**/*.bash", "**/*.zsh"],
            "shfmt",
            ["-w", "{file}"],
            ["-d", "{file}"],
        ),
        tool(
            "php",
            false,
            ["**/*.php"],
            "php-cs-fixer",
            ["fix", "--quiet", "{file}"],
            [],
        ),
    ])
}

fn default_linters() -> ToolRegistryConfig {
    let mut registry = registry([
        tool(
            "tflint",
            true,
            ["**/*.tf"],
            "tflint",
            ["--no-color", "--chdir", "{dir}"],
            [],
        ),
        tool(
            "tfsec",
            true,
            ["**/*.tf"],
            "tfsec",
            ["--no-color", "{dir}"],
            [],
        ),
        tool(
            "hadolint",
            true,
            dockerfile_globs(),
            "hadolint",
            ["{file}"],
            [],
        ),
        tool(
            "checkov",
            false,
            [
                "**/*.tf",
                "**/*.tfvars",
                "**/*.yaml",
                "**/*.yml",
                "**/*.json",
            ],
            "checkov",
            ["-d", "{root}", "--quiet"],
            [],
        ),
        tool("python", true, ["**/*.py"], "ruff", ["check", "{file}"], []),
        tool(
            "rust",
            false,
            ["Cargo.toml", "**/Cargo.toml"],
            "cargo",
            [
                "clippy",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
            [],
        ),
        tool("go", false, ["**/*.go"], "golangci-lint", ["run"], []),
        tool(
            "php",
            false,
            ["**/*.php"],
            "phpstan",
            ["analyse", "{file}"],
            [],
        ),
        tool(
            "markdown",
            false,
            ["**/*.md", "**/*.markdown"],
            "markdownlint",
            ["{file}"],
            [],
        ),
    ]);
    if let Some(hadolint) = registry.tools.get_mut("hadolint") {
        hadolint.docker_fallback = true;
        hadolint.docker_image = Some("hadolint/hadolint".to_owned());
    }
    if let Some(tflint) = registry.tools.get_mut("tflint") {
        tflint.docker_image = Some("ghcr.io/terraform-linters/tflint".to_owned());
    }
    if let Some(tfsec) = registry.tools.get_mut("tfsec") {
        tfsec.docker_image = Some("aquasec/tfsec".to_owned());
    }
    registry
}

fn default_validators() -> ToolRegistryConfig {
    registry([
        tool(
            "terraform",
            true,
            ["**/*.tf"],
            "builtin:terraform-validate",
            ["{dir}"],
            [],
        ),
        tool("go", false, ["go.mod"], "go", ["test", "./..."], []),
        tool("node", false, ["package.json"], "npm", ["test"], []),
        tool(
            "php",
            false,
            ["composer.json"],
            "composer",
            ["validate", "--strict"],
            [],
        ),
        tool(
            "python",
            false,
            ["pyproject.toml"],
            "python",
            ["-m", "pytest"],
            [],
        ),
    ])
}

fn registry<const N: usize>(tools: [(&str, ToolConfig); N]) -> ToolRegistryConfig {
    ToolRegistryConfig {
        enabled: true,
        custom: BTreeMap::new(),
        tools: tools
            .into_iter()
            .map(|(name, tool)| (name.to_owned(), tool))
            .collect(),
    }
}

fn tool<G, A, C>(
    name: &'static str,
    enabled: bool,
    globs: G,
    command: &'static str,
    args: A,
    check_args: C,
) -> (&'static str, ToolConfig)
where
    G: IntoIterator<Item = &'static str>,
    A: IntoIterator<Item = &'static str>,
    C: IntoIterator<Item = &'static str>,
{
    (
        name,
        ToolConfig {
            enabled,
            globs: globs.into_iter().map(str::to_owned).collect(),
            ignore: Vec::new(),
            command: command.to_owned(),
            args: args.into_iter().map(str::to_owned).collect(),
            check_args: check_args.into_iter().map(str::to_owned).collect(),
            extra_args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            docker_fallback: false,
            docker_image: None,
            docker_args: Vec::new(),
        },
    )
}

fn dockerfile_globs() -> Vec<&'static str> {
    vec![
        "Dockerfile",
        "Dockerfile.*",
        "**/Dockerfile",
        "**/Dockerfile.*",
    ]
}

pub fn load(explicit_config: Option<&Path>, cwd: &Path) -> Result<LoadedConfig> {
    let mut merged = serde_yaml::to_value(Config::default())?;
    let mut sources = Vec::new();

    for path in global_config_candidates() {
        merge_file_if_exists(&mut merged, &path, &mut sources)?;
    }

    if let Some(path) = explicit_config {
        merge_file_if_exists(&mut merged, path, &mut sources)?;
    } else if let Some(path) = find_local_config(cwd) {
        merge_file_if_exists(&mut merged, &path, &mut sources)?;
    }

    let config = serde_yaml::from_value(merged).context("invalid devctl configuration")?;
    Ok(LoadedConfig { config, sources })
}

pub fn write_example(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(DevctlError::RefuseOverwrite(path.to_path_buf()).into());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::write(path, EXAMPLE_CONFIG)
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}

fn merge_file_if_exists(merged: &mut Value, path: &Path, sources: &mut Vec<PathBuf>) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }

    let text = fs::read_to_string(path)
        .with_context(|| format!("could not read config {}", path.display()))?;
    let value: Value = serde_yaml::from_str(&text)
        .with_context(|| format!("could not parse config {}", path.display()))?;
    deep_merge(merged, value);
    sources.push(path.to_path_buf());
    Ok(())
}

fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Mapping(base_map), Value::Mapping(overlay_map)) => {
            merge_mapping(base_map, overlay_map);
        }
        (base_slot, overlay_value) => *base_slot = overlay_value,
    }
}

fn merge_mapping(base: &mut Mapping, overlay: Mapping) {
    for (key, overlay_value) in overlay {
        match base.get_mut(&key) {
            Some(base_value) => deep_merge(base_value, overlay_value),
            None => {
                base.insert(key, overlay_value);
            }
        }
    }
}

fn global_config_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(project_dirs) = ProjectDirs::from("", "", "devctl") {
        candidates.push(project_dirs.config_dir().join("config.yaml"));
    }
    if let Some(base_dirs) = BaseDirs::new() {
        candidates.push(base_dirs.home_dir().join(".config/devctl/config.yaml"));
    }

    candidates.sort();
    candidates.dedup();
    candidates
}

fn find_local_config(cwd: &Path) -> Option<PathBuf> {
    for directory in cwd.ancestors() {
        let candidate = directory.join("devctl.yaml");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_values_override_defaults_without_losing_nested_defaults() {
        let mut base = serde_yaml::to_value(Config::default()).unwrap();
        let overlay: Value = serde_yaml::from_str(
            r#"
format:
  json:
    enabled: false
"#,
        )
        .unwrap();

        deep_merge(&mut base, overlay);
        let config: Config = serde_yaml::from_value(base).unwrap();

        assert!(!config.format.json.enabled);
        assert!(config.format.terraform.enabled);
    }
}
