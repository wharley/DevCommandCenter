use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("repository error: {0}")]
    Repository(String),
    #[error("git error: {0}")]
    Git(String),
    #[error("event bus error: {0}")]
    EventBus(String),
    #[error("provider error: {0}")]
    Provider(String),
}
