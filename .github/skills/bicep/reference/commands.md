# bcp command reference

`bcp` wraps the Azure Bicep MCP server (`Azure.Bicep.McpServer`). Every server tool has a
dedicated subcommand; `bcp call` reaches any tool by name.

## Output contract

- **stdout** carries the result. Default is JSON: the server's structured payload plus a few
  documented additions. Documents print as plain text: `format` prints the formatted source,
  `best-practices` prints markdown, `schema` prints the parsed JSON schema object.
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
| `skill print\|install\|paths` | – | print or install this skill (`install --global` for `~/.copilot/skills`) |
| `doctor` | – | toolchain, daemon and server report with timings |
| `version [--server]` | – | CLI (and server) version |

Global flags: `--transport auto|daemon|spawn`, `--state-dir DIR`, `--server-cmd CMD`,
`--timeout SECS` (default 120), `-f json|text`, `--text`, `--raw`, `--compact`, `-q`.

Environment: `BICEP_MCP_COMMAND` (server command, default `dotnet dnx -y Azure.Bicep.McpServer`),
`BCP_TRANSPORT`, `BCP_STATE_DIR`.

## Extension references

Extension commands take an OCI reference including a tag, for example
`br:mcr.microsoft.com/bicep/extensions/microsoftgraph/v1.0:1.0.0`. `bcp extensions` lists the
well-known extensions with their `ociReference` and `availableTags`; combine them as
`<ociReference>:<tag>`.
