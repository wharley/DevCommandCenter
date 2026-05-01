pub mod application;
pub mod domain;
pub mod error;
pub mod ports;

pub use error::CoreError;

pub type Result<T> = std::result::Result<T, CoreError>;
