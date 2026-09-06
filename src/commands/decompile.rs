use super::{invoke_or_raw, write_file};
use crate::args;
use crate::cli::DecompileArgs;
use crate::error::CliError;
use crate::output::{Format, Outcome};
use crate::tools::{absolute, absolute_existing, path_string, uri_to_path};
use crate::transport::Ctx;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

pub async fn decompile(
    ctx: &Ctx,
    args: DecompileArgs,
    parameters: bool,
) -> Result<Outcome, CliError> {
    let tool = if parameters {
        "decompile_arm_parameters_file"
    } else {
        "decompile_arm_template_file"
    };
    let path = absolute_existing(&args.file)?;
    let value = invoke_or_raw!(ctx, tool, args!("filePath" => path_string(&path)));

    let entrypoint = value["entrypointUri"]
        .as_str()
        .map(uri_to_path)
        .unwrap_or_default();
    let mut files: Vec<(PathBuf, String)> = value["filesToSave"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(uri, content)| {
                    content.as_str().map(|c| (uri_to_path(uri), c.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort_by(|a, b| a.0.cmp(&b.0));

    if args.write {
        let out_dir = match &args.out_dir {
            Some(dir) => Some(absolute(dir)?),
            None => None,
        };
        let targets: Vec<(PathBuf, String)> = files
            .into_iter()
            .map(|(p, c)| {
                let target = match &out_dir {
                    Some(dir) => dir.join(p.file_name().unwrap_or_default()),
                    None => p,
                };
                (target, c)
            })
            .collect();
        if let Some((existing, _)) = targets.iter().find(|(p, _)| !args.force && p.exists()) {
            return Err(CliError::usage(format!(
                "{} already exists; pass --force to overwrite",
                existing.display()
            )));
        }
        let mut written = Vec::new();
        for (target, content) in &targets {
            write_file(target, content)?;
            written.push(Value::String(path_string(target)));
        }
        let entry_target = targets
            .iter()
            .find(|(p, _)| p.file_name() == entrypoint.file_name())
            .map(|(p, _)| path_string(p))
            .unwrap_or_else(|| path_string(&entrypoint));
        return Ok(Outcome::render(
            ctx.out(Format::Json),
            json!({"entrypoint": entry_target, "written": written}),
            |v| crate::output::lines_of(v, "written"),
        ));
    }

    let mut map = Map::new();
    for (p, c) in &files {
        map.insert(path_string(p), Value::String(c.clone()));
    }
    let single = files.len() == 1;
    let rendered = files
        .iter()
        .map(|(p, c)| {
            if single {
                c.clone()
            } else {
                format!("// ==== {}\n{c}", p.display())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Outcome::render(
        ctx.out(Format::Json),
        json!({"entrypoint": path_string(&entrypoint), "files": map}),
        move |_| rendered,
    ))
}
