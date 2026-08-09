use serde::Serialize;
use std::fmt::{Display, Formatter};

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl CommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl Display for CommandError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<rusqlite::Error> for CommandError {
    fn from(value: rusqlite::Error) -> Self {
        Self::new("database_error", value.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(value: std::io::Error) -> Self {
        Self::new("filesystem_error", value.to_string())
    }
}

impl From<image::ImageError> for CommandError {
    fn from(value: image::ImageError) -> Self {
        Self::new("invalid_image", value.to_string())
    }
}

pub type CommandResult<T> = Result<T, CommandError>;
