//! Choosing how to reach the server (daemon vs one-shot child) and the
//! per-invocation context shared by all commands.

use crate::error::CliError;
use crate::mcp::client::{Session, raw_value, result_to_value};
use crate::output::{Format, OutputOpts};
use crate::state::{self, DaemonState};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum TransportMode {
    /// Use the daemon if `bcp serve` is running, otherwise spawn a server.
    Auto,
    /// Require the daemon; fail (exit 3) if it is not running.
    Daemon,
    /// Always spawn a one-shot server for this invocation.
    Spawn,
}

/// First line a client sends on a daemon connection.
#[derive(Debug, Serialize, Deserialize)]
pub struct Preamble {
    pub token: String,
    pub op: String,
}

pub struct DaemonConn {
    pub reader: BufReader<OwnedReadHalf>,
    pub writer: OwnedWriteHalf,
    pub ack: Value,
}

/// Connect to the daemon, send the preamble, and read the acknowledgement.
pub async fn connect_daemon(daemon: &DaemonState, op: &str) -> Result<DaemonConn, CliError> {
    let stream = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect(("127.0.0.1", daemon.port)),
    )
    .await
    .map_err(|_| CliError::transport("timed out connecting to daemon"))?
    .map_err(|e| {
        CliError::transport(format!(
            "cannot connect to daemon on port {}: {e}",
            daemon.port
        ))
    })?;
    let _ = stream.set_nodelay(true);
    let (read_half, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let preamble = serde_json::to_string(&Preamble {
        token: daemon.token.clone(),
        op: op.to_owned(),
    })
    .expect("preamble serializes");
    writer
        .write_all(format!("{preamble}\n").as_bytes())
        .await
        .map_err(|e| CliError::transport(format!("daemon write failed: {e}")))?;
    let mut line = String::new();
    tokio::time::timeout(CONNECT_TIMEOUT, reader.read_line(&mut line))
        .await
        .map_err(|_| CliError::transport("daemon did not acknowledge the connection"))?
        .map_err(|e| CliError::transport(format!("daemon read failed: {e}")))?;
    let ack: Value = serde_json::from_str(line.trim()).map_err(|_| {
        CliError::transport(format!("daemon sent an invalid acknowledgement: {line:?}"))
    })?;
    if ack.get("ok").and_then(Value::as_bool) != Some(true) {
        let reason = ack["error"].as_str().unwrap_or("rejected");
        return Err(CliError::transport(format!(
            "daemon rejected the connection: {reason}"
        )));
    }
    Ok(DaemonConn {
        reader,
        writer,
        ack,
    })
}

/// Result of `Ctx::invoke`: either the untouched wire result (`--raw`) or
/// the structured payload.
pub enum Invoked {
    /// Raw wire result plus the error to report if it carried `isError`.
    Raw(Value, Option<CliError>),
    Value(Value),
}

/// Everything a command needs: options plus a lazily opened session.
pub struct Ctx {
    pub mode: TransportMode,
    pub state_dir: PathBuf,
    pub server_cmd: Vec<String>,
    pub timeout: Duration,
    pub format: Option<Format>,
    pub compact: bool,
    pub raw: bool,
    pub quiet: bool,
    session: tokio::sync::OnceCell<Session>,
}

/// Options used to build a [`Ctx`].
pub struct CtxOptions {
    pub mode: TransportMode,
    pub state_dir: PathBuf,
    pub server_cmd: Vec<String>,
    pub timeout: Duration,
    pub format: Option<Format>,
    pub compact: bool,
    pub raw: bool,
    pub quiet: bool,
}

impl Ctx {
    pub fn new(opts: CtxOptions) -> Self {
        Ctx {
            mode: opts.mode,
            state_dir: opts.state_dir,
            server_cmd: opts.server_cmd,
            timeout: opts.timeout,
            format: opts.format,
            compact: opts.compact,
            raw: opts.raw,
            quiet: opts.quiet,
            session: tokio::sync::OnceCell::new(),
        }
    }

    /// Output options with a per-command default format.
    pub fn out(&self, default: Format) -> OutputOpts {
        OutputOpts {
            format: self.format.unwrap_or(default),
            compact: self.compact,
        }
    }

    pub fn warn(&self, message: &str) {
        if !self.quiet {
            eprintln!("bcp: {message}");
        }
    }

    /// Open (once) the session according to the transport mode.
    pub async fn session(&self) -> Result<&Session, CliError> {
        self.session.get_or_try_init(|| self.open_session()).await
    }

    async fn open_session(&self) -> Result<Session, CliError> {
        match self.mode {
            TransportMode::Spawn => Session::spawn(&self.server_cmd, self.timeout).await,
            TransportMode::Daemon => match state::read(&self.state_dir) {
                Some(daemon) => self.connect_session(&daemon).await,
                None => Err(CliError::transport(format!(
                    "daemon is not running (no state file at {}); start it with `bcp serve`",
                    state::state_file(&self.state_dir).display()
                ))),
            },
            TransportMode::Auto => {
                if let Some(daemon) = state::read(&self.state_dir) {
                    match self.connect_session(&daemon).await {
                        Ok(session) => return Ok(session),
                        Err(e) => {
                            let msg = e.message().to_owned();
                            if msg.contains("cannot connect") {
                                state::remove(&self.state_dir);
                                self.warn("stale daemon state file removed; spawning a server for this call");
                            } else {
                                self.warn(&format!(
                                    "daemon unusable ({msg}); spawning a server for this call"
                                ));
                            }
                        }
                    }
                }
                Session::spawn(&self.server_cmd, self.timeout).await
            }
        }
    }

    async fn connect_session(&self, daemon: &DaemonState) -> Result<Session, CliError> {
        let conn = connect_daemon(daemon, "mcp").await?;
        Session::connect(conn.reader, conn.writer, self.timeout).await
    }

    /// Call a tool, honouring `--raw`.
    pub async fn invoke(&self, tool: &str, args: Map<String, Value>) -> Result<Invoked, CliError> {
        let session = self.session().await?;
        let result = session.call_raw(tool, args).await?;
        if self.raw {
            let error = result_to_value(&result)
                .err()
                .map(|message| CliError::tool(tool, message));
            return Ok(Invoked::Raw(raw_value(&result), error));
        }
        result_to_value(&result)
            .map(Invoked::Value)
            .map_err(|message| CliError::tool(tool, message))
    }

    pub async fn finish(self) {
        if let Some(session) = self.session.into_inner() {
            session.close().await;
        }
    }
}

/// Small helper to build tool argument objects.
#[macro_export]
macro_rules! args {
    ($($k:expr => $v:expr),* $(,)?) => {{
        let mut m = ::serde_json::Map::new();
        $( m.insert(($k).to_string(), ::serde_json::Value::from($v)); )*
        m
    }};
}
