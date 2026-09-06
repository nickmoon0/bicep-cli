---
name: bicep
description: Author, validate, format and migrate Azure Bicep infrastructure using the bcp CLI (a wrapper around the Bicep MCP server). Use whenever creating or editing .bicep or .bicepparam files, converting ARM JSON templates to Bicep, looking up Azure resource types, API versions or property schemas, choosing Azure Verified Modules (AVM), previewing what a deployment would create, or checking Bicep best practices.
license: MIT
---

# Bicep with `bcp`

`bcp` exposes every tool of the official Bicep MCP server as a shell command. Use it instead of
guessing: it compiles, formats and decompiles files, and it returns the exact resource schemas
that the Azure resource providers publish.

## Setup check (once per session)

```sh
bcp doctor
```

- Exit 0: ready. `connectedVia` says whether a warm daemon (`bcp serve`) or a one-shot server
  was used. Either works; the daemon just makes calls faster.
- Exit 3: `bcp` cannot start the server (usually the .NET 10 SDK is missing). Tell the user
  and stop; do not write Bicep from memory.

## Rules

1. **Read the best practices first.** Run `bcp best-practices` before writing or reviewing
   Bicep and follow it (it covers naming, parameters, user-defined types, modules, security).
2. **Never guess resource types, API versions or property names.** Look them up:
   `bcp types <Provider.Namespace> --latest` for types and versions, then
   `bcp schema <type> --latest --no-descriptions` for the properties and `required` list.
3. **Prefer Azure Verified Modules** for common resources: `bcp avm --filter <text>`, then
   reference the module as `br/public:<modulePath>:<latestVersion>`.
4. **Build after every edit** with `bcp build <file> --diagnostics-only` and keep going until
   `success` is `true`. Fix errors; address or justify warnings.
5. **Format before finishing**: `bcp format <file> --write`.
6. **Read the exit code and stderr.** `0` ok, `1` the tool reported a problem (compile errors,
   unformatted file, unknown type), `2` your invocation was wrong, `3` the server is
   unreachable. On failure stderr holds one JSON object: `{"error":{"kind","message",...}}`.
7. **Keep output small.** Use `--compact`, `--diagnostics-only`, `--template-only`, `--names`,
   `--filter` and `--no-descriptions`. Paths may be relative to the current directory.

## Workflows

### A. Create or change a resource

```sh
bcp best-practices                                   # once
bcp avm --filter 'key vault' --compact               # is there a verified module?
bcp types Microsoft.KeyVault --latest --compact      # exact type@apiVersion
bcp schema Microsoft.KeyVault/vaults --latest --no-descriptions --compact
# write or edit main.bicep
bcp build main.bicep --diagnostics-only --compact    # repeat until success: true
bcp format main.bicep --write
```

### B. Parameters files and deployment preview

```sh
bcp build-params main.bicepparam --diagnostics-only --compact
bcp snapshot main.bicepparam --subscription <id> --resource-group <rg> --location <region>
```

`snapshot` returns `predictedResources` (ids, names, locations, properties as they would be
deployed) and `outputs` without touching Azure. Use it to confirm names and IDs resolve as the
user expects.

### C. Migrate an ARM JSON template

```sh
bcp decompile template.json --write            # writes template.bicep next to it (--out-dir DIR, --force)
bcp decompile-params parameters.json --write   # writes .bicepparam; complete the `using` line
bcp build template.bicep --diagnostics-only
bcp format template.bicep --write
```

Decompiled code is best-effort: search it for `TODO` comments and fix them.

### D. Prove a refactor is behaviour-preserving

Take a snapshot before and after with identical metadata and compare `predictedResources` and
`outputs`. Text diffs of Bicep are noisy; the snapshot diff is the real answer.

```sh
bcp snapshot main.bicepparam --subscription <id> --resource-group <rg> --location <region> > before.json
# refactor
bcp snapshot main.bicepparam --subscription <id> --resource-group <rg> --location <region> > after.json
```

## Reading results

- `build` / `build-params`: `success` (bool), `template` / `parameters` (JSON as a string, or
  null on failure), `diagnostics[]` with `level` (`Error` | `Warning` | `Info`), `code`,
  `message`, `path`, `line`, `column` (1-based) and `documentationUri`. Exit 1 when
  `success` is false, but the diagnostics are still printed on stdout.
- `format`: prints the formatted source. `--check` exits 1 if the file would change,
  `--write` saves it in place.
- `schema`: a JSON Schema object. Start from `required` and `properties`; nested types are in
  `$defs`; `x-bicep-resource-functions` lists functions such as `listKeys`.
- `types` / `ext-types`: `resourceTypes[]` as `Type@apiVersion`, newest first per type.
- `avm`: `modules[]` with `modulePath`, `latestVersion`, `description` (add `--full` for all
  versions and docs links).

## Cheat sheet

| Task | Command |
|---|---|
| Best practices | `bcp best-practices` |
| Compile, errors only | `bcp build main.bicep --diagnostics-only --compact` |
| ARM template to a file | `bcp build main.bicep --template-only -o main.json` |
| Compile parameters | `bcp build-params main.bicepparam --compact` |
| Format in place / check | `bcp format main.bicep --write` / `bcp format main.bicep --check` |
| Files a template uses | `bcp refs main.bicep --paths` |
| Types + versions | `bcp types Microsoft.Storage --latest --filter storageAccounts` |
| Property schema | `bcp schema Microsoft.Storage/storageAccounts --latest --no-descriptions` |
| Verified modules | `bcp avm --filter storage --compact` |
| Microsoft Graph types | `bcp extensions` then `bcp ext-types <ref>` / `bcp ext-schema <ref> <type> --latest` |
| Preview deployment | `bcp snapshot main.bicepparam --subscription … --resource-group … --location …` |
| Any other server tool | `bcp tools --names`, `bcp call <tool> --arg key=value` |

## Pitfalls

- `--latest` prefers stable API versions; add `--include-preview` when a preview is required.
- A missing file exits 2 immediately; check the path before retrying.
- On Windows shells, prefer `--arg key=value` over `--args '{...}'` to avoid JSON quoting issues.
- `bcp serve` may be running in another terminal; never start or stop it unless asked.

## More detail

- `reference/commands.md` — every command, flag, and the output contract.
- `reference/tools.md` — the server's own description of each tool and its parameters.
- `reference/examples.md` — real invocations with their exact output.
