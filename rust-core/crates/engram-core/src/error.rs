use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum EngramError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid .engram file {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl EngramError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "io",
            Self::Parse { .. } => "parse",
            Self::Invalid(_) => "invalid",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::Json(_) => "json",
        }
    }
}

pub type Result<T> = std::result::Result<T, EngramError>;

pub(crate) fn invalid<T>(msg: impl Into<String>) -> Result<T> {
    Err(EngramError::Invalid(msg.into()))
}
