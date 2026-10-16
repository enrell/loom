//! The crate's error type. Every variant renders as a sentence a user (or an
//! agent) can act on.

use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{context}: {source}")]
    Io { context: String, source: std::io::Error },
    #[error(transparent)]
    Plugin(#[from] loom_plugin::Error),
    #[error("{path}: {message}")]
    BadFile { path: PathBuf, message: String },
    #[error("game {0:?} is not set up; run `loom game add {0}`")]
    UnknownGame(String),
    #[error("no installed plugin provides game {0:?}; run `loom plugin add <import-path>`")]
    NoPlugin(String),
    #[error("no mod named {0:?}")]
    UnknownMod(String),
    #[error("no profile named {0:?}")]
    UnknownProfile(String),
    #[error("{0}")]
    Invalid(String),
    #[error("network: {0}")]
    Network(String),
    #[error("virtual file system: {0}")]
    Vfs(String),
}

impl Error {
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }
}

/// Attaches what we were doing to an I/O error.
pub(crate) trait Context<T> {
    fn ctx(self, what: impl FnOnce() -> String) -> Result<T>;
}

impl<T> Context<T> for std::io::Result<T> {
    fn ctx(self, what: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|source| Error::Io { context: what(), source })
    }
}
