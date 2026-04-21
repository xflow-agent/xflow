use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFriendlyError {
    pub message: String,
    pub severity: ErrorSeverity,
    pub suggestion: Option<String>,
}

impl UserFriendlyError {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
            severity: ErrorSeverity::Error,
            suggestion: None,
        }
    }

    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_suggestion(mut self, suggestion: &str) -> Self {
        self.suggestion = Some(suggestion.to_string());
        self
    }

    pub fn to_display_string(&self) -> String {
        match &self.suggestion {
            Some(s) => format!("Error: {} ({})", self.message, s),
            None => format!("Error: {}", self.message),
        }
    }
}

impl From<std::io::Error> for UserFriendlyError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => {
                Self::new("File not found").with_suggestion("check the file path")
            }
            std::io::ErrorKind::PermissionDenied => {
                Self::new("Permission denied").with_suggestion("check file permissions")
            }
            std::io::ErrorKind::InvalidInput => {
                Self::new("Invalid input").with_suggestion("check the input parameters")
            }
            std::io::ErrorKind::TimedOut => {
                Self::new("Operation timed out").with_suggestion("try again later")
            }
            _ => Self::new("IO error").with_suggestion("check if the file is accessible"),
        }
    }
}

impl std::fmt::Display for UserFriendlyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

impl std::error::Error for UserFriendlyError {}

pub fn format_io_error(err: &std::io::Error) -> String {
    let user_err = UserFriendlyError::from(std::io::Error::new(err.kind(), ""));
    user_err.to_display_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_error_to_user_friendly() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let user_err: UserFriendlyError = io_err.into();
        assert_eq!(user_err.message, "File not found");
        assert!(user_err.suggestion.is_some());
    }

    #[test]
    fn test_display_string() {
        let err = UserFriendlyError::new("File not found").with_suggestion("check path");
        assert_eq!(err.to_display_string(), "Error: File not found (check path)");
    }

    #[test]
    fn test_display_string_no_suggestion() {
        let err = UserFriendlyError::new("IO error");
        assert_eq!(err.to_display_string(), "Error: IO error");
    }
}
