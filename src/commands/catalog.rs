use super::invoke_or_raw;
use crate::args;
use crate::cli::{
    AvmArgs, ExtSchemaArgs, ExtTypesArgs, ExtensionsArgs, ResourceTypesArgs, SchemaArgs,
};
use crate::error::CliError;
use crate::output::{Format, Outcome, lines_of, pretty};
use crate::tools::{
    cmp_api_version, filter_contains, is_preview, latest_per_type, parse_embedded_json,
    sort_resource_types, split_type_version,
};
use crate::transport::Ctx;
use serde_json::{Map, Value, json};

fn strings(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn shape_types(
    mut list: Vec<String>,
    filter: Option<&str>,
    latest: bool,
    include_preview: bool,
) -> Vec<String> {
    if let Some(f) = filter {
        list = filter_contains(&list, f);
    }
    if latest {
        list = latest_per_type(&list, include_preview);
    }
    sort_resource_types(&mut list);
    list
}

pub async fn resource_types(ctx: &Ctx, args: ResourceTypesArgs) -> Result<Outcome, CliError> {
    let value = invoke_or_raw!(
        ctx,
        "list_azure_resource_types",
        args!("providerNamespace" => args.namespace.clone())
    );
    let list = shape_types(
        strings(&value, "resourceTypes"),
        args.filter.as_deref(),
        args.latest,
        args.include_preview,
    );
    if list.is_empty() {
        ctx.warn(&format!(
            "no resource types found for `{}` (namespaces look like Microsoft.Storage)",
            args.namespace
        ));
    }
    Ok(Outcome::render(
        ctx.out(Format::Json),
        json!({"resourceTypes": list}),
        |v| lines_of(v, "resourceTypes"),
    ))
}

pub async fn ext_types(ctx: &Ctx, args: ExtTypesArgs) -> Result<Outcome, CliError> {
    let value = invoke_or_raw!(
        ctx,
        "list_extension_resource_types",
        args!("extensionReference" => args.extension.clone())
    );
    let list = shape_types(
        strings(&value, "resourceTypes"),
        args.filter.as_deref(),
        args.latest,
        args.include_preview,
    );
    if list.is_empty() {
        ctx.warn(&format!(
            "no resource types found for extension `{}`",
            args.extension
        ));
    }
    Ok(Outcome::render(
        ctx.out(Format::Json),
        json!({"resourceTypes": list}),
        |v| lines_of(v, "resourceTypes"),
    ))
}

/// Pick the API version for `type_name` from a `type@version` list.
fn pick_latest(list: &[String], type_name: &str, include_preview: bool) -> Option<String> {
    let wanted = type_name.to_ascii_lowercase();
    let mut candidates: Vec<&str> = list
        .iter()
        .filter_map(|e| {
            let (t, v) = split_type_version(e);
            (t.eq_ignore_ascii_case(&wanted)).then_some(v).flatten()
        })
        .collect();
    if !include_preview && candidates.iter().any(|v| !is_preview(v)) {
        candidates.retain(|v| !is_preview(v));
    }
    candidates.sort_by(|a, b| cmp_api_version(b, a));
    candidates.first().map(|s| (*s).to_owned())
}

struct SchemaRequest<'a> {
    resource_type: &'a str,
    api_version: Option<&'a str>,
    latest: bool,
    include_preview: bool,
    no_descriptions: bool,
    no_readonly: bool,
}

async fn resolve_version(
    ctx: &Ctx,
    req: &SchemaRequest<'_>,
    list_tool: &str,
    list_args: Map<String, Value>,
) -> Result<(String, String), CliError> {
    let (type_name, inline_version) = split_type_version(req.resource_type);
    let version = match (inline_version, req.api_version, req.latest) {
        (Some(v), None, false) => v.to_owned(),
        (None, Some(v), false) => v.to_owned(),
        (Some(a), Some(b), _) if a != b => {
            return Err(CliError::usage(format!(
                "conflicting API versions `{a}` and `{b}`"
            )));
        }
        (_, _, true) => {
            let listed = ctx.session().await?.call(list_tool, list_args).await?;
            pick_latest(
                &strings(&listed, "resourceTypes"),
                type_name,
                req.include_preview,
            )
            .ok_or_else(|| CliError::usage(format!("no API versions found for `{type_name}`")))?
        }
        (Some(v), Some(_), false) => v.to_owned(),
        (None, None, false) => {
            return Err(CliError::usage(
                "an API version is required: use type@version, --api-version, or --latest",
            ));
        }
    };
    Ok((type_name.to_owned(), version))
}

fn schema_outcome(ctx: &Ctx, value: Value) -> Outcome {
    let schema = parse_embedded_json(&value, "schema");
    Outcome::render(ctx.out(Format::Json), schema, pretty)
}

pub async fn schema(ctx: &Ctx, args: SchemaArgs) -> Result<Outcome, CliError> {
    let req = SchemaRequest {
        resource_type: &args.resource_type,
        api_version: args.api_version.as_deref(),
        latest: args.latest,
        include_preview: args.include_preview,
        no_descriptions: args.no_descriptions,
        no_readonly: args.no_readonly,
    };
    let (type_name, _) = split_type_version(req.resource_type);
    let namespace = type_name.split('/').next().unwrap_or(type_name).to_owned();
    let (type_name, version) = resolve_version(
        ctx,
        &req,
        "list_azure_resource_types",
        args!("providerNamespace" => namespace),
    )
    .await?;
    let value = invoke_or_raw!(
        ctx,
        "get_azure_resource_type_schema",
        args!(
            "resourceType" => type_name,
            "apiVersion" => version,
            "excludeDescriptions" => req.no_descriptions,
            "excludeReadOnlyProperties" => req.no_readonly,
        )
    );
    Ok(schema_outcome(ctx, value))
}

pub async fn ext_schema(ctx: &Ctx, args: ExtSchemaArgs) -> Result<Outcome, CliError> {
    let req = SchemaRequest {
        resource_type: &args.resource_type,
        api_version: args.api_version.as_deref(),
        latest: args.latest,
        include_preview: args.include_preview,
        no_descriptions: args.no_descriptions,
        no_readonly: args.no_readonly,
    };
    let (type_name, version) = resolve_version(
        ctx,
        &req,
        "list_extension_resource_types",
        args!("extensionReference" => args.extension.clone()),
    )
    .await?;
    let value = invoke_or_raw!(
        ctx,
        "get_extension_resource_type_schema",
        args!(
            "extensionReference" => args.extension.clone(),
            "resourceType" => type_name,
            "apiVersion" => version,
            "excludeDescriptions" => req.no_descriptions,
            "excludeReadOnlyProperties" => req.no_readonly,
        )
    );
    Ok(schema_outcome(ctx, value))
}

pub async fn extensions(ctx: &Ctx, args: ExtensionsArgs) -> Result<Outcome, CliError> {
    let value = invoke_or_raw!(ctx, "list_well_known_extensions", Map::new());
    if args.names {
        let names: Vec<Value> = value["extensions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| e["name"].as_str())
                    .map(|s| Value::String(s.into()))
                    .collect()
            })
            .unwrap_or_default();
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
    Ok(Outcome::render(ctx.out(Format::Json), value, |v| {
        v["extensions"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|e| {
                        let tags = e["availableTags"]
                            .as_array()
                            .map(|t| {
                                t.iter()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default();
                        format!(
                            "{}\t{}\ttags: {}",
                            e["name"].as_str().unwrap_or(""),
                            e["ociReference"].as_str().unwrap_or(""),
                            tags
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }))
}

fn latest_version(module: &Value) -> Option<String> {
    module["versions"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .max_by(|a, b| semver_key(a).cmp(&semver_key(b)))
        .map(str::to_owned)
}

fn semver_key(v: &str) -> (Vec<u64>, String) {
    let nums = v
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect();
    (nums, v.to_owned())
}

pub async fn avm(ctx: &Ctx, args: AvmArgs) -> Result<Outcome, CliError> {
    let value = invoke_or_raw!(ctx, "list_avm_metadata", Map::new());
    let mut modules: Vec<Value> = value["modules"].as_array().cloned().unwrap_or_default();
    if let Some(filter) = &args.filter {
        let needle = filter.to_ascii_lowercase();
        modules.retain(|m| {
            let path = m["modulePath"].as_str().unwrap_or("").to_ascii_lowercase();
            let desc = m["description"].as_str().unwrap_or("").to_ascii_lowercase();
            path.contains(&needle) || desc.contains(&needle)
        });
    }
    modules.sort_by(|a, b| a["modulePath"].as_str().cmp(&b["modulePath"].as_str()));
    let total = modules.len();
    if let Some(limit) = args.limit {
        modules.truncate(limit);
    }
    if args.names {
        let names: Vec<Value> = modules.iter().map(|m| m["modulePath"].clone()).collect();
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
    let shaped: Vec<Value> = if args.full {
        modules
    } else {
        modules
            .iter()
            .map(|m| {
                json!({
                    "modulePath": m["modulePath"],
                    "latestVersion": latest_version(m),
                    "description": m["description"].as_str().map(|d| d.lines().next().unwrap_or("").trim()),
                })
            })
            .collect()
    };
    let hint = if args.full {
        Value::Null
    } else {
        Value::String("use --full for all versions and documentation URIs; use br/public:<modulePath>:<version> in Bicep".into())
    };
    let mut out = json!({"count": shaped.len(), "total": total, "modules": shaped});
    if !hint.is_null() {
        out["hint"] = hint;
    }
    Ok(Outcome::render(ctx.out(Format::Json), out, |v| {
        v["modules"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|m| {
                        format!(
                            "{} ({})\t{}",
                            m["modulePath"].as_str().unwrap_or(""),
                            m["latestVersion"]
                                .as_str()
                                .or(m["versions"]
                                    .as_array()
                                    .and_then(|x| x.last())
                                    .and_then(Value::as_str))
                                .unwrap_or("?"),
                            m["description"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }))
}

pub async fn best_practices(ctx: &Ctx) -> Result<Outcome, CliError> {
    let value = invoke_or_raw!(ctx, "get_bicep_best_practices", Map::new());
    Ok(Outcome::render(ctx.out(Format::Text), value, |v| {
        v["content"].as_str().unwrap_or_default().to_owned()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_latest_prefers_stable_and_is_case_insensitive() {
        let list: Vec<String> = [
            "Microsoft.X/a@2024-12-01-preview",
            "Microsoft.X/a@2024-11-01",
            "Microsoft.X/b@2020-01-01",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            pick_latest(&list, "microsoft.x/A", false).as_deref(),
            Some("2024-11-01")
        );
        assert_eq!(
            pick_latest(&list, "Microsoft.X/a", true).as_deref(),
            Some("2024-12-01-preview")
        );
        assert_eq!(pick_latest(&list, "Microsoft.X/zzz", true), None);
    }

    #[test]
    fn latest_semver() {
        let m = json!({"versions": ["0.9.0", "0.10.0", "0.2.1"]});
        assert_eq!(latest_version(&m).as_deref(), Some("0.10.0"));
    }
}
