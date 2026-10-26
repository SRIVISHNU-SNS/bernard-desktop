use std::fmt::Display;

pub type CommandResult<T> = Result<T, String>;

pub fn command_error(context: &str, error: impl Display) -> String {
    format!("{context}: {error}")
}

pub fn internal_error(error: impl Display) -> String {
    error.to_string()
}
