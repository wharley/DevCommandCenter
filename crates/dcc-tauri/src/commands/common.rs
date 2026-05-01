use thiserror::Error;

pub type CmdResult<T> = Result<T, CmdError>;

#[derive(Debug, Error)]
pub enum CmdError {
	#[error("command failed: {0}")]
	Failed(String),
}

pub async fn run_blocking<T, F>(work: F) -> CmdResult<T>
where
	T: Send + 'static,
	F: FnOnce() -> T + Send + 'static,
{
	Ok(work())
}
