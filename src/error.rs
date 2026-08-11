use std::fmt;

/// Errors surfaced by ephor. `Registry` maps to exit code 2 (invalid registry or
/// configuration, matching the Python `RegistryError` convention); `Command`
/// maps to exit code 1 (a subprocess or IO failure during an operation).
#[derive(Debug)]
pub enum EphorError {
    Registry(String),
    Command(String),
}

impl fmt::Display for EphorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EphorError::Registry(msg) | EphorError::Command(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for EphorError {}

impl From<std::io::Error> for EphorError {
    fn from(err: std::io::Error) -> Self {
        EphorError::Command(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, EphorError>;

pub fn registry_error(msg: impl Into<String>) -> EphorError {
    EphorError::Registry(msg.into())
}
