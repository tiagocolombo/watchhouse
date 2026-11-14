#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    #[error("record not found")]
    NotFound,
    #[error("persistence failure: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("invalid persisted contract: {0}")]
    Contract(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
