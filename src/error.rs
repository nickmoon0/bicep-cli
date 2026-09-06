//! Error type that owns the exit-code and stderr-envelope contract.

use serde_json::{Value, json};
use std::fmt;

/// Every failure the CLI can report. The variant decides the exit code.
#[derive(Debug)]
pub enum CliError {
    /// Bad invocation: unknown flag, missing file, malformed argument. Exit 2.
    Usage(String),
    /// The server ran the tool and reported an error, or the tool's result
    /// signals failure (e.g. compilation errors). Exit 1.
    Tool {
        tool: String,
        message: String,
        details: Option<Value>,
    },
    /// Could not reach or talk to the server. Exit 3.
    Transport(String),
}

impl CliError {
    pub fn usage(message: impl Into<String>) -> Self {
        CliError::Usage(message.into())
    }

    pub fn transport(message: impl Into<String>) -> Self {
        CliError::Transport(message.into())
    }

    pub fn tool(tool: impl Into<String>, message: impl Into<String>) -> Self {
        CliError::Tool {
            tool: tool.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn io(context: &str, err: std::io::Error) -> Self {
        CliError::Usage(format!("{context}: {err}"))
    }

    pub fn kind(&self) -> &'static str {
        match self {
            CliError::Usage(_) => "usage",
            CliError::Tool { .. } => "tool",
            CliError::Transport(_) => "transport",
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Usage(_) => 2,
            CliError::Tool { .. } => 1,
            CliError::Transport(_) => 3,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            CliError::Usage(m) | CliError::Transport(m) => m,
            CliError::Tool { message, .. } => message,
        }
    }

    /// The single JSON object written to stderr on failure.
    pub fn to_json(&self) -> Value {
        let mut error = json!({
            "kind": self.kind(),
            "message": self.message(),
            "exitCode": self.exit_code(),
        });
        if let CliError::Tool { tool, details, .. } = self {
            error["tool"] = Value::String(tool.clone());
            if let Some(details) = details {
                error["details"] = details.clone();
            }
        }
        json!({ "error": error })
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for CliError {}

impl From<anyhow::Error> for CliError {
    fn from(err: anyhow::Error) -> Self {
        match err.downcast::<CliError>() {
            Ok(cli) => cli,
            Err(other) => CliError::Transport(format!("{other:#}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_follow_contract() {
        assert_eq!(CliError::usage("x").exit_code(), 2);
        assert_eq!(CliError::tool("build_bicep", "x").exit_code(), 1);
        assert_eq!(CliError::transport("x").exit_code(), 3);
    }

    #[test]
    fn envelope_shape() {
        let err = CliError::Tool {
            tool: "build_bicep".into(),
            message: "boom".into(),
            details: Some(json!({"n": 1})),
        };
        let v = err.to_json();
        assert_eq!(v["error"]["kind"], "tool");
        assert_eq!(v["error"]["tool"], "build_bicep");
        assert_eq!(v["error"]["exitCode"], 1);
        assert_eq!(v["error"]["details"]["n"], 1);
        assert_eq!(v["error"]["message"], "boom");
    }

    #[test]
    fn anyhow_roundtrip_preserves_variant() {
        let e: anyhow::Error = CliError::usage("bad").into();
        let back: CliError = e.into();
        assert_eq!(back.exit_code(), 2);
    }
}
