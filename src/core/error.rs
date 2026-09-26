use std::fmt;

/// An error in a Reqfile, a decision.yaml or a config.yaml, located in its file.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl ConfigError {
    pub fn at(file: &str, line: usize, message: impl Into<String>) -> Self {
        Self {
            file: file.to_string(),
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.file, self.line, self.message)
    }
}
