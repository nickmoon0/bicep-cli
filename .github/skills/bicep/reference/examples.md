# bcp examples with real output

Paths are shortened; `bcp` prints absolute paths.

## A build with errors (exit 1)

```sh
bcp build bad.bicep --diagnostics-only
```

stdout:

```json
{
  "diagnostics": [
    {
      "code": "BCP009",
      "column": 18,
      "documentationUri": "https://aka.ms/bicep/core-diagnostics#BCP009",
      "fileUri": "file:///work/bad.bicep",
      "length": 0,
      "level": "Error",
      "line": 1,
      "message": "Expected a literal value, an array, an object, a parenthesized expression, or a function call at this location.",
      "path": "/work/bad.bicep",
      "position": 17
    },
    {
      "code": "BCP035",
      "column": 10,
      "documentationUri": "https://aka.ms/bicep/core-diagnostics#BCP035",
      "fileUri": "file:///work/bad.bicep",
      "length": 2,
      "level": "Warning",
      "line": 2,
      "message": "The specified \"resource\" declaration is missing the following required properties: \"kind\", \"location\", \"sku\".",
      "path": "/work/bad.bicep",
      "position": 60
    }
  ],
  "success": false
}
```

stderr:

```json
{"error":{"exitCode":1,"kind":"tool","message":"build failed: 1 error(s), 3 warning(s)","tool":"build_bicep"}}
```

A clean build prints `{"diagnostics":[],"success":true}` and exits 0.

## A missing file (exit 2, no server started)

```sh
bcp build missing.bicep
```

```json
{"error":{"exitCode":2,"kind":"usage","message":"file not found: /work/missing.bicep"}}
```

## Newest API version of a type

```sh
bcp types Microsoft.Storage --latest --filter "storageAccounts@" --compact
```

```json
{"resourceTypes":["Microsoft.Storage/storageAccounts@2026-04-01"]}
```

## Property schema (trimmed)

```sh
bcp schema Microsoft.Storage/storageAccounts --latest --no-descriptions --no-readonly --compact
```

Top-level keys: `title` (`Microsoft.Storage/storageAccounts@2026-04-01`), `required`
(`["kind","location","name","sku"]`), `properties` (`extendedLocation`, `identity`, `kind`,
`location`, `name`, `placement`, `properties`, `sku`, `tags`, `zones`), `$defs` (38 nested
types such as `StorageAccountPropertiesCreateParametersOrStorageAccountProperties`),
`x-bicep-resource-functions` (e.g. `listKeys`, `listAccountSas`). Follow `$ref`s from
`properties.properties` into `$defs` to find allowed values (`enum`) and nested objects.

## Azure Verified Modules

```sh
bcp avm --filter storage-account --compact
```

```json
{"count":16,"hint":"use --full for all versions and documentation URIs; use br/public:<modulePath>:<version> in Bicep",
 "modules":[{"description":"This module deploys a Storage Account.","latestVersion":"0.33.0","modulePath":"avm/res/storage/storage-account"}, "…"]}
```

Use in Bicep:

```bicep
module storage 'br/public:avm/res/storage/storage-account:0.33.0' = {
  params: {
    name: 'stexample001'
  }
}
```

## Deployment preview

```sh
bcp snapshot main.bicepparam --subscription 00000000-0000-0000-0000-000000000000 --resource-group rg-demo --location westeurope
```

```json
{
  "diagnostics": [],
  "outputs": {
    "id": "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg-demo/providers/Microsoft.Storage/storageAccounts/stbcptest001"
  },
  "predictedResources": [
    {
      "apiVersion": "2023-05-01",
      "id": "/subscriptions/00000000-0000-0000-0000-000000000000/resourceGroups/rg-demo/providers/Microsoft.Storage/storageAccounts/stbcptest001",
      "kind": "StorageV2",
      "location": "westeurope",
      "name": "stbcptest001",
      "sku": { "name": "Standard_LRS" },
      "type": "Microsoft.Storage/storageAccounts"
    }
  ]
}
```

## Format check (exit 1 when unformatted)

```sh
bcp format unformatted.bicep --check --compact
```

stdout `{"changed":true,"path":"/work/unformatted.bicep"}`, stderr
`{"error":{"exitCode":1,"kind":"tool","message":"/work/unformatted.bicep is not formatted","tool":"format_bicep_file"}}`.
`bcp format unformatted.bicep --write` fixes it and prints `{"changed":true,"path":…,"written":true}`.

## Decompile an ARM template

```sh
bcp decompile template.json --compact
```

```json
{"entrypoint":"/work/template.bicep","files":{"/work/template.bicep":"param loc string = resourceGroup().location\n\nresource stdemo123 'Microsoft.Storage/storageAccounts@2023-05-01' = {\n  name: 'stdemo123'\n  location: loc\n  sku: {\n    name: 'Standard_LRS'\n  }\n  kind: 'StorageV2'\n}\n"}}
```

Add `--write` to save the files (`--out-dir DIR` to choose where, `--force` to overwrite).

## Referenced files

```sh
bcp refs main.bicep --compact
```

```json
{"fileUris":["file:///work/main.bicep","file:///work/mod.bicep"],"paths":["/work/main.bicep","/work/mod.bicep"]}
```
