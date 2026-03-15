//! Application error type.

use std::io;
use thiserror::Error;

/// Application-level errors.
#[derive(Debug, Error)]
pub enum AppError {
    /// User cancelled (e.g. Esc on form).
    #[error("Cancelled")]
    UserCancelled,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        AppError::Other(e.into())
    }
}
