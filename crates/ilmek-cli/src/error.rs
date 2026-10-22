use std::fmt;

/// A failure reported to the operator. Messages never include secrets.
#[derive(Debug)]
pub(crate) struct CliError(String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl CliError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

/// Adds a short description of what was being attempted to any displayable error.
pub(crate) trait Context<T> {
    fn context(self, what: &str) -> Result<T, CliError>;
}

impl<T, E: fmt::Display> Context<T> for Result<T, E> {
    fn context(self, what: &str) -> Result<T, CliError> {
        self.map_err(|error| CliError(format!("{what}: {error}")))
    }
}

pub(crate) type CliResult<T = ()> = Result<T, CliError>;
