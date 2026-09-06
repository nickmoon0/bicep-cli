//! End-to-end tests against the real Bicep MCP server.
//!
//! Skipped unless `BCP_E2E=1` (they need .NET 10 and the server package).
//! Tests that hit the network (AVM metadata, extension tags) additionally
//! require `BCP_E2E_NETWORK=1`. Set `BCP_TRANSPORT=daemon` with a running
//! `bcp serve` to exercise the daemon path.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::Instant;

fn enabled() -> bool {
    std::env::var("BCP_E2E").map(|v| v == "1").unwrap_or(false)
}

fn network_enabled() -> bool {
    std::env::var("BCP_E2E_NETWORK")
        .map(|v| v == "1")
        .unwrap_or(false)
}

macro_rules! require_e2e {
    () => {
        if !enabled() {
            eprintln!("skipped: set BCP_E2E=1 to run");
            return;
        }
    };
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bicep")
}

fn state_dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(explicit) = std::env::var_os("BCP_STATE_DIR") {
            return PathBuf::from(explicit);
        }
        std::env::temp_dir().join(format!("bcp-e2e-{}", std::process::id()))
    })
}

fn bcp(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bcp"))
        .args(args)
        .env("BCP_STATE_DIR", state_dir())
        .output()
        .expect("bcp runs")
}

fn stdout_json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}):\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn stderr_error(out: &Output) -> Value {
    let text = String::from_utf8_lossy(&out.stderr);
    let line = text
        .lines()
        .rev()
        .find(|l| l.starts_with('{'))
        .unwrap_or_else(|| panic!("no JSON on stderr: {text}"));
    serde_json::from_str(line).expect("stderr JSON")
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}

fn fixture(name: &str) -> String {
    fixtures().join(name).to_string_lossy().into_owned()
}

#[test]
fn build_good_file_succeeds() {
    require_e2e!();
    let out = bcp(&["build", &fixture("good.bicep")]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["success"], true);
    assert!(v["template"].as_str().unwrap().contains("storageAccounts"));
}

#[test]
fn build_bad_file_exits_1_with_line_and_column() {
    require_e2e!();
    let out = bcp(&["build", &fixture("bad.bicep"), "--compact"]);
    assert_eq!(code(&out), 1);
    let v = stdout_json(&out);
    assert_eq!(v["success"], false);
    let diag = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["level"] == "Error")
        .expect("an error diagnostic");
    assert_eq!(diag["line"], 1);
    assert!(diag["column"].as_u64().unwrap() > 1);
    assert!(diag["path"].as_str().unwrap().ends_with("bad.bicep"));
    let err = stderr_error(&out);
    assert_eq!(err["error"]["kind"], "tool");
    assert_eq!(err["error"]["exitCode"], 1);
}

#[test]
fn build_template_only_and_output_file() {
    require_e2e!();
    let out = bcp(&["build", &fixture("good.bicep"), "--template-only"]);
    assert_eq!(code(&out), 0);
    let template: Value = serde_json::from_slice(&out.stdout).expect("template JSON");
    assert!(template["resources"].is_array());

    let target = state_dir().join("good.json");
    let out = bcp(&[
        "build",
        &fixture("good.bicep"),
        "-o",
        &target.to_string_lossy(),
    ]);
    assert_eq!(code(&out), 0);
    let v = stdout_json(&out);
    assert!(v["template"].is_null());
    assert!(v["templateFile"].as_str().unwrap().ends_with("good.json"));
    assert!(target.exists());
}

#[test]
fn build_params_produces_parameters_and_template() {
    require_e2e!();
    let out = bcp(&["build-params", &fixture("main.bicepparam")]);
    assert_eq!(code(&out), 0);
    let v = stdout_json(&out);
    assert_eq!(v["success"], true);
    assert!(v["parameters"].as_str().unwrap().contains("westeurope"));
    assert!(v["template"].as_str().unwrap().contains("storageAccounts"));
}

#[test]
fn format_prints_check_and_write() {
    require_e2e!();
    let out = bcp(&["format", &fixture("unformatted.bicep")]);
    assert_eq!(code(&out), 0);
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("param location string"));

    let out = bcp(&["format", &fixture("unformatted.bicep"), "--check"]);
    assert_eq!(code(&out), 1);
    let out = bcp(&["format", &fixture("good.bicep"), "--check"]);
    assert_eq!(code(&out), 0);

    let copy = state_dir().join("copy.bicep");
    std::fs::create_dir_all(state_dir()).unwrap();
    std::fs::copy(fixtures().join("unformatted.bicep"), &copy).unwrap();
    let out = bcp(&["format", &copy.to_string_lossy(), "--write"]);
    assert_eq!(code(&out), 0);
    assert_eq!(stdout_json(&out)["written"], true);
    let out = bcp(&["format", &copy.to_string_lossy(), "--check"]);
    assert_eq!(code(&out), 0);
}

#[test]
fn refs_lists_module() {
    require_e2e!();
    let out = bcp(&["refs", &fixture("good.bicep"), "--paths"]);
    assert_eq!(code(&out), 0);
    let paths = stdout_json(&out);
    assert!(
        paths
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str().unwrap().ends_with("mod.bicep"))
    );
}

#[test]
fn decompile_template_and_params() {
    require_e2e!();
    let out = bcp(&["decompile", &fixture("template.json")]);
    assert_eq!(code(&out), 0);
    let v = stdout_json(&out);
    assert!(
        v["entrypoint"]
            .as_str()
            .unwrap()
            .ends_with("template.bicep")
    );
    assert_eq!(v["files"].as_object().unwrap().len(), 1);

    let out_dir = state_dir().join("decompiled");
    let out = bcp(&[
        "decompile",
        &fixture("template.json"),
        "--write",
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(code(&out), 0);
    assert!(out_dir.join("template.bicep").exists());
    let again = bcp(&[
        "decompile",
        &fixture("template.json"),
        "--write",
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(code(&again), 2, "refuses to overwrite without --force");
    let forced = bcp(&[
        "decompile",
        &fixture("template.json"),
        "--write",
        "--force",
        "--out-dir",
        &out_dir.to_string_lossy(),
    ]);
    assert_eq!(code(&forced), 0);

    let out = bcp(&["decompile-params", &fixture("params.json")]);
    assert_eq!(code(&out), 0);
    let v = stdout_json(&out);
    assert!(
        v["entrypoint"]
            .as_str()
            .unwrap()
            .ends_with("params.bicepparam")
    );
}

#[test]
fn snapshot_predicts_resources() {
    require_e2e!();
    let out = bcp(&[
        "snapshot",
        &fixture("main.bicepparam"),
        "--subscription",
        "00000000-0000-0000-0000-000000000000",
        "--resource-group",
        "rg1",
        "--location",
        "westeurope",
    ]);
    assert_eq!(code(&out), 0);
    let v = stdout_json(&out);
    assert_eq!(
        v["predictedResources"][0]["type"],
        "Microsoft.Storage/storageAccounts"
    );
    assert!(
        v["predictedResources"][0]["id"]
            .as_str()
            .unwrap()
            .contains("/resourceGroups/rg1/")
    );
}

#[test]
fn resource_types_and_schema() {
    require_e2e!();
    let out = bcp(&[
        "resource-types",
        "Microsoft.KeyVault",
        "--latest",
        "--filter",
        "Microsoft.KeyVault/vaults@",
    ]);
    assert_eq!(code(&out), 0);
    let types = stdout_json(&out)["resourceTypes"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(types.len(), 1);
    assert!(
        types[0]
            .as_str()
            .unwrap()
            .starts_with("Microsoft.KeyVault/vaults@")
    );

    let out = bcp(&[
        "schema",
        "Microsoft.KeyVault/vaults",
        "--latest",
        "--no-descriptions",
    ]);
    assert_eq!(code(&out), 0);
    let schema = stdout_json(&out);
    assert!(
        schema["title"]
            .as_str()
            .unwrap()
            .starts_with("Microsoft.KeyVault/vaults@")
    );
    assert!(schema["properties"].is_object());

    let out = bcp(&["schema", "Microsoft.KeyVault/vaults@1999-01-01"]);
    assert_eq!(code(&out), 1);
    let out = bcp(&["schema", "Microsoft.KeyVault/vaults"]);
    assert_eq!(code(&out), 2);
    let out = bcp(&["resource-types", "Bogus.Namespace"]);
    assert_eq!(code(&out), 0);
    assert_eq!(
        stdout_json(&out)["resourceTypes"].as_array().unwrap().len(),
        0
    );
}

#[test]
fn best_practices_is_markdown() {
    require_e2e!();
    let out = bcp(&["best-practices"]);
    assert_eq!(code(&out), 0);
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("# Bicep best-practices"));
    let out = bcp(&["best-practices", "-f", "json"]);
    assert!(stdout_json(&out)["content"].is_string());
}

#[test]
fn extensions_ext_types_ext_schema_and_avm_need_network() {
    require_e2e!();
    if !network_enabled() {
        eprintln!("skipped: set BCP_E2E_NETWORK=1 to run");
        return;
    }
    let out = bcp(&["extensions", "--names"]);
    assert_eq!(code(&out), 0);
    assert!(
        stdout_json(&out)
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == "MicrosoftGraph")
    );

    let ext = "br:mcr.microsoft.com/bicep/extensions/microsoftgraph/v1.0:1.0.0";
    let out = bcp(&["ext-types", ext, "--filter", "groups"]);
    assert_eq!(code(&out), 0);
    assert_eq!(
        stdout_json(&out)["resourceTypes"][0],
        "Microsoft.Graph/groups@v1.0"
    );

    let out = bcp(&[
        "ext-schema",
        ext,
        "Microsoft.Graph/groups",
        "--latest",
        "--no-descriptions",
    ]);
    assert_eq!(code(&out), 0);
    assert!(stdout_json(&out)["properties"].is_object());

    let out = bcp(&["avm", "--filter", "key-vault/vault", "--names"]);
    assert_eq!(code(&out), 0);
    assert!(
        stdout_json(&out)
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n == "avm/res/key-vault/vault")
    );
}

#[test]
fn tools_lists_all_fourteen() {
    require_e2e!();
    let out = bcp(&["tools", "--names"]);
    assert_eq!(code(&out), 0);
    assert_eq!(stdout_json(&out).as_array().unwrap().len(), 14);
    let out = bcp(&["tools", "--markdown"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("## `build_bicep`"));
}

#[test]
fn call_and_batch() {
    require_e2e!();
    let out = bcp(&[
        "call",
        "build_bicep",
        "--arg",
        &format!("filePath={}", fixture("good.bicep")),
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(stdout_json(&out)["success"], true);

    let out = bcp(&["call", "no_such_tool"]);
    assert_eq!(code(&out), 1);

    let input = format!(
        "{{\"tool\":\"get_file_references\",\"args\":{{\"filePath\":\"{}\"}}}}\nnot json\n",
        fixture("good.bicep").replace('\\', "\\\\")
    );
    let mut child = Command::new(env!("CARGO_BIN_EXE_bcp"))
        .args(["batch"])
        .env("BCP_STATE_DIR", state_dir())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), input.as_bytes()).unwrap();
    drop(child.stdin.take());
    let out = child.wait_with_output().unwrap();
    assert_eq!(code(&out), 1);
    let lines: Vec<Value> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["ok"], true);
    assert_eq!(lines[1]["ok"], false);
}

#[test]
fn usage_errors_do_not_start_a_server() {
    require_e2e!();
    let started = Instant::now();
    let out = bcp(&["build", "does-not-exist.bicep"]);
    assert_eq!(code(&out), 2);
    assert!(
        started.elapsed().as_millis() < 500,
        "should fail before spawning"
    );
    assert_eq!(stderr_error(&out)["error"]["kind"], "usage");
}

#[test]
fn transport_errors_exit_3() {
    require_e2e!();
    let out = bcp(&[
        "--server-cmd",
        "definitely-not-a-program-bcp",
        "--transport",
        "spawn",
        "version",
        "--server",
    ]);
    assert_eq!(code(&out), 3);
    assert_eq!(stderr_error(&out)["error"]["kind"], "transport");
}

#[test]
fn doctor_reports_server() {
    require_e2e!();
    let out = bcp(&["doctor"]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["ok"], true);
    assert_eq!(v["toolCount"], 14);
    assert_eq!(v["serverInfo"]["name"], "Azure.Bicep.McpServer");
}

#[test]
fn daemon_lifecycle() {
    require_e2e!();
    if std::env::var("BCP_TRANSPORT").is_ok() {
        eprintln!("skipped: BCP_TRANSPORT is set externally");
        return;
    }
    let dir = state_dir().join("daemon-test");
    let dir_s = dir.to_string_lossy().into_owned();
    let out = bcp(&["--state-dir", &dir_s, "daemon", "status"]);
    assert_eq!(stdout_json(&out)["running"], false);

    let mut serve = Command::new(env!("CARGO_BIN_EXE_bcp"))
        .args(["--state-dir", &dir_s, "serve", "--idle", "60"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..200 {
        if dir.join("daemon.json").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(
        dir.join("daemon.json").exists(),
        "daemon wrote its state file"
    );

    let out = bcp(&["--state-dir", &dir_s, "--transport", "daemon", "doctor"]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["connectedVia"], "daemon");
    assert_eq!(v["daemon"]["running"], true);

    let out = bcp(&[
        "--state-dir",
        &dir_s,
        "--transport",
        "daemon",
        "build",
        &fixture("good.bicep"),
        "--diagnostics-only",
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(stdout_json(&out)["success"], true);

    let out = bcp(&["--state-dir", &dir_s, "daemon", "stop"]);
    assert_eq!(stdout_json(&out)["stopped"], true);
    let status = serve.wait().unwrap();
    assert!(status.success());
    assert!(
        !dir.join("daemon.json").exists(),
        "state file removed on stop"
    );

    // A stale state file must not break auto mode.
    std::fs::write(
        dir.join("daemon.json"),
        r#"{"port":1,"token":"x","pid":1,"serverCmd":["x"],"cliVersion":"0"}"#,
    )
    .unwrap();
    let out = bcp(&["--state-dir", &dir_s, "version", "--server"]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        !dir.join("daemon.json").exists(),
        "stale state file removed"
    );
}

#[test]
fn skill_tools_reference_is_current() {
    require_e2e!();
    let out = bcp(&["tools", "--markdown"]);
    assert_eq!(code(&out), 0);
    let expected = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/skills/bicep/reference/tools.md"),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        expected.replace("\r\n", "\n"),
        "regenerate with: bcp tools --markdown > .github/skills/bicep/reference/tools.md"
    );
}

#[test]
fn skill_install_matches_source() {
    require_e2e!();
    let dir = state_dir().join("skills");
    let out = bcp(&["skill", "install", "--dir", &dir.to_string_lossy()]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/skills/bicep");
    for rel in [
        "SKILL.md",
        "reference/commands.md",
        "reference/tools.md",
        "reference/examples.md",
    ] {
        let a = std::fs::read_to_string(source.join(rel)).unwrap();
        let b = std::fs::read_to_string(dir.join("bicep").join(rel)).unwrap();
        assert_eq!(
            a.replace("\r\n", "\n"),
            b.replace("\r\n", "\n"),
            "{rel} differs"
        );
    }
    let out = bcp(&["skill", "print"]);
    assert_eq!(code(&out), 0);
    assert!(
        String::from_utf8_lossy(&out.stdout)
            .replace("\r\n", "\n")
            .starts_with("---\nname: bicep")
    );
    let out = bcp(&["skill", "paths"]);
    assert_eq!(code(&out), 0);
    assert!(stdout_json(&out).is_array());
}
