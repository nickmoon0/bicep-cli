use super::invoke_or_raw;
use crate::args;
use crate::cli::{FormatArgs, RefsArgs};
use crate::error::CliError;
use crate::output::{Format, Outcome};
use crate::tools::{absolute_existing, path_string, uri_to_path};
use crate::transport::Ctx;
use serde_json::{Value, json};

pub async fn format(ctx: &Ctx, args: FormatArgs) -> Result<Outcome, CliError> {
    let path = absolute_existing(&args.file)?;
    let original = std::fs::read_to_string(&path)
        .map_err(|e| CliError::io(&format!("cannot read {}", path.display()), e))?;
    let value = invoke_or_raw!(
        ctx,
        "format_bicep_file",
        args!("filePath" => path_string(&path))
    );
    let content = value["content"].as_str().unwrap_or_default().to_owned();
    let changed = content != original;
    let path_str = path_string(&path);

    if args.check {
        let outcome = Outcome::render(
            ctx.out(Format::Json),
            json!({"path": path_str, "changed": changed}),
            |_| {
                if changed {
                    format!("{path_str}: needs formatting")
                } else {
                    format!("{path_str}: formatted")
                }
            },
        );
        return Ok(if changed {
            outcome.with_error(CliError::tool(
                "format_bicep_file",
                format!("{path_str} is not formatted"),
            ))
        } else {
            outcome
        });
    }
    if args.write {
        if changed {
            super::write_file(&path, &content)?;
        }
        return Ok(Outcome::render(
            ctx.out(Format::Json),
            json!({"path": path_str, "changed": changed, "written": changed}),
            |_| {
                if changed {
                    format!("formatted {path_str}")
                } else {
                    format!("{path_str} unchanged")
                }
            },
        ));
    }
    Ok(Outcome::render(
        ctx.out(Format::Text),
        json!({"content": content, "changed": changed}),
        |v| v["content"].as_str().unwrap_or_default().to_owned(),
    ))
}

pub async fn refs(ctx: &Ctx, args: RefsArgs) -> Result<Outcome, CliError> {
    let path = absolute_existing(&args.file)?;
    let mut value = invoke_or_raw!(
        ctx,
        "get_file_references",
        args!("filePath" => path_string(&path))
    );
    let paths: Vec<Value> = value["fileUris"]
        .as_array()
        .map(|uris| {
            uris.iter()
                .filter_map(Value::as_str)
                .map(|u| Value::String(path_string(&uri_to_path(u))))
                .collect()
        })
        .unwrap_or_default();
    if args.paths {
        return Ok(Outcome::render(
            ctx.out(Format::Json),
            Value::Array(paths),
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
    value["paths"] = Value::Array(paths);
    Ok(Outcome::render(ctx.out(Format::Json), value, |v| {
        crate::output::lines_of(v, "paths")
    }))
}
