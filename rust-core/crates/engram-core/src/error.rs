use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum EngramError {
    #[error("Could not access {}: {}.", path.display(), io_reason(source))]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{} is not a valid .engram file: {message}", path.display())]
    Parse { path: PathBuf, message: String },
    #[error("{}", sentence(.0))]
    Invalid(String),
    #[error("Could not find {0}.")]
    NotFound(String),
    #[error("{}", sentence(.0))]
    Conflict(String),
    #[error("Could not read the JSON input: {0}")]
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

fn io_reason(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => "the file or folder does not exist".into(),
        PermissionDenied => "permission was denied; check the file permissions".into(),
        AlreadyExists => "a file with that name already exists".into(),
        ReadOnlyFilesystem => "the disk is read-only".into(),
        StorageFull => "the disk is full".into(),
        kind => kind.to_string(),
    }
}

fn sentence(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 1);
    let mut chars = s.chars();
    if let Some(first) = chars.next() {
        out.extend(first.to_uppercase());
        out.push_str(chars.as_str());
    }
    if !out.ends_with(['.', '?', ')']) {
        out.push('.');
    }
    out
}

pub type Result<T> = std::result::Result<T, EngramError>;

pub(crate) fn invalid<T>(msg: impl Into<String>) -> Result<T> {
    Err(EngramError::Invalid(msg.into()))
}
