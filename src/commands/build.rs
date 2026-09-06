use super::{invoke_or_raw, write_file};
use crate::args;
use crate::cli::{BuildArgs, BuildParamsArgs, SnapshotArgs};
use crate::error::CliError;
use crate::output::{Format, Outcome, pretty};
use crate::tools::{
    absolute, absolute_existing, augment_diagnostics, count_level, diagnostics_text, path_string,
};
use crate::transport::Ctx;
use serde_json::{Value, json};

fn diagnostics_of(value: &mut Value) -> Vec<Value> {
    match value.get_mut("diagnostics").and_then(Value::as_array_mut) {
        Some(diags) => {
            augment_diagnostics(diags);
            diags.clone()
        }
        None => Vec::new(),
    }
}

fn failure(tool: &str, what: &str, diags: &[Value]) -> CliError {
    CliError::tool(
        tool,
        format!(
            "{what} failed: {} error(s), {} warning(s)",
            count_level(diags, "error"),
            count_level(diags, "warning")
        ),
    )
}

fn text_summary(value: &Value, diags: &[Value], what: &str) -> String {
    let success = value["success"].as_bool().unwrap_or(false);
    let mut out = diagnostics_text(diags);
    if !out.is_empty() {
        out.push('\n');
    }
    let verdict = if success { "succeeded" } else { "failed" };
    out.push_str(&format!("{what} {verdict}"));
    out
}

/// Move a string field out to a file, replacing it with a `<field>File` path.
fn spill(value: &mut Value, field: &str, target: &std::path::Path) -> Result<(), CliError> {
    if let Some(content) = value[field].as_str() {
        let target = absolute(target)?;
        write_file(&target, content)?;
        value[field] = Value::Null;
        value[format!("{field}File")] = Value::String(path_string(&target));
    }
    Ok(())
}

pub async fn build(ctx: &Ctx, args: BuildArgs) -> Result<Outcome, CliError> {
    let path = absolute_existing(&args.file)?;
    let mut value = invoke_or_raw!(ctx, "build_bicep", args!("filePath" => path_string(&path)));
    let diags = diagnostics_of(&mut value);
    let success = value["success"].as_bool().unwrap_or(false);
    let error = (!success).then(|| failure("build_bicep", "build", &diags));

    if args.template_only {
        let outcome = match value["template"].as_str() {
            Some(t) => Outcome::text(t.to_owned()),
            None => Outcome::json(json!({"success": success, "diagnostics": diags})),
        };
        return Ok(with(outcome, error));
    }
    if let Some(out) = &args.output {
        spill(&mut value, "template", out)?;
    }
    if args.diagnostics_only {
        value = json!({"success": success, "diagnostics": diags});
    }
    let outcome = Outcome::render(ctx.out(Format::Json), value, |v| {
        text_summary(v, &diags, "build")
    });
    Ok(with(outcome, error))
}

pub async fn build_params(ctx: &Ctx, args: BuildParamsArgs) -> Result<Outcome, CliError> {
    let path = absolute_existing(&args.file)?;
    let mut value = invoke_or_raw!(
        ctx,
        "build_bicepparam",
        args!("filePath" => path_string(&path))
    );
    let diags = diagnostics_of(&mut value);
    let success = value["success"].as_bool().unwrap_or(false);
    let error = (!success).then(|| failure("build_bicepparam", "build", &diags));

    let only = if args.parameters_only {
        Some("parameters")
    } else if args.template_only {
        Some("template")
    } else {
        None
    };
    if let Some(field) = only {
        let outcome = match value[field].as_str() {
            Some(t) => Outcome::text(t.to_owned()),
            None => Outcome::json(json!({"success": success, "diagnostics": diags})),
        };
        return Ok(with(outcome, error));
    }
    if let Some(out) = &args.output {
        spill(&mut value, "parameters", out)?;
    }
    if let Some(out) = &args.template_out {
        spill(&mut value, "template", out)?;
    }
    if args.diagnostics_only {
        value = json!({"success": success, "diagnostics": diags});
    }
    let outcome = Outcome::render(ctx.out(Format::Json), value, |v| {
        text_summary(v, &diags, "build")
    });
    Ok(with(outcome, error))
}

pub async fn snapshot(ctx: &Ctx, args: SnapshotArgs) -> Result<Outcome, CliError> {
    let path = absolute_existing(&args.file)?;
    let mut params = args!("filePath" => path_string(&path));
    let optional = [
        ("tenantId", args.tenant),
        ("managementGroupId", args.management_group),
        ("subscriptionId", args.subscription),
        ("resourceGroup", args.resource_group),
        ("location", args.location),
        ("deploymentName", args.deployment_name),
    ];
    for (key, value) in optional {
        if let Some(v) = value {
            params.insert(key.to_owned(), Value::String(v));
        }
    }
    let value = invoke_or_raw!(ctx, "get_deployment_snapshot", params);
    Ok(Outcome::render(ctx.out(Format::Json), value, pretty))
}

fn with(outcome: Outcome, error: Option<CliError>) -> Outcome {
    match error {
        Some(e) => outcome.with_error(e),
        None => outcome,
    }
}
