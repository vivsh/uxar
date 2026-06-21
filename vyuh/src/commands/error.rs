use crate::callables;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    /// Emitted when `--help` or a parse error causes early exit; output is in the message.
    #[error("Argument parsing exit: {0}")]
    Exit(String),

    #[error("Command not found: {0}")]
    NotFound(String),

    #[error("Command not found: {0}. Use 'help' to list available commands.")]
    UnknownCommand(String),

    #[error("Command already exists: {0}")]
    AlreadyExists(String),

    #[error("Unsupported type for command argument: {0}")]
    UnsupportedType(String),

    #[error("Unknown flag for command '{command}': --{flag}")]
    UnknownFlag { command: String, flag: String },

    #[error("Failed to parse --{flag} value '{value}' as {expected_type}: {error}")]
    ParseError {
        flag: String,
        value: String,
        expected_type: String,
        error: String,
    },

    #[error(transparent)]
    CallError(#[from] callables::CallError),

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}
