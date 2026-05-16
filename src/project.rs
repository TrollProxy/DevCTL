use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::Config;
use crate::fs as fs_util;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectKind {
    Terraform,
    Docker,
    Rust,
    Node,
    Python,
    Go,
    Php,
    Generic,
}

impl ProjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Terraform => "terraform",
            Self::Docker => "docker",
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Python => "python",
            Self::Go => "go",
            Self::Php => "php",
            Self::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub root: PathBuf,
    pub kinds: BTreeSet<ProjectKind>,
}

impl Project {
    pub fn kind_labels(&self) -> String {
        self.kinds
            .iter()
            .map(|kind| kind.label())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn discover(root: &Path, config: &Config, workspace: bool) -> Result<Vec<Project>> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let files = fs_util::discover_files(&root, &config.effective_ignore())?;

    if !workspace {
        let kinds = detect_kinds(&root, &files);
        return Ok(vec![Project { root, kinds }]);
    }

    let mut projects: BTreeMap<PathBuf, BTreeSet<ProjectKind>> = BTreeMap::new();
    for file in &files {
        for (kind, project_root) in markers_for_file(&root, file) {
            projects.entry(project_root).or_default().insert(kind);
        }
    }

    if projects.is_empty() {
        let kinds = detect_kinds(&root, &files);
        return Ok(vec![Project { root, kinds }]);
    }

    Ok(projects
        .into_iter()
        .map(|(root, kinds)| Project { root, kinds })
        .collect())
}

pub fn has_kind(root: &Path, config: &Config, kind: ProjectKind) -> Result<bool> {
    let files = fs_util::discover_files(root, &config.effective_ignore())?;
    Ok(detect_kinds(root, &files).contains(&kind))
}

fn detect_kinds(root: &Path, files: &[PathBuf]) -> BTreeSet<ProjectKind> {
    let mut kinds = BTreeSet::new();
    for file in files {
        for (kind, _) in markers_for_file(root, file) {
            kinds.insert(kind);
        }
    }
    kinds
}

fn markers_for_file(root: &Path, file: &Path) -> Vec<(ProjectKind, PathBuf)> {
    let mut markers = Vec::new();
    let relative = file.strip_prefix(root).unwrap_or(file);
    let name = file_name(file);
    let extension = file_extension(file);
    let parent = file.parent().unwrap_or(root).to_path_buf();

    if extension == "tf" {
        markers.push((ProjectKind::Terraform, parent.clone()));
    }
    if name == "Dockerfile" || name.starts_with("Dockerfile.") {
        markers.push((ProjectKind::Docker, parent.clone()));
    }
    if name == "Cargo.toml" {
        markers.push((ProjectKind::Rust, parent.clone()));
    }
    if name == "package.json" {
        markers.push((ProjectKind::Node, parent.clone()));
    }
    if matches!(
        name.as_str(),
        "pyproject.toml" | "requirements.txt" | "setup.py"
    ) {
        markers.push((ProjectKind::Python, parent.clone()));
    }
    if name == "go.mod" {
        markers.push((ProjectKind::Go, parent.clone()));
    }
    if name == "composer.json" {
        markers.push((ProjectKind::Php, parent));
    }

    if markers.is_empty() && relative.components().count() == 1 {
        match extension.as_str() {
            "py" => markers.push((ProjectKind::Python, root.to_path_buf())),
            "go" => markers.push((ProjectKind::Go, root.to_path_buf())),
            "php" => markers.push((ProjectKind::Php, root.to_path_buf())),
            "rs" => markers.push((ProjectKind::Rust, root.to_path_buf())),
            _ => {}
        }
    }

    markers
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned()
}

fn file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn detects_single_project_kinds() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("main.tf"), "terraform {}\n").unwrap();
        fs::write(temp.path().join("Dockerfile"), "FROM scratch\n").unwrap();

        let projects = discover(temp.path(), &Config::default(), false).unwrap();

        assert_eq!(projects.len(), 1);
        assert!(projects[0].kinds.contains(&ProjectKind::Terraform));
        assert!(projects[0].kinds.contains(&ProjectKind::Docker));
    }

    #[test]
    fn workspace_discovers_nested_project_roots() {
        let temp = tempfile::tempdir().unwrap();
        let api = temp.path().join("api");
        let infra = temp.path().join("infra");
        fs::create_dir_all(&api).unwrap();
        fs::create_dir_all(&infra).unwrap();
        fs::write(api.join("package.json"), "{}\n").unwrap();
        fs::write(infra.join("main.tf"), "terraform {}\n").unwrap();

        let projects = discover(temp.path(), &Config::default(), true).unwrap();

        assert_eq!(projects.len(), 2);
    }
}
