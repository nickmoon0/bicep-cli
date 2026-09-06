//! `bcp serve`: keeps one upstream server alive and proxies MCP over a
//! loopback TCP port, guarded by a token in the state file.

use crate::error::CliError;
use crate::mcp::client::{CLI_VERSION, Session};
use crate::state::{self, DaemonState};
use crate::transport::{Ctx, Preamble, connect_daemon};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo,
};
use rmcp::service::{RequestContext, RoleServer, ServiceError};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const PREAMBLE_TIMEOUT: Duration = Duration::from_secs(5);

struct Shared {
    state: DaemonState,
    upstream: Session,
    started: Instant,
    calls: AtomicU64,
    active: AtomicUsize,
    last_activity: std::sync::Mutex<Instant>,
}

impl Shared {
    fn status(&self) -> Value {
        json!({
            "ok": true,
            "running": true,
            "pid": self.state.pid,
            "port": self.state.port,
            "uptimeSecs": self.started.elapsed().as_secs(),
            "calls": self.calls.load(Ordering::Relaxed),
            "activeConnections": self.active.load(Ordering::Relaxed),
            "serverCmd": self.state.server_cmd,
            "cliVersion": self.state.cli_version,
            "serverInfo": self.upstream.server_info_json(),
        })
    }

    fn touch(&self) {
        if let Ok(mut t) = self.last_activity.lock() {
            *t = Instant::now();
        }
    }

    fn idle_for(&self) -> Duration {
        self.last_activity
            .lock()
            .map(|t| t.elapsed())
            .unwrap_or_default()
    }
}

/// MCP server that forwards `tools/list` and `tools/call` upstream.
#[derive(Clone)]
struct Proxy {
    shared: Arc<Shared>,
}

fn to_mcp_error(err: ServiceError) -> McpError {
    match err {
        ServiceError::McpError(data) => data,
        other => McpError::internal_error(other.to_string(), None),
    }
}

impl ServerHandler for Proxy {
    fn get_info(&self) -> ServerInfo {
        // Present the upstream server's identity so `bcp version --server`
        // and `bcp doctor` report Azure.Bicep.McpServer, not the proxy.
        let upstream = self.shared.upstream.server_info();
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = upstream
            .as_ref()
            .and_then(|i| i.server_info.clone())
            .unwrap_or_else(|| Implementation::new("bcp-daemon", CLI_VERSION));
        info.server_info.title = Some(format!("via bcp daemon {CLI_VERSION}"));
        info.instructions = upstream.and_then(|i| i.instructions.clone());
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.shared
            .upstream
            .list_tools_page(request)
            .await
            .map_err(to_mcp_error)
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        self.shared.calls.fetch_add(1, Ordering::Relaxed);
        self.shared.touch();
        let result = self
            .shared
            .upstream
            .call_tool_params(request)
            .await
            .map_err(to_mcp_error)?;
        Ok(CallToolResponse::from(result))
    }
}

async fn write_line(writer: &mut tokio::net::tcp::OwnedWriteHalf, value: &Value) {
    let mut line = value.to_string();
    line.push('\n');
    let _ = writer.write_all(line.as_bytes()).await;
    let _ = writer.flush().await;
}

async fn handle_conn(stream: TcpStream, shared: Arc<Shared>, shutdown: watch::Sender<bool>) {
    let _ = stream.set_nodelay(true);
    let (read_half, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut line = String::new();
    match tokio::time::timeout(PREAMBLE_TIMEOUT, reader.read_line(&mut line)).await {
        Ok(Ok(n)) if n > 0 => {}
        _ => return,
    }
    let Ok(preamble) = serde_json::from_str::<Preamble>(line.trim()) else {
        write_line(
            &mut writer,
            &json!({"ok": false, "error": "invalid preamble"}),
        )
        .await;
        return;
    };
    if preamble.token != shared.state.token {
        write_line(&mut writer, &json!({"ok": false, "error": "unauthorized"})).await;
        return;
    }
    shared.touch();
    match preamble.op.as_str() {
        "status" => write_line(&mut writer, &shared.status()).await,
        "shutdown" => {
            write_line(&mut writer, &json!({"ok": true})).await;
            let _ = shutdown.send(true);
        }
        "mcp" => {
            write_line(&mut writer, &json!({"ok": true})).await;
            shared.active.fetch_add(1, Ordering::Relaxed);
            let proxy = Proxy {
                shared: shared.clone(),
            };
            match proxy.serve((reader, writer)).await {
                Ok(running) => {
                    let _ = running.waiting().await;
                }
                Err(e) => eprintln!("bcp serve: client handshake failed: {e}"),
            }
            shared.active.fetch_sub(1, Ordering::Relaxed);
            shared.touch();
        }
        other => {
            write_line(
                &mut writer,
                &json!({"ok": false, "error": format!("unknown op `{other}`")}),
            )
            .await
        }
    }
}

async fn terminate_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    }
    #[cfg(not(unix))]
    {
        std::future::pending::<()>().await
    }
}

/// Run the daemon in the foreground until stopped.
pub async fn serve(ctx: &Ctx, idle_secs: u64) -> Result<Value, CliError> {
    if let Some(existing) = state::read(&ctx.state_dir) {
        if connect_daemon(&existing, "status").await.is_ok() {
            return Err(CliError::usage(format!(
                "a daemon is already running (pid {}, port {}); stop it with `bcp daemon stop`",
                existing.pid, existing.port
            )));
        }
        state::remove(&ctx.state_dir);
    }
    let upstream = Session::spawn(&ctx.server_cmd, ctx.timeout).await?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| CliError::transport(format!("cannot bind loopback port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| CliError::transport(format!("cannot read bound port: {e}")))?
        .port();
    let daemon_state = DaemonState {
        port,
        token: state::new_token(),
        pid: std::process::id(),
        server_cmd: ctx.server_cmd.clone(),
        cli_version: CLI_VERSION.to_owned(),
    };
    state::write(&ctx.state_dir, &daemon_state)
        .map_err(|e| CliError::io("cannot write daemon state file", e))?;
    let state_file = state::state_file(&ctx.state_dir);
    println!(
        "{}",
        json!({
            "port": port,
            "pid": daemon_state.pid,
            "stateFile": state_file.to_string_lossy(),
            "serverCmd": daemon_state.server_cmd,
            "idleSecs": idle_secs,
        })
    );
    eprintln!(
        "bcp serve: listening on 127.0.0.1:{port} (state file {}); press Ctrl-C to stop",
        state_file.display()
    );

    let shared = Arc::new(Shared {
        state: daemon_state,
        upstream,
        started: Instant::now(),
        calls: AtomicU64::new(0),
        active: AtomicUsize::new(0),
        last_activity: std::sync::Mutex::new(Instant::now()),
    });
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    let reason;
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _)) => {
                        tokio::spawn(handle_conn(stream, shared.clone(), shutdown_tx.clone()));
                    }
                    Err(e) => eprintln!("bcp serve: accept failed: {e}"),
                }
            }
            _ = tokio::signal::ctrl_c() => { reason = "interrupted"; break; }
            _ = terminate_signal() => { reason = "terminated"; break; }
            _ = shutdown_rx.changed() => { reason = "stop requested"; break; }
            _ = ticker.tick() => {
                if shared.upstream.is_closed() {
                    reason = "upstream server exited";
                    break;
                }
                if idle_secs > 0
                    && shared.active.load(Ordering::Relaxed) == 0
                    && shared.idle_for() >= Duration::from_secs(idle_secs)
                {
                    reason = "idle timeout";
                    break;
                }
            }
        }
    }
    state::remove(&ctx.state_dir);
    eprintln!("bcp serve: shutting down ({reason})");
    let calls = shared.calls.load(Ordering::Relaxed);
    let summary = json!({"stopped": true, "reason": reason, "calls": calls});
    drop(shared);
    Ok(summary)
}

/// `bcp daemon status`
pub async fn status(ctx: &Ctx) -> Result<Value, CliError> {
    let state_file = state::state_file(&ctx.state_dir);
    let Some(daemon) = state::read(&ctx.state_dir) else {
        return Ok(json!({
            "running": false,
            "stateFile": state_file.to_string_lossy(),
        }));
    };
    match connect_daemon(&daemon, "status").await {
        Ok(conn) => {
            let mut v = conn.ack;
            v["stateFile"] = Value::String(state_file.to_string_lossy().into_owned());
            Ok(v)
        }
        Err(e) => Ok(json!({
            "running": false,
            "stale": true,
            "stateFile": state_file.to_string_lossy(),
            "pid": daemon.pid,
            "port": daemon.port,
            "error": e.message(),
        })),
    }
}

/// `bcp daemon stop`
pub async fn stop(ctx: &Ctx) -> Result<Value, CliError> {
    let Some(daemon) = state::read(&ctx.state_dir) else {
        return Ok(json!({"stopped": false, "running": false}));
    };
    match connect_daemon(&daemon, "shutdown").await {
        Ok(_) => {
            // Give the daemon a moment to remove its state file.
            for _ in 0..30 {
                if state::read(&ctx.state_dir).is_none() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Ok(json!({"stopped": true, "pid": daemon.pid, "port": daemon.port}))
        }
        Err(e) => {
            state::remove(&ctx.state_dir);
            Ok(json!({
                "stopped": false,
                "running": false,
                "stale": true,
                "removedStateFile": true,
                "error": e.message(),
            }))
        }
    }
}
