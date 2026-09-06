# bcp — Bicep MCP server tools as a CLI

`bcp` wraps the [Azure Bicep MCP server](https://github.com/Azure/bicep/tree/main/src/Bicep.McpServer)
(`Azure.Bicep.McpServer`) so that agents and scripts that cannot use MCP — for
example GitHub Copilot in organisations where MCP is disabled — still get every
one of its tools from a shell. It is cross-platform (Linux, macOS, Windows) and
has full parity with the server's 14 tools.

```text
bcp build main.bicep                 compile, diagnostics + ARM template
bcp format main.bicep --write        format in place
bcp types Microsoft.Storage --latest newest API version per resource type
bcp schema Microsoft.Storage/storageAccounts --latest --no-descriptions
bcp avm --filter 'key vault'         find Azure Verified Modules
bcp best-practices                   the Bicep team's authoring rules (markdown)
```

## Requirements

- [.NET 10 SDK](https://dotnet.microsoft.com/download) — provides `dotnet dnx`, which
  downloads and runs `Azure.Bicep.McpServer` from NuGet on first use.
- Rust toolchain (stable) to build.

## Install

```sh
cargo install --path .
```

Or build with `cargo build --release` and put `target/release/bcp` (or `bcp.exe`) on `PATH`.

Check everything is wired up:

```sh
bcp doctor
```

## How it talks to the server

The server is stdio-only, so something has to own its stdin. `bcp` supports two modes and
picks automatically (`--transport auto`, the default):

1. **Daemon** — run `bcp serve` in a spare terminal (or as a background task). It starts one
   server and proxies MCP over a loopback TCP port protected by a random token stored in a
   state file. Every other `bcp` call then completes in tens of milliseconds.
2. **Spawn** — if no daemon is running, each command starts its own server, does its work and
   exits. That costs roughly two seconds per call but needs no setup.

```sh
bcp serve --idle 1800     # stop after 30 min without clients (0 = never)
bcp daemon status
bcp daemon stop
```

State file locations: `%LOCALAPPDATA%\bcp\daemon.json` on Windows,
`$XDG_RUNTIME_DIR/bcp/daemon.json` (else `~/.cache/bcp/`) on Unix. Override with
`--state-dir` or `BCP_STATE_DIR`. A stale state file (daemon crashed) is removed automatically.

## Output contract

- **stdout** carries the result. Default is JSON: the server's `structuredContent`, plus a few
  documented additions (below). Documents print as plain text: `format` prints the formatted
  source, `best-practices` prints markdown, `schema` prints the parsed JSON schema object.
- `--text` / `-f text` gives a human-readable rendering; `-f json` forces JSON everywhere.
- `--compact` prints single-line JSON. `--raw` prints the server's exact tool result
  (`content` + `structuredContent` + `isError`) with no post-processing.
- **stderr** gets exactly one JSON object on failure:
  `{"error":{"kind":"usage|tool|transport","message":"…","tool":"…","exitCode":n}}`.
- **Exit codes**: `0` success · `1` tool error (compile errors, `format --check` differences,
  unknown resource type…) · `2` usage error (bad flags, missing file) · `3` server/transport
  error (cannot start server, daemon unreachable in `--transport daemon`, timeout).
- Paths may be relative; they are resolved against the current directory and must exist. A
  missing file fails in milliseconds without contacting the server.

Additions to the server payload: `build`/`build-params` diagnostics gain `path`, `line`,
`column` (1-based); `refs` gains `paths`; `decompile` reports native paths instead of URIs;
list commands are sorted and can be filtered; `avm` is summarised unless `--full`.

## Commands

| Command (alias) | Server tool | Notes |
|---|---|---|
| `build <file>` (`b`) | `build_bicep` | `--template-only`, `--diagnostics-only`, `-o FILE` writes the template |
| `build-params <file>` (`bp`) | `build_bicepparam` | `--parameters-only`, `--template-only`, `-o FILE`, `--template-out FILE` |
| `format <file>` (`fmt`) | `format_bicep_file` | prints formatted source; `--write` saves, `--check` exits 1 if unformatted |
| `refs <file>` | `get_file_references` | `--paths` prints only a JSON array of paths |
| `decompile <file>` | `decompile_arm_template_file` | `--write [--out-dir DIR] [--force]` |
| `decompile-params <file>` | `decompile_arm_parameters_file` | same flags |
| `snapshot <file.bicepparam>` | `get_deployment_snapshot` | `--subscription`, `--resource-group`, `--location`, `--tenant`, `--management-group`, `--deployment-name` |
| `resource-types <ns>` (`types`) | `list_azure_resource_types` | `--latest [--include-preview]`, `--filter TEXT` |
| `schema <type[@ver]>` | `get_azure_resource_type_schema` | `--latest`, `--api-version`, `--no-descriptions`, `--no-readonly` |
| `extensions` | `list_well_known_extensions` | `--names` |
| `ext-types <ref>` | `list_extension_resource_types` | `--latest`, `--filter` |
| `ext-schema <ref> <type[@ver]>` | `get_extension_resource_type_schema` | as `schema` |
| `avm` | `list_avm_metadata` | `--filter TEXT`, `--names`, `--limit N`, `--full` |
| `best-practices` (`bpx`) | `get_bicep_best_practices` | markdown on stdout |
| `tools` | `tools/list` | `--names`, `--markdown` (for writing agent instructions), `--map` |
| `call <tool>` | any | `--args '{json}'` and/or `--arg key=value`; `filePath` is made absolute |
| `batch` | many | stdin: one `{"tool":…,"args":{…}}` per line → one result line each, in one session |
| `serve` | – | run the daemon (`--idle SECS`) |
| `daemon status\|stop` | – | inspect / stop the daemon |
| `skill print\|install\|paths` | – | print or install the Copilot skill (`install --global` for `~/.copilot/skills`) |
| `doctor` | – | toolchain, daemon and server report with timings |
| `version [--server]` | – | CLI (and server) version |

Global flags: `--transport auto|daemon|spawn`, `--state-dir DIR`, `--server-cmd CMD`,
`--timeout SECS` (default 120), `-f json|text`, `--text`, `--raw`, `--compact`, `-q`.

Environment: `BICEP_MCP_COMMAND` (server command, default `dotnet dnx -y Azure.Bicep.McpServer`),
`BCP_TRANSPORT`, `BCP_STATE_DIR`.

To skip `dnx` resolution or pin a version, point the server command at the DLL directly, e.g.
`--server-cmd "dotnet C:\Users\me\.nuget\packages\azure.bicep.mcpserver\0.46.1\tools\net10.0\any\Azure.Bicep.McpServer.dll"`.

## Using bcp from an agent

The intended workflow for an agent writing Bicep, mirroring the MCP server's own guidance:

1. `bcp best-practices` once, and keep the rules in context.
2. `bcp types <Namespace> --latest` then `bcp schema <type> --latest --no-descriptions` to get
   exact property names before writing a resource.
3. `bcp avm --filter <text>` to prefer an Azure Verified Module over hand-written resources.
4. After editing: `bcp build main.bicep --diagnostics-only`, fix until `success` is `true`,
   then `bcp format main.bicep --write`.
5. `bcp snapshot main.bicepparam --subscription … --resource-group … --location …` to preview
   the resources a deployment would create.

`bcp tools --markdown` prints every tool's description and parameters as markdown, ready to
paste into an instructions or skill file. `bcp --help` and `bcp <command> --help` are written
to be read by agents too.

## GitHub Copilot skill

The repository ships an [Agent Skill](https://docs.github.com/en/copilot/concepts/agents/about-agent-skills)
at `.github/skills/bicep/` that teaches Copilot (CLI, VS Code, JetBrains, cloud agent) when and
how to use `bcp`: read best practices first, look up types and schemas instead of guessing,
prefer Azure Verified Modules, build after every edit, format before finishing. The same files
are embedded in the binary, so one `bcp` install carries its own instructions:

```sh
bcp skill install --global      # ~/.copilot/skills/bicep — applies to every repository on this machine
bcp skill install               # ./.github/skills/bicep — commit it with an infrastructure repo
bcp skill install --dir DIR     # any other skills directory
bcp skill paths                 # where Copilot looks, and whether the skill there is current
bcp skill print [reference/commands.md]
```

Copilot loads `SKILL.md` when a task matches its description (Bicep, `.bicepparam`, ARM
templates, resource schemas, AVM). The `reference/` files are read on demand. To make Copilot
lean on it harder in a repository, add to `.github/copilot-instructions.md`:

```markdown
For any Bicep or ARM template work use the `bicep` skill and the `bcp` CLI.
Never guess resource property names or API versions; look them up with `bcp schema` / `bcp types`.
```

`reference/tools.md` is generated from the server: after a server update run
`bcp tools --markdown > .github/skills/bicep/reference/tools.md` (an e2e test checks it is current).

## Development

```sh
cargo test                                   # unit tests, no server needed
BCP_E2E=1 cargo test --test e2e -- --test-threads=1          # against the real server (spawn mode)
bcp serve &  BCP_E2E=1 cargo test --test e2e -- --test-threads=1  # same, through the daemon
BCP_E2E=1 BCP_E2E_NETWORK=1 cargo test --test e2e -- --test-threads=1   # incl. AVM/extension lookups
cargo clippy --all-targets -- -D warnings && cargo fmt --check
cargo check --target x86_64-pc-windows-msvc  # after `rustup target add x86_64-pc-windows-msvc`
```

CI runs lint, unit and end-to-end tests on Ubuntu and Windows.

### Dependencies

Direct: `rmcp` (official Rust MCP SDK), `tokio`, `clap`, `serde`, `serde_json`, `anyhow`.
`cargo tree --target all` lists the full resolved set; there are no Unix-only direct
dependencies. Daemon transport uses plain loopback TCP so no OS-specific socket code is needed.

## License

MIT
