//! Output rendering: JSON (default) or text, plus the "soft failure" outcome
//! used when a command has a payload to print *and* a non-zero exit code.

use crate::error::CliError;
use serde_json::Value;
use std::io::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    Json,
    Text,
}

#[derive(Clone, Copy, Debug)]
pub struct OutputOpts {
    pub format: Format,
    pub compact: bool,
}

/// What a command produces on stdout.
#[derive(Debug, Clone)]
pub enum Output {
    Json(Value),
    Text(String),
    /// Nothing on stdout (e.g. `--write` variants report on stderr only).
    None,
}

/// The full result of a command: something to print, and optionally an error
/// that still has to be reported (with its exit code) after printing.
#[derive(Debug)]
pub struct Outcome {
    pub output: Output,
    pub error: Option<CliError>,
}

impl Outcome {
    pub fn json(value: Value) -> Self {
        Outcome {
            output: Output::Json(value),
            error: None,
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Outcome {
            output: Output::Text(text.into()),
            error: None,
        }
    }

    pub fn silent() -> Self {
        Outcome {
            output: Output::None,
            error: None,
        }
    }

    pub fn with_error(mut self, error: CliError) -> Self {
        self.error = Some(error);
        self
    }

    /// Pick JSON or a text rendering depending on the requested format.
    pub fn render(opts: OutputOpts, value: Value, text: impl FnOnce(&Value) -> String) -> Self {
        match opts.format {
            Format::Json => Outcome::json(value),
            Format::Text => Outcome::text(text(&value)),
        }
    }
}

impl Output {
    pub fn write_to(&self, out: &mut impl Write, opts: OutputOpts) -> std::io::Result<()> {
        match self {
            Output::Json(value) => {
                if opts.compact {
                    serde_json::to_writer(&mut *out, value)?;
                } else {
                    serde_json::to_writer_pretty(&mut *out, value)?;
                }
                out.write_all(b"\n")
            }
            Output::Text(text) => {
                out.write_all(text.as_bytes())?;
                if !text.ends_with('\n') {
                    out.write_all(b"\n")?;
                }
                Ok(())
            }
            Output::None => Ok(()),
        }
    }
}

/// Text rendering for plain string lists: one item per line.
pub fn lines_of(value: &Value, key: &str) -> String {
    value[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Fallback text rendering: pretty JSON.
pub fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn render(output: Output, compact: bool) -> String {
        let mut buf = Vec::new();
        output
            .write_to(
                &mut buf,
                OutputOpts {
                    format: Format::Json,
                    compact,
                },
            )
            .unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn json_compact_and_pretty() {
        let v = json!({"a": [1, 2]});
        assert_eq!(render(Output::Json(v.clone()), true), "{\"a\":[1,2]}\n");
        assert!(render(Output::Json(v), false).contains("\n  \"a\""));
    }

    #[test]
    fn text_gets_trailing_newline_once() {
        assert_eq!(render(Output::Text("hi".into()), false), "hi\n");
        assert_eq!(render(Output::Text("hi\n".into()), false), "hi\n");
    }

    #[test]
    fn lines_of_joins_strings() {
        assert_eq!(lines_of(&json!({"x": ["a", "b"]}), "x"), "a\nb");
        assert_eq!(lines_of(&json!({}), "x"), "");
    }
}
