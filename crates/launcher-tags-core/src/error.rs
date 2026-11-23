use thiserror::Error;

/// Errors that abort a conversion. Messages never contain backup values (tags, notes, …);
/// at most structural identifiers such as a member name, page number or preference key.
#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("not a readable ZIP archive: {0}")]
    Zip(String),

    #[error("unsafe archive entry #{index} (absolute path, '..' component or invalid name); refusing to read this archive")]
    UnsafeArchiveEntry { index: usize },

    #[error("archive limit exceeded: {0}")]
    ArchiveLimit(String),

    #[error("invalid JSON in {context}: {message}")]
    Json { context: String, message: String },

    #[error("unexpected structure: {0}")]
    Schema(String),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Kvaesitso backup contains no customizations.NNNN members; refusing to produce a misleading result")]
    NoCustomizations,

    #[error("existing preference '{key}' is not a {{\"type\":\"string\",\"value\":\"...\"}} entry; refusing to overwrite it")]
    NicknameTypeCollision { key: String },
}

impl Error {
    pub(crate) fn json(context: impl Into<String>, err: &serde_json::Error) -> Self {
        // serde_json error messages contain only the position and category, not input data.
        Error::Json {
            context: context.into(),
            message: err.to_string(),
        }
    }
}
