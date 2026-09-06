use super::invoke_or_raw;
use crate::cli::{CallArgs, ToolsArgs};
use crate::error::CliError;
use crate::mcp::client::{raw_value, result_to_value};
use crate::output::{Format, Outcome};
use crate::tools::{COMMAND_MAP, absolute, parse_args_json, parse_kv, path_string};
use crate::transport::Ctx;
use rmcp::model::Tool;
use serde_json::{Map, Value, json};
use std::io::{BufRead, Write};

fn tool_json(t: &Tool) -> Value {
    json!({
        "name": t.name,
        "title": t.title,
        "description": t.description,
        "inputSchema": t.input_schema,
        "outputSchema": t.output_schema,
    })
}

fn subcommand_for(tool: &str) -> Option<&'static str> {
    COMMAND_MAP
        .iter()
        .find(|(_, t)| *t == tool)
        .map(|(c, _)| *c)
}

pub fn tools_markdown(tools: &[Tool]) -> String {
    let mut out = String::new();
    for t in tools {
        let title = t.title.as_deref().unwrap_or("");
        out.push_str(&format!("## `{}` — {title}\n\n", t.name));
        if let Some(cmd) = subcommand_for(&t.name) {
            out.push_str(&format!("CLI: `bcp {cmd}` (or `bcp call {}`)\n\n", t.name));
        } else {
            out.push_str(&format!("CLI: `bcp call {}`\n\n", t.name));
        }
        if let Some(d) = &t.description {
            out.push_str(d.trim());
            out.push_str("\n\n");
        }
        let props = t.input_schema.get("properties").and_then(Value::as_object);
        let required: Vec<&str> = t
            .input_schema
            .get("required")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        match props {
            Some(props) if !props.is_empty() => {
                out.push_str("| Parameter | Type | Required | Description |\n|---|---|---|---|\n");
                for (name, schema) in props {
                    let ty = match &schema["type"] {
                        Value::String(s) => s.clone(),
                        Value::Array(a) => a
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(" \\| "),
                        _ => "any".into(),
                    };
                    let desc = schema["description"]
                        .as_str()
                        .unwrap_or("")
                        .replace('\n', " ");
                    let req = if required.contains(&name.as_str()) {
                        "yes"
                    } else {
                        "no"
                    };
                    out.push_str(&format!("| `{name}` | {ty} | {req} | {desc} |\n"));
                }
                out.push('\n');
            }
            _ => out.push_str("No parameters.\n\n"),
        }
    }
    out
}

pub async fn tools(ctx: &Ctx, args: ToolsArgs) -> Result<Outcome, CliError> {
    if args.map {
        let map: Vec<Value> = COMMAND_MAP
            .iter()
            .map(|(c, t)| json!({"command": c, "tool": t}))
            .collect();
        return Ok(Outcome::render(
            ctx.out(Format::Json),
            Value::Array(map),
            |v| {
                v.as_array()
                    .map(|a| {
                        a.iter()
                            .map(|e| {
                                format!(
                                    "{}\t{}",
                                    e["command"].as_str().unwrap_or(""),
                                    e["tool"].as_str().unwrap_or("")
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default()
            },
        ));
    }
    let mut tools = ctx.session().await?.list_tools().await?;
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    if args.names {
        let names: Vec<Value> = tools
            .iter()
            .map(|t| Value::String(t.name.to_string()))
            .collect();
        return Ok(Outcome::render(
            ctx.out(Format::Json),
            Value::Array(names),
            |v| {
                v.as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default()
            },
        ));
    }
    if args.markdown {
        return Ok(Outcome::text(tools_markdown(&tools)));
    }
    let list: Vec<Value> = tools.iter().map(tool_json).collect();
    Ok(Outcome::render(
        ctx.out(Format::Json),
        Value::Array(list),
        |v| {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .map(|t| {
                            format!(
                                "{}: {}",
                                t["name"].as_str().unwrap_or(""),
                                t["title"].as_str().unwrap_or("")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default()
        },
    ))
}

/// Make a `filePath` argument absolute so relative paths work like elsewhere.
fn absolutize_file_path(args: &mut Map<String, Value>) -> Result<(), CliError> {
    if let Some(Value::String(p)) = args.get("filePath") {
        let abs = absolute(std::path::Path::new(p))?;
        args.insert("filePath".into(), Value::String(path_string(&abs)));
    }
    Ok(())
}

pub async fn call(ctx: &Ctx, args: CallArgs) -> Result<Outcome, CliError> {
    let mut map = match &args.args {
        Some(s) => parse_args_json(s)?,
        None => Map::new(),
    };
    for kv in &args.kv {
        let (k, v) = parse_kv(kv)?;
        map.insert(k, v);
    }
    absolutize_file_path(&mut map)?;
    let value = invoke_or_raw!(ctx, &args.tool, map);
    Ok(Outcome::render(
        ctx.out(Format::Json),
        value,
        crate::output::pretty,
    ))
}

/// Streams one result line per input line; exit 1 if any call failed.
pub async fn batch(ctx: &Ctx) -> Result<Outcome, CliError> {
    let session = ctx.session().await?;
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut failures = 0usize;
    let mut total = 0usize;
    for (index, line) in stdin.lock().lines().enumerate() {
        let line = line.map_err(|e| CliError::io("cannot read stdin", e))?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        total += 1;
        let request: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                failures += 1;
                writeln!(
                    out,
                    "{}",
                    json!({"index": index, "ok": false, "error": format!("invalid JSON: {e}")})
                )
                .map_err(|e| CliError::io("cannot write stdout", e))?;
                continue;
            }
        };
        let tool = request["tool"].as_str().unwrap_or("").to_owned();
        let mut args = request["args"].as_object().cloned().unwrap_or_default();
        if tool.is_empty() {
            failures += 1;
            writeln!(
                out,
                "{}",
                json!({"index": index, "ok": false, "error": "missing `tool`"})
            )
            .map_err(|e| CliError::io("cannot write stdout", e))?;
            continue;
        }
        let response = match absolutize_file_path(&mut args) {
            Err(e) => json!({"index": index, "tool": tool, "ok": false, "error": e.message()}),
            Ok(()) => match session.call_raw(&tool, args).await {
                Err(e) => {
                    json!({"index": index, "tool": tool, "ok": false, "error": e.message(), "kind": e.kind()})
                }
                Ok(result) if ctx.raw => {
                    json!({"index": index, "tool": tool, "ok": result.is_error != Some(true), "result": raw_value(&result)})
                }
                Ok(result) => match result_to_value(&result) {
                    Ok(v) => json!({"index": index, "tool": tool, "ok": true, "result": v}),
                    Err(message) => {
                        json!({"index": index, "tool": tool, "ok": false, "error": message, "kind": "tool"})
                    }
                },
            },
        };
        if response["ok"] != Value::Bool(true) {
            failures += 1;
        }
        writeln!(out, "{response}").map_err(|e| CliError::io("cannot write stdout", e))?;
        out.flush()
            .map_err(|e| CliError::io("cannot write stdout", e))?;
    }
    let outcome = Outcome::silent();
    if failures > 0 {
        return Ok(outcome.with_error(CliError::tool(
            "batch",
            format!("{failures} of {total} calls failed"),
        )));
    }
    Ok(outcome)
}
