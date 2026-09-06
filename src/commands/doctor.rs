use crate::cli::VersionArgs;
use crate::daemon;
use crate::error::CliError;
use crate::mcp::client::{CLI_VERSION, SessionKind};
use crate::output::{Format, Outcome, pretty};
use crate::state;
use crate::transport::Ctx;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

async fn probe(program: &str, args: &[&str]) -> Value {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args).stdin(std::process::Stdio::null());
    match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
        Ok(Ok(out)) if out.status.success() => {
            json!({"found": true, "version": String::from_utf8_lossy(&out.stdout).trim()})
        }
        Ok(Ok(out)) => json!({
            "found": true,
            "error": format!("exit {}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim()),
        }),
        Ok(Err(e)) => json!({"found": false, "error": e.to_string()}),
        Err(_) => json!({"found": true, "error": "timed out"}),
    }
}

pub async fn doctor(ctx: &Ctx) -> Result<Outcome, CliError> {
    let dotnet = probe("dotnet", &["--version"]).await;
    let daemon_status = daemon::status(ctx).await.unwrap_or(Value::Null);
    let started = Instant::now();
    let session = ctx.session().await;
    let startup_ms = started.elapsed().as_millis() as u64;
    let mut report = json!({
        "cli": CLI_VERSION,
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "transportMode": format!("{:?}", ctx.mode).to_lowercase(),
        "serverCmd": ctx.server_cmd,
        "stateFile": state::state_file(&ctx.state_dir).to_string_lossy(),
        "dotnet": dotnet,
        "daemon": daemon_status,
    });
    match session {
        Ok(session) => {
            let tools = session.list_tools().await?;
            let first_call = Instant::now();
            let _ = session
                .call("get_bicep_best_practices", serde_json::Map::new())
                .await;
            report["ok"] = Value::Bool(true);
            report["connectedVia"] = Value::String(
                match session.kind {
                    SessionKind::Daemon => "daemon",
                    SessionKind::Spawned => "spawn",
                }
                .into(),
            );
            report["startupMs"] = Value::from(startup_ms);
            report["roundTripMs"] = Value::from(first_call.elapsed().as_millis() as u64);
            report["toolCount"] = Value::from(tools.len());
            report["serverInfo"] = session.server_info_json();
            Ok(Outcome::render(ctx.out(Format::Json), report, pretty))
        }
        Err(e) => {
            report["ok"] = Value::Bool(false);
            report["error"] = Value::String(e.message().to_owned());
            Ok(Outcome::render(ctx.out(Format::Json), report, pretty).with_error(e))
        }
    }
}

pub async fn version(ctx: &Ctx, args: VersionArgs) -> Result<Outcome, CliError> {
    let mut v = json!({"cli": CLI_VERSION});
    if args.server {
        let session = ctx.session().await?;
        v["server"] = session.server_info_json();
    }
    Ok(Outcome::render(ctx.out(Format::Json), v, |v| {
        let mut s = format!("bcp {}", v["cli"].as_str().unwrap_or(""));
        if let Some(name) = v["server"]["name"].as_str() {
            s.push_str(&format!(
                "\n{name} {}",
                v["server"]["version"].as_str().unwrap_or("")
            ));
        }
        s
    }))
}
