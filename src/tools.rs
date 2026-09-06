//! Pure helpers shared by the command implementations: path/URI handling,
//! diagnostic augmentation, API-version ordering, argument parsing.

use crate::error::CliError;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Names of the 14 upstream tools, in a stable order.
#[cfg(test)]
pub const TOOL_NAMES: [&str; 14] = [
    "build_bicep",
    "build_bicepparam",
    "format_bicep_file",
    "get_file_references",
    "decompile_arm_template_file",
    "decompile_arm_parameters_file",
    "get_deployment_snapshot",
    "list_azure_resource_types",
    "get_azure_resource_type_schema",
    "list_extension_resource_types",
    "get_extension_resource_type_schema",
    "list_well_known_extensions",
    "list_avm_metadata",
    "get_bicep_best_practices",
];

/// Subcommand -> tool mapping, used by `tools --map` and the README.
pub const COMMAND_MAP: [(&str, &str); 14] = [
    ("build", "build_bicep"),
    ("build-params", "build_bicepparam"),
    ("format", "format_bicep_file"),
    ("refs", "get_file_references"),
    ("decompile", "decompile_arm_template_file"),
    ("decompile-params", "decompile_arm_parameters_file"),
    ("snapshot", "get_deployment_snapshot"),
    ("resource-types", "list_azure_resource_types"),
    ("schema", "get_azure_resource_type_schema"),
    ("ext-types", "list_extension_resource_types"),
    ("ext-schema", "get_extension_resource_type_schema"),
    ("extensions", "list_well_known_extensions"),
    ("avm", "list_avm_metadata"),
    ("best-practices", "get_bicep_best_practices"),
];

// ---------------------------------------------------------------------------
// Paths and file URIs
// ---------------------------------------------------------------------------

/// Make a user-supplied path absolute (relative to cwd) and require it to
/// exist. Runs before any server contact so a typo costs milliseconds.
pub fn absolute_existing(path: &Path) -> Result<PathBuf, CliError> {
    let abs = std::path::absolute(path)
        .map_err(|e| CliError::usage(format!("cannot resolve path {}: {e}", path.display())))?;
    if !abs.exists() {
        return Err(CliError::usage(format!(
            "file not found: {}",
            abs.display()
        )));
    }
    Ok(abs)
}

/// Absolute path without the existence check (for outputs).
pub fn absolute(path: &Path) -> Result<PathBuf, CliError> {
    std::path::absolute(path)
        .map_err(|e| CliError::usage(format!("cannot resolve path {}: {e}", path.display())))
}

pub fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let keep = b.is_ascii_alphanumeric() || b"-._~/:".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `file:///home/x/a.bicep` -> `/home/x/a.bicep`;
/// `file:///C:/x/a.bicep` -> `C:\x\a.bicep`. Decided by the URI's shape, not
/// the host OS, so output is deterministic everywhere.
pub fn uri_to_path(uri: &str) -> PathBuf {
    let Some(rest) = uri.strip_prefix("file://") else {
        return PathBuf::from(uri);
    };
    // Drop an authority component if present (file://host/path); localhost only.
    let rest = if let Some(stripped) = rest.strip_prefix("localhost") {
        stripped
    } else {
        rest
    };
    let decoded = percent_decode(rest);
    let is_drive = decoded.len() >= 3
        && decoded.as_bytes()[0] == b'/'
        && decoded.as_bytes()[1].is_ascii_alphabetic()
        && decoded.as_bytes()[2] == b':';
    if is_drive {
        PathBuf::from(decoded[1..].replace('/', "\\"))
    } else {
        PathBuf::from(decoded)
    }
}

/// Inverse of [`uri_to_path`] (test helper).
#[cfg(test)]
pub fn path_to_uri(path: &Path) -> String {
    let s = path_string(path).replace('\\', "/");
    let is_drive = s.len() >= 2 && s.as_bytes()[0].is_ascii_alphabetic() && s.as_bytes()[1] == b':';
    if is_drive {
        format!("file:///{}", percent_encode(&s))
    } else {
        format!("file://{}", percent_encode(&s))
    }
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// Convert a UTF-16 code-unit offset (what the .NET server reports) into a
/// 1-based (line, column) pair. Columns are also in UTF-16 units.
pub fn line_col(text: &str, utf16_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    let mut seen = 0usize;
    for ch in text.chars() {
        if seen >= utf16_offset {
            break;
        }
        seen += ch.len_utf16();
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += ch.len_utf16();
        }
    }
    (line, col)
}

/// Add `path`, `line`, `column` to every diagnostic object in place.
pub fn augment_diagnostics(diagnostics: &mut [Value]) {
    let mut cache: HashMap<String, Option<String>> = HashMap::new();
    for diag in diagnostics.iter_mut() {
        let Some(uri) = diag
            .get("fileUri")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let path = uri_to_path(&uri);
        let text = cache
            .entry(uri.clone())
            .or_insert_with(|| std::fs::read_to_string(&path).ok())
            .clone();
        let Some(obj) = diag.as_object_mut() else {
            continue;
        };
        obj.insert("path".into(), Value::String(path_string(&path)));
        if let (Some(text), Some(pos)) = (text, obj.get("position").and_then(Value::as_u64)) {
            let (line, col) = line_col(&text, pos as usize);
            obj.insert("line".into(), Value::from(line));
            obj.insert("column".into(), Value::from(col));
        }
    }
}

pub fn count_level(diagnostics: &[Value], level: &str) -> usize {
    diagnostics
        .iter()
        .filter(|d| {
            d.get("level")
                .and_then(Value::as_str)
                .is_some_and(|l| l.eq_ignore_ascii_case(level))
        })
        .count()
}

/// `error path:line:col code message` per diagnostic.
pub fn diagnostics_text(diagnostics: &[Value]) -> String {
    diagnostics
        .iter()
        .map(|d| {
            let level = d["level"].as_str().unwrap_or("info").to_lowercase();
            let path = d["path"].as_str().or(d["fileUri"].as_str()).unwrap_or("?");
            let line = d["line"].as_u64().unwrap_or(0);
            let col = d["column"].as_u64().unwrap_or(0);
            let code = d["code"].as_str().unwrap_or("");
            let msg = d["message"].as_str().unwrap_or("");
            format!("{level} {path}:{line}:{col} {code} {msg}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Resource types and API versions
// ---------------------------------------------------------------------------

/// `Microsoft.KeyVault/vaults@2024-11-01` -> ("Microsoft.KeyVault/vaults", Some("2024-11-01")).
pub fn split_type_version(s: &str) -> (&str, Option<&str>) {
    match s.rsplit_once('@') {
        Some((t, v)) if !v.is_empty() => (t, Some(v)),
        _ => (s, None),
    }
}

/// Anything with a suffix after the date (`-preview`, `-privatepreview`, …)
/// or non-date versions like `beta`.
pub fn is_preview(version: &str) -> bool {
    let bytes = version.as_bytes();
    let looks_like_date = bytes.len() >= 10
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit);
    if looks_like_date {
        bytes.len() > 10
    } else {
        version.to_ascii_lowercase().contains("beta")
            || version.to_ascii_lowercase().contains("preview")
    }
}

/// Newer versions sort greater. Date part first; a stable release beats a
/// preview of the same date.
pub fn cmp_api_version(a: &str, b: &str) -> std::cmp::Ordering {
    let date = |v: &str| v.get(..10).unwrap_or(v).to_owned();
    date(a)
        .cmp(&date(b))
        .then_with(|| is_preview(b).cmp(&is_preview(a)))
        .then_with(|| a.cmp(b))
}

/// Sort `type@version` entries by type, newest version first.
pub fn sort_resource_types(list: &mut [String]) {
    list.sort_by(|a, b| {
        let (ta, va) = split_type_version(a);
        let (tb, vb) = split_type_version(b);
        ta.to_ascii_lowercase()
            .cmp(&tb.to_ascii_lowercase())
            .then_with(|| cmp_api_version(vb.unwrap_or(""), va.unwrap_or("")))
    });
}

/// Keep only the newest version of each type. Previews are skipped unless
/// `include_preview` or a type has nothing else.
pub fn latest_per_type(list: &[String], include_preview: bool) -> Vec<String> {
    let mut best: HashMap<String, (String, bool)> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for entry in list {
        let (t, Some(v)) = split_type_version(entry) else {
            continue;
        };
        let key = t.to_ascii_lowercase();
        let preview = is_preview(v);
        let candidate_ok = include_preview || !preview;
        match best.get_mut(&key) {
            None => {
                order.push(key.clone());
                best.insert(key, (entry.clone(), candidate_ok));
            }
            Some((current, current_ok)) => {
                let (_, cv) = split_type_version(current);
                let replace = match (*current_ok, candidate_ok) {
                    (false, true) => true,
                    (true, false) => false,
                    _ => cmp_api_version(v, cv.unwrap_or("")) == std::cmp::Ordering::Greater,
                };
                if replace {
                    *current = entry.clone();
                    *current_ok = candidate_ok;
                }
            }
        }
    }
    let mut out: Vec<String> = order.into_iter().map(|k| best[&k].0.clone()).collect();
    out.sort_by_key(|s| s.to_ascii_lowercase());
    out
}

pub fn filter_contains(list: &[String], needle: &str) -> Vec<String> {
    let needle = needle.to_ascii_lowercase();
    list.iter()
        .filter(|s| s.to_ascii_lowercase().contains(&needle))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Generic argument helpers
// ---------------------------------------------------------------------------

/// `key=value` where value is parsed as JSON when possible, else a string.
pub fn parse_kv(arg: &str) -> Result<(String, Value), CliError> {
    let (k, v) = arg
        .split_once('=')
        .ok_or_else(|| CliError::usage(format!("--arg expects key=value, got `{arg}`")))?;
    if k.is_empty() {
        return Err(CliError::usage(format!("--arg has empty key in `{arg}`")));
    }
    let value = serde_json::from_str::<Value>(v).unwrap_or_else(|_| Value::String(v.to_owned()));
    Ok((k.to_owned(), value))
}

/// Parse a `--args` JSON object.
pub fn parse_args_json(s: &str) -> Result<Map<String, Value>, CliError> {
    match serde_json::from_str::<Value>(s) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(CliError::usage("--args must be a JSON object")),
        Err(e) => Err(CliError::usage(format!("--args is not valid JSON: {e}"))),
    }
}

/// Parse the `schema` field (a JSON document as a string) into an object.
pub fn parse_embedded_json(value: &Value, key: &str) -> Value {
    match value.get(key) {
        Some(Value::String(s)) => {
            serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn command_map_covers_every_tool() {
        for tool in TOOL_NAMES {
            assert!(
                COMMAND_MAP.iter().any(|(_, t)| *t == tool),
                "{tool} has no subcommand"
            );
        }
        assert_eq!(COMMAND_MAP.len(), TOOL_NAMES.len());
    }

    #[test]
    fn uri_roundtrip_unix() {
        let p = uri_to_path("file:///home/nick/a%20b.bicep");
        assert_eq!(p, PathBuf::from("/home/nick/a b.bicep"));
        assert_eq!(
            path_to_uri(Path::new("/home/nick/a b.bicep")),
            "file:///home/nick/a%20b.bicep"
        );
    }

    #[test]
    fn uri_roundtrip_windows() {
        let p = uri_to_path("file:///C:/Users/nick/main.bicep");
        assert_eq!(p, PathBuf::from("C:\\Users\\nick\\main.bicep"));
        assert_eq!(
            path_to_uri(Path::new("C:\\Users\\nick\\main.bicep")),
            "file:///C:/Users/nick/main.bicep"
        );
    }

    #[test]
    fn non_file_uri_passes_through() {
        assert_eq!(
            uri_to_path("br:mcr.microsoft.com/x"),
            PathBuf::from("br:mcr.microsoft.com/x")
        );
    }

    #[test]
    fn line_col_basic_and_crlf_and_utf16() {
        assert_eq!(line_col("abc\ndef", 0), (1, 1));
        assert_eq!(line_col("abc\ndef", 5), (2, 2));
        assert_eq!(line_col("ab\r\ncd", 4), (2, 1));
        // '😀' is 2 UTF-16 units; offset 3 lands on 'x'
        assert_eq!(line_col("😀x", 2), (1, 3));
    }

    #[test]
    fn preview_detection_and_ordering() {
        assert!(is_preview("2024-12-01-preview"));
        assert!(!is_preview("2024-11-01"));
        assert!(is_preview("beta"));
        assert!(!is_preview("v1.0"));
        assert_eq!(
            cmp_api_version("2024-11-01", "2024-11-01-preview"),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            cmp_api_version("2023-01-01", "2024-01-01"),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn latest_per_type_prefers_stable() {
        let list: Vec<String> = [
            "Microsoft.X/a@2024-12-01-preview",
            "Microsoft.X/a@2024-11-01",
            "Microsoft.X/a@2020-01-01",
            "Microsoft.X/b@2025-01-01-preview",
            "Microsoft.X/b@2024-01-01-preview",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(
            latest_per_type(&list, false),
            vec![
                "Microsoft.X/a@2024-11-01",
                "Microsoft.X/b@2025-01-01-preview"
            ]
        );
        assert_eq!(
            latest_per_type(&list, true),
            vec![
                "Microsoft.X/a@2024-12-01-preview",
                "Microsoft.X/b@2025-01-01-preview"
            ]
        );
    }

    #[test]
    fn sort_is_by_type_then_newest_first() {
        let mut list: Vec<String> = ["b@2020-01-01", "a@2020-01-01", "a@2024-01-01"]
            .into_iter()
            .map(String::from)
            .collect();
        sort_resource_types(&mut list);
        assert_eq!(list, ["a@2024-01-01", "a@2020-01-01", "b@2020-01-01"]);
    }

    #[test]
    fn kv_parsing() {
        assert_eq!(parse_kv("a=1").unwrap(), ("a".into(), json!(1)));
        assert_eq!(parse_kv("a=true").unwrap(), ("a".into(), json!(true)));
        assert_eq!(parse_kv("a=hello").unwrap(), ("a".into(), json!("hello")));
        assert_eq!(parse_kv("p=C:\\x").unwrap().1, json!("C:\\x"));
        assert_eq!(parse_kv("nope").unwrap_err().exit_code(), 2);
    }

    #[test]
    fn embedded_json_is_parsed() {
        let v = json!({"schema": "{\"a\":1}"});
        assert_eq!(parse_embedded_json(&v, "schema"), json!({"a": 1}));
    }

    #[test]
    fn augment_reads_file() {
        let dir = std::env::temp_dir().join(format!("bcp-diag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.bicep");
        std::fs::write(&file, "param a string\nparam b = \n").unwrap();
        let mut diags =
            vec![json!({"fileUri": path_to_uri(&file), "position": 25, "level": "Error"})];
        augment_diagnostics(&mut diags);
        assert_eq!(diags[0]["line"], 2);
        assert_eq!(diags[0]["column"], 11);
        assert_eq!(diags[0]["path"], path_string(&file));
        assert_eq!(count_level(&diags, "error"), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
