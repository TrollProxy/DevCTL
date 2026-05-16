use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DevctlError {
    #[error("required tool `{program}` was not found on PATH. {suggestion}")]
    MissingTool { program: String, suggestion: String },

    #[error("custom command `{0}` is not configured")]
    UnknownCommand(String),

    #[error("refusing to overwrite existing file `{0}`; pass --force to replace it")]
    RefuseOverwrite(PathBuf),

    #[error("terraform version `{actual}` does not match configured pin `{expected}`")]
    TerraformVersion { expected: String, actual: String },

    #[error("could not parse ignore pattern `{pattern}`")]
    InvalidIgnorePattern {
        pattern: String,
        source: globset::Error,
    },
}
