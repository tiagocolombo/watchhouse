#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Backpressure(String),
    #[error("record not found")]
    NotFound,
    #[error("persistence failure: {0}")]
    Storage(rusqlite::Error),
    #[error("invalid persisted contract: {0}")]
    Contract(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(_, Some(message)) = &error {
            let kind = match message.as_str() {
                "factory_capacity_cases" => Some("queued cases"),
                "factory_capacity_effects" => Some("queued effects"),
                "factory_capacity_deliveries" => Some("pending source deliveries"),
                "factory_capacity_evidence" => Some("ordinary evidence storage"),
                "factory_capacity_supervisor_evidence" => Some("supervisor evidence reserve"),
                _ => None,
            };
            if let Some(kind) = kind {
                return Self::Backpressure(format!(
                    "Factory capacity reached for {kind}; retry after capacity is available; existing receipts and cleanup remain valid"
                ));
            }
        }
        Self::Storage(error)
    }
}
