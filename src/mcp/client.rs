//! Thin wrapper over the rmcp client: one `Session` per CLI invocation (or
//! one long-lived upstream session inside the daemon).

use crate::error::CliError;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientInfo, Implementation, ServerPeerInfo, Tool,
};
use rmcp::service::{RoleClient, RunningService, ServiceError};
use rmcp::transport::TokioChildProcess;
use rmcp::{ClientHandler, ServiceExt};
use serde_json::{Map, Value};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};

pub const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Default)]
pub struct ClientImpl;

impl ClientHandler for ClientImpl {
    fn get_info(&self) -> ClientInfo {
        let mut info = ClientInfo::default();
        info.client_info = Implementation::new("bcp", CLI_VERSION);
        info
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    Spawned,
    Daemon,
}

pub struct Session {
    service: RunningService<RoleClient, ClientImpl>,
    stderr: Option<Arc<Mutex<String>>>,
    timeout: Duration,
    pub kind: SessionKind,
}

impl Session {
    /// Start the upstream server as a child process and complete the MCP
    /// handshake. Its stderr is captured for error messages.
    pub async fn spawn(cmd: &[String], timeout: Duration) -> Result<Self, CliError> {
        let Some((program, args)) = cmd.split_first() else {
            return Err(CliError::usage("server command is empty"));
        };
        let mut command = tokio::process::Command::new(program);
        command.args(args);
        let (process, stderr) = TokioChildProcess::builder(command)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                CliError::transport(format!("failed to start server `{}`: {e}", cmd.join(" ")))
            })?;
        let captured = Arc::new(Mutex::new(String::new()));
        if let Some(mut stderr) = stderr {
            let sink = captured.clone();
            tokio::spawn(async move {
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf).await;
                if let Ok(mut s) = sink.lock() {
                    s.push_str(&buf);
                }
            });
        }
        let service = match tokio::time::timeout(timeout, ClientImpl.serve(process)).await {
            Ok(Ok(service)) => service,
            Ok(Err(e)) => {
                return Err(CliError::transport(with_stderr(
                    format!("MCP handshake with `{}` failed: {e}", cmd.join(" ")),
                    &captured,
                )));
            }
            Err(_) => {
                return Err(CliError::transport(with_stderr(
                    format!(
                        "server `{}` did not complete the MCP handshake within {}s",
                        cmd.join(" "),
                        timeout.as_secs()
                    ),
                    &captured,
                )));
            }
        };
        Ok(Session {
            service,
            stderr: Some(captured),
            timeout,
            kind: SessionKind::Spawned,
        })
    }

    /// Complete the MCP handshake over an already-open byte stream (the
    /// daemon connection, after the preamble).
    pub async fn connect<R, W>(reader: R, writer: W, timeout: Duration) -> Result<Self, CliError>
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        let service = match tokio::time::timeout(timeout, ClientImpl.serve((reader, writer))).await
        {
            Ok(Ok(service)) => service,
            Ok(Err(e)) => {
                return Err(CliError::transport(format!(
                    "MCP handshake with daemon failed: {e}"
                )));
            }
            Err(_) => {
                return Err(CliError::transport(format!(
                    "daemon did not complete the MCP handshake within {}s",
                    timeout.as_secs()
                )));
            }
        };
        Ok(Session {
            service,
            stderr: None,
            timeout,
            kind: SessionKind::Daemon,
        })
    }

    pub fn server_info(&self) -> Option<Arc<ServerPeerInfo>> {
        self.service.peer_info()
    }

    /// `{name, version, protocolVersion}` for reports.
    pub fn server_info_json(&self) -> Value {
        match self.server_info() {
            Some(info) => serde_json::json!({
                "name": info.server_info.as_ref().map(|s| s.name.clone()),
                "version": info.server_info.as_ref().map(|s| s.version.clone()),
                "protocolVersion": info.protocol_version.to_string(),
            }),
            None => Value::Null,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.service.is_transport_closed()
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>, CliError> {
        tokio::time::timeout(self.timeout, self.service.list_all_tools())
            .await
            .map_err(|_| CliError::transport("tools/list timed out"))?
            .map_err(|e| self.map_service_error("tools/list", e))
    }

    /// Forward a raw `tools/list` (used by the daemon proxy).
    pub async fn list_tools_page(
        &self,
        params: Option<rmcp::model::PaginatedRequestParams>,
    ) -> Result<rmcp::model::ListToolsResult, ServiceError> {
        self.service.list_tools(params).await
    }

    /// Forward a raw `tools/call` (used by the daemon proxy).
    pub async fn call_tool_params(
        &self,
        params: CallToolRequestParams,
    ) -> Result<CallToolResult, ServiceError> {
        self.service.call_tool(params).await
    }

    /// Call a tool and return the untouched result.
    pub async fn call_raw(
        &self,
        tool: &str,
        arguments: Map<String, Value>,
    ) -> Result<CallToolResult, CliError> {
        let mut params = CallToolRequestParams::new(tool.to_owned());
        params.arguments = Some(arguments);
        tokio::time::timeout(self.timeout, self.service.call_tool(params))
            .await
            .map_err(|_| {
                CliError::transport(format!(
                    "tool `{tool}` timed out after {}s",
                    self.timeout.as_secs()
                ))
            })?
            .map_err(|e| self.map_service_error(tool, e))
    }

    /// Call a tool and return its structured payload, mapping `isError`
    /// results to `CliError::Tool`.
    pub async fn call(&self, tool: &str, arguments: Map<String, Value>) -> Result<Value, CliError> {
        let result = self.call_raw(tool, arguments).await?;
        result_to_value(&result).map_err(|message| CliError::tool(tool, message))
    }

    pub async fn close(self) {
        let _ = self.service.cancel().await;
    }

    fn map_service_error(&self, tool: &str, err: ServiceError) -> CliError {
        match err {
            ServiceError::McpError(data) if data.code.0 == -32602 => {
                CliError::usage(format!("{tool}: invalid arguments: {}", data.message))
            }
            ServiceError::McpError(data) if data.code.0 == -32601 => {
                CliError::usage(format!("unknown tool `{tool}`: {}", data.message))
            }
            ServiceError::McpError(data) => CliError::transport(format!(
                "{tool}: server error {}: {}",
                data.code.0, data.message
            )),
            ServiceError::TransportClosed => CliError::transport(self.with_own_stderr(format!(
                "{tool}: connection to the server closed unexpectedly"
            ))),
            other => CliError::transport(self.with_own_stderr(format!("{tool}: {other}"))),
        }
    }

    fn with_own_stderr(&self, message: String) -> String {
        match &self.stderr {
            Some(captured) => with_stderr(message, captured),
            None => message,
        }
    }
}

fn with_stderr(message: String, captured: &Arc<Mutex<String>>) -> String {
    let stderr = captured
        .lock()
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
    if stderr.is_empty() {
        message
    } else {
        format!("{message}\nserver stderr:\n{stderr}")
    }
}

/// First text block of a result, if any.
pub fn first_text(result: &CallToolResult) -> Option<String> {
    result
        .content
        .iter()
        .find_map(|block| block.as_text().map(|t| t.text.clone()))
}

/// `structuredContent` if present, else the text block parsed as JSON, else
/// the text itself. `Err` carries the server's error text for `isError`.
pub fn result_to_value(result: &CallToolResult) -> Result<Value, String> {
    if result.is_error == Some(true) {
        let text = first_text(result).unwrap_or_else(|| "tool reported an error".to_owned());
        return Err(text.strip_prefix("Error: ").unwrap_or(&text).to_owned());
    }
    if let Some(structured) = &result.structured_content {
        return Ok(structured.clone());
    }
    match first_text(result) {
        Some(text) => Ok(serde_json::from_str(&text).unwrap_or(Value::String(text))),
        None => Ok(Value::Null),
    }
}

pub fn raw_value(result: &CallToolResult) -> Value {
    serde_json::to_value(result).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::ContentBlock as Content;
    use serde_json::json;

    #[test]
    fn structured_content_wins() {
        let mut r = CallToolResult::success(vec![Content::text("{\"a\":2}")]);
        r.structured_content = Some(json!({"a": 1}));
        assert_eq!(result_to_value(&r).unwrap(), json!({"a": 1}));
    }

    #[test]
    fn text_is_parsed_as_json_or_kept() {
        let r = CallToolResult::success(vec![Content::text("{\"a\":2}")]);
        assert_eq!(result_to_value(&r).unwrap(), json!({"a": 2}));
        let r = CallToolResult::success(vec![Content::text("plain")]);
        assert_eq!(result_to_value(&r).unwrap(), json!("plain"));
    }

    #[test]
    fn is_error_maps_to_message_without_prefix() {
        let r = CallToolResult::error(vec![Content::text("Error: File path must be absolute.")]);
        assert_eq!(
            result_to_value(&r).unwrap_err(),
            "File path must be absolute."
        );
    }

    #[test]
    fn raw_value_has_wire_fields() {
        let r = CallToolResult::error(vec![Content::text("x")]);
        let v = raw_value(&r);
        assert_eq!(v["isError"], true);
        assert_eq!(v["content"][0]["text"], "x");
    }
}
