use std::path::{Path, PathBuf};

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;

use crate::error::DevctlError;

pub fn discover_files(root: &Path, ignores: &[String]) -> Result<Vec<PathBuf>> {
    let ignore_set = compile_ignores(ignores)?;
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut builder = WalkBuilder::new(&root);
    builder
        .standard_filters(true)
        .hidden(false)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .add_custom_ignore_filename(".devctlignore");

    let mut files = Vec::new();
    for entry in builder.build() {
        let entry = entry?;
        let path = entry.path();
        if should_ignore(&root, path, &ignore_set) {
            continue;
        }
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() && !file_type.is_symlink() {
            files.push(path.to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

pub fn compile_ignores(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(
            Glob::new(pattern).map_err(|source| DevctlError::InvalidIgnorePattern {
                pattern: pattern.clone(),
                source,
            })?,
        );
    }
    Ok(builder.build()?)
}

pub fn path_to_slash(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn should_ignore(root: &Path, path: &Path, ignore_set: &GlobSet) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.as_os_str().is_empty() {
        return false;
    }
    ignore_set.is_match(path_to_slash(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_paths_are_cross_platform() {
        let path = PathBuf::from("a").join("b").join("file.json");
        assert_eq!(path_to_slash(&path), "a/b/file.json");
    }
}
