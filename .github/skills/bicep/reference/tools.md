## `build_bicep` — Build Bicep

CLI: `bcp build` (or `bcp call build_bicep`)

Compiles a Bicep file (.bicep) to an ARM template JSON string and returns the result along with any diagnostics.

Use this tool to:
- Compile Bicep source code to ARM template JSON
- Check for compilation errors before deployment
- Obtain the ARM template output for inspection or deployment

The compiled ARM template JSON is returned along with any compilation diagnostics (errors, warnings, and informational messages).
The file path must be absolute. If compilation fails due to errors, the Template field will be null.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `filePath` | string | yes | The path to the .bicep file |

## `build_bicepparam` — Build Bicep Parameters

CLI: `bcp build-params` (or `bcp call build_bicepparam`)

Compiles a Bicep parameters file (.bicepparam) to a parameters JSON string and returns the result along with any diagnostics.

Use this tool to:
- Compile Bicep parameters source code to ARM parameters JSON
- Check for compilation errors before deployment
- Obtain the parameters JSON output for inspection or deployment

The compiled parameters JSON and the associated ARM template JSON are returned along with any compilation diagnostics (errors, warnings, and informational messages).
The file path must be absolute. If compilation fails due to errors, the Parameters field will be null.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `filePath` | string | yes | The path to the .bicepparam file |

## `decompile_arm_parameters_file` — Decompile ARM parameters file

CLI: `bcp decompile-params` (or `bcp call decompile_arm_parameters_file`)

Converts an ARM template parameters JSON file into Bicep parameters syntax (.bicepparam).

Use this tool to:
- Migrate ARM JSON parameter files to Bicep parameters format
- Convert deployment parameter files when modernizing to Bicep
- Generate .bicepparam files from existing ARM deployments

Accepts files with .json, .jsonc, or .arm extensions. The file path must be absolute.

The generated .bicepparam file includes a 'using' statement placeholder that must be completed, all parameters with their values preserved, and KeyVault references converted to az.getSecret() function calls if present.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `filePath` | string | yes | The path to the .json ARM parameters file |

## `decompile_arm_template_file` — Decompile ARM template file

CLI: `bcp decompile` (or `bcp call decompile_arm_template_file`)

Converts an Azure Resource Manager (ARM) template JSON file into modern Bicep syntax (.bicep).

Use this tool to:
- Migrate existing ARM JSON templates to the more readable and maintainable Bicep language
- Learn Bicep syntax by seeing how JSON templates translate to Bicep
- Modernize legacy infrastructure-as-code

Accepts files with .json, .jsonc, or .arm extensions. The file path must be absolute.
The result includes the entrypoint URI and all generated Bicep files (which may include additional modules for nested/linked templates).

Note: Decompilation is a best-effort process. Some ARM template features may require manual adjustment in the generated Bicep code. Review the output for any TODO comments or warnings.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `filePath` | string | yes | The path to the .json ARM template file |

## `format_bicep_file` — Format Bicep File

CLI: `bcp format` (or `bcp call format_bicep_file`)

Formats a Bicep file (.bicep) or Bicep parameters file (.bicepparam) according to official Bicep formatting standards.

Use this tool to:
- Apply consistent code formatting (indentation, spacing, line breaks) to Bicep files
- Clean up manually edited or generated Bicep code before saving
- Ensure code follows team formatting conventions

The formatter respects configuration settings from bicepconfig.json if present in the file's directory hierarchy, including:
- Indentation style (spaces vs tabs) and size
- Newline character (LF, CRLF, CR)
- Whether to insert a final newline

The file path must be absolute. Formatting preserves semantic meaning and only changes whitespace and layout. Files with syntax errors will still be formatted to the extent possible.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `filePath` | string | yes | The path to the .bicep or .bicepparam file |

## `get_azure_resource_type_schema` — Get Azure resource type schema

CLI: `bcp schema` (or `bcp call get_azure_resource_type_schema`)

Retrieves the complete JSON schema definition for a specific Azure resource type and API version, including all properties, nested types, and constraints.
Use this tool to:
- Understand what properties are available on an Azure resource
- Learn about required vs optional properties, their types, and allowed values
- Discover nested resource types and their schemas
- Find available resource functions and their signatures
- Generate accurate Bicep code with proper property names and types
The returned JSON schema includes resource type definitions, nested complex types, resource function signatures (like list* operations), and property constraints.
Data is sourced directly from Azure Resource Provider APIs, ensuring the most accurate and up-to-date schema information.
Specify the resource type (e.g., Microsoft.KeyVault/vaults) and API version (e.g., 2024-11-01 or 2024-12-01-preview).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `apiVersion` | string | yes | The API version of the resource type; e.g. 2024-11-01 or 2024-12-01-preview |
| `excludeDescriptions` | boolean | no | When true, omits description fields from the schema to reduce payload size. Default: false |
| `excludeReadOnlyProperties` | boolean | no | When true, omits read-only properties from the schema to reduce payload size. Default: false |
| `resourceType` | string | yes | The resource type of the Azure resource; e.g. Microsoft.KeyVault/vaults |

## `get_bicep_best_practices` — Get Bicep best-practices

CLI: `bcp best-practices` (or `bcp call get_bicep_best_practices`)

Retrieves comprehensive, up-to-date best practices and coding standards for authoring Bicep templates.
Use this tool when:
- Generating new Bicep code to ensure it follows current best practices
- Reviewing existing Bicep code for quality improvements
- Learning recommended patterns for common scenarios
- Understanding security, maintainability, and reliability guidelines
Covers naming conventions, code organization, parameter usage, resource declarations, module composition, security recommendations, performance optimization, and testing approaches.
The practices are maintained by the Bicep team and reflect current recommended approaches.

No parameters.

## `get_deployment_snapshot` — Get deployment snapshot

CLI: `bcp snapshot` (or `bcp call get_deployment_snapshot`)

Creates a deployment snapshot from a Bicep parameters file (.bicepparam) by compiling it and pre-expanding the resulting ARM template.
The snapshot contains the predicted resources (as they would appear in a deployment) and any diagnostics produced during preflight.

Use this tool to:
- Preview what resources a deployment would create without running a deployment
- Validate that parameters resolve as expected
- Troubleshoot why a deployment would produce unexpected resource IDs, names, or locations
- Inspect preflight diagnostics produced by template expansion

This tool can also be used to perform a semantic diff between two Bicep implementations:
- Generate a snapshot for each version (using the same parameter values and metadata)
- Compare the resulting predicted resources and diagnostics to verify both produce the same deployment outcome

This is especially useful for automated refactoring, where text-level diffs are noisy but the intended deployment result should remain unchanged.

The file path must be absolute and must point to a .bicepparam file.
The optional tenant/subscription/resource group/location/deployment name values are used as deployment metadata during snapshot generation.
If a value is omitted, the snapshot may contain unresolved placeholder expressions for the corresponding metadata.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `deploymentName` | string \| null | no | Optional deployment name to use as deployment metadata |
| `filePath` | string | yes | The absolute path to the .bicepparam file |
| `location` | string \| null | no | Optional Azure location to use as deployment metadata |
| `managementGroupId` | string \| null | no | Optional Azure management group ID to use as deployment metadata |
| `resourceGroup` | string \| null | no | Optional Azure resource group name to use as deployment metadata |
| `subscriptionId` | string \| null | no | Optional Azure subscription ID to use as deployment metadata |
| `tenantId` | string \| null | no | Optional Azure tenant ID to use as deployment metadata |

## `get_extension_resource_type_schema` — Get extension resource type schema

CLI: `bcp ext-schema` (or `bcp call get_extension_resource_type_schema`)

Retrieves the complete JSON schema definition for a specific extension resource type and API version, including all properties, nested types, and constraints.
Use this tool to:
- Understand what properties are available on an extension resource
- Learn about required vs optional properties, their types, and allowed values
- Generate accurate Bicep code with proper property names and types
The extensionReference must be a canonical OCI artifact reference in the format "br:<registry>/<repository>:<tag>"
(e.g., "br:mcr.microsoft.com/bicep/extensions/microsoftgraph/v1.0:1.0.0").
Use list_well_known_extensions to discover available extensions and their versions.
Specify the resource type (e.g., Microsoft.Graph/applications) and API version (e.g., v1.0 or beta).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `apiVersion` | string | yes | The API version of the resource type; e.g. v1.0 or beta |
| `excludeDescriptions` | boolean | no | When true, omits description fields from the schema to reduce payload size. Default: false |
| `excludeReadOnlyProperties` | boolean | no | When true, omits read-only properties from the schema to reduce payload size. Default: false |
| `extensionReference` | string | yes | The OCI artifact reference for the extension; e.g. br:mcr.microsoft.com/bicep/extensions/microsoftgraph/v1.0:1.0.0 |
| `resourceType` | string | yes | The resource type; e.g. Microsoft.Graph/applications |

## `get_file_references` — Get File References

CLI: `bcp refs` (or `bcp call get_file_references`)

Analyzes a Bicep file (.bicep) or Bicep parameters file (.bicepparam) and returns a list of all files referenced by the entry file, including modules, parameter files, and any other dependencies.

Use this tool to:
- Identify all files that a Bicep or Bicep parameters file depends on
- Perform dependency or impact analysis before making changes
- Understand the structure and external links of a Bicep deployment

The result is a list of absolute URIs for each referenced file. The file path must be absolute. Only direct references are included; transitive references require additional calls.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `filePath` | string | yes | The path to the .bicep or .bicepparam file |

## `list_avm_metadata` — List Azure Verified Modules (AVM)

CLI: `bcp avm` (or `bcp call list_avm_metadata`)

Lists metadata for all Azure Verified Modules (AVM) - Microsoft's official, pre-built, tested, and maintained Bicep modules for common Azure resource patterns.
Use this tool to:
- Discover reusable, production-ready Bicep modules for common scenarios
- Find officially supported modules instead of writing resources from scratch
- Check available versions and documentation for AVM modules
- Accelerate Bicep development by leveraging tested, best-practice implementations
Azure Verified Modules provide:
- Pre-configured resource deployments following Microsoft best practices
- Built-in security, reliability, and compliance features
- Regular updates and maintenance by Microsoft
- Comprehensive documentation and examples
Use these modules in your Bicep files to reduce code and improve quality.

No parameters.

## `list_azure_resource_types` — List available Azure resource types

CLI: `bcp resource-types` (or `bcp call list_azure_resource_types`)

Lists all available Azure resource types and their API versions for a specific Azure resource provider namespace.
Use this tool to:
- Discover what resource types are available in a provider (e.g., what can be created under Microsoft.Storage)
- Find the latest API versions for Azure resources
- Explore the complete resource type catalog for a given provider
Data is sourced directly from Azure Resource Provider APIs, ensuring accuracy and currency.
Example provider namespaces: Microsoft.Compute, Microsoft.Storage, Microsoft.Network, Microsoft.Web, Microsoft.KeyVault

| Parameter | Type | Required | Description |
|---|---|---|---|
| `providerNamespace` | string | yes | The resource provider (or namespace) of the Azure resource; e.g. Microsoft.KeyVault |

## `list_extension_resource_types` — List extension resource types

CLI: `bcp ext-types` (or `bcp call list_extension_resource_types`)

Lists all available resource types and their API versions for a Bicep extension.
Use this tool to:
- Discover what resource types are available in an extension (e.g., Microsoft Graph)
- Find the API versions for extension resources
The extensionReference must be a canonical OCI artifact reference in the format "br:<registry>/<repository>:<tag>"
(e.g., "br:mcr.microsoft.com/bicep/extensions/microsoftgraph/v1.0:1.0.0").
Use list_well_known_extensions to discover available extensions and their versions.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `extensionReference` | string | yes | The OCI artifact reference for the extension; e.g. br:mcr.microsoft.com/bicep/extensions/microsoftgraph/v1.0:1.0.0 |

## `list_well_known_extensions` — List well-known Bicep extensions

CLI: `bcp extensions` (or `bcp call list_well_known_extensions`)

Lists well-known Bicep extensions (e.g., Microsoft Graph) with their available versions.
This is not an exhaustive list of all possible extensions; other extensions may exist beyond what is listed here.
Use this tool to:
- Discover available Bicep extensions beyond Azure (az) resources
- Find available versions for Microsoft Graph and other extensions
- Get extension names and versions to use with list_extension_resource_types and get_extension_resource_type_schema
Extensions provide resource types for non-Azure providers like Microsoft Graph.
Use the returned extension name and version with other tools to explore extension resource types.

No parameters.

