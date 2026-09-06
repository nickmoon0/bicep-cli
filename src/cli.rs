//! Command-line grammar. The `--help` text is part of the product: agents
//! read it to learn the tool, so descriptions are written for them.

use crate::output::Format;
use crate::transport::TransportMode;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

const ABOUT: &str = "Bicep MCP server tools as a CLI: build, format, decompile, schemas, AVM modules, best practices.";

const LONG_ABOUT: &str = "\
bcp wraps the Azure Bicep MCP server (Azure.Bicep.McpServer) so that agents and
scripts without MCP support can use every one of its tools from a shell.

Output is JSON on stdout by default (stable, machine readable). On failure a
single JSON object {\"error\": {...}} is written to stderr and the exit code is:
  0 success   1 tool/compile error   2 usage error   3 server/transport error

File paths may be relative; they are resolved against the current directory.

Run `bcp serve` in a spare terminal to keep one server warm (calls take
~50 ms instead of ~2 s). Without it, each command starts a server itself.";

const AFTER_HELP: &str = "\
Examples:
  bcp build main.bicep                   compile, show diagnostics + ARM template
  bcp build main.bicep --template-only -o main.json
  bcp format main.bicep --write          format in place
  bcp types Microsoft.Storage --latest   newest API version per resource type
  bcp schema Microsoft.Storage/storageAccounts --latest --no-descriptions
  bcp avm --filter 'key vault'           find Azure Verified Modules
  bcp best-practices                     Bicep authoring rules (markdown)
  bcp tools --markdown                   describe every server tool

Environment: BICEP_MCP_COMMAND, BCP_TRANSPORT, BCP_STATE_DIR";

#[derive(Parser, Debug)]
#[command(name = "bcp", version, about = ABOUT, long_about = LONG_ABOUT, after_long_help = AFTER_HELP)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args, Debug, Clone)]
pub struct GlobalOpts {
    /// How to reach the server: auto (daemon if running, else spawn), daemon, spawn
    #[arg(long, global = true, env = "BCP_TRANSPORT", value_enum, default_value_t = TransportMode::Auto)]
    pub transport: TransportMode,

    /// Directory holding the daemon state file
    #[arg(long, global = true, env = "BCP_STATE_DIR", value_name = "DIR")]
    pub state_dir: Option<PathBuf>,

    /// Command used to start the server [default: dotnet dnx -y Azure.Bicep.McpServer]
    #[arg(long, global = true, env = "BICEP_MCP_COMMAND", value_name = "CMD")]
    pub server_cmd: Option<String>,

    /// Per-call timeout in seconds
    #[arg(long, global = true, default_value_t = 120, value_name = "SECS")]
    pub timeout: u64,

    /// Output format (json is the default for data, text for documents)
    #[arg(short = 'f', long, global = true, value_enum, value_name = "FORMAT")]
    pub format: Option<Format>,

    /// Shorthand for --format text
    #[arg(long, global = true)]
    pub text: bool,

    /// Print the server's raw tool result (content + structuredContent) with no post-processing
    #[arg(long, global = true)]
    pub raw: bool,

    /// Single-line JSON output
    #[arg(long, global = true)]
    pub compact: bool,

    /// Suppress warnings on stderr
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Compile a .bicep file to an ARM template and report diagnostics
    #[command(visible_alias = "b")]
    Build(BuildArgs),

    /// Compile a .bicepparam file to parameters JSON (+ its template)
    #[command(name = "build-params", visible_alias = "bp")]
    BuildParams(BuildParamsArgs),

    /// Format a .bicep or .bicepparam file (prints the result; --write to save)
    #[command(visible_alias = "fmt")]
    Format(FormatArgs),

    /// List the files a .bicep/.bicepparam file directly references
    Refs(RefsArgs),

    /// Convert an ARM template JSON file to Bicep
    Decompile(DecompileArgs),

    /// Convert an ARM parameters JSON file to .bicepparam
    #[command(name = "decompile-params")]
    DecompileParams(DecompileArgs),

    /// Predict the resources a .bicepparam deployment would create (no Azure access needed)
    Snapshot(SnapshotArgs),

    /// List resource types and API versions for a provider namespace
    #[command(name = "resource-types", visible_alias = "types")]
    ResourceTypes(ResourceTypesArgs),

    /// Get the JSON schema of an Azure resource type at an API version
    Schema(SchemaArgs),

    /// List well-known Bicep extensions (e.g. Microsoft Graph) and their versions
    Extensions(ExtensionsArgs),

    /// List resource types of a Bicep extension
    #[command(name = "ext-types")]
    ExtTypes(ExtTypesArgs),

    /// Get the JSON schema of an extension resource type
    #[command(name = "ext-schema")]
    ExtSchema(ExtSchemaArgs),

    /// Search Azure Verified Modules (official, tested Bicep modules)
    Avm(AvmArgs),

    /// Print the Bicep team's best-practices document (markdown)
    #[command(name = "best-practices", visible_alias = "bpx")]
    BestPractices,

    /// List the server's tools and their input schemas
    Tools(ToolsArgs),

    /// Call any server tool by name with JSON arguments
    Call(CallArgs),

    /// Run many tool calls from stdin (one JSON object per line) in one session
    Batch,

    /// Run the daemon in the foreground so other bcp calls reuse one warm server
    Serve(ServeArgs),

    /// Inspect or stop the daemon
    Daemon(DaemonArgs),

    /// Check the toolchain, daemon and server, and report timings
    Doctor,

    /// Print the CLI version (and the server's with --server)
    Version(VersionArgs),
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    /// Path to the .bicep file
    pub file: PathBuf,
    /// Print only the ARM template JSON
    #[arg(long, conflicts_with = "diagnostics_only")]
    pub template_only: bool,
    /// Print only success + diagnostics
    #[arg(long)]
    pub diagnostics_only: bool,
    /// Write the ARM template to this file (stdout then omits it)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct BuildParamsArgs {
    /// Path to the .bicepparam file
    pub file: PathBuf,
    /// Print only the parameters JSON
    #[arg(long, conflicts_with_all = ["template_only", "diagnostics_only"])]
    pub parameters_only: bool,
    /// Print only the ARM template JSON
    #[arg(long, conflicts_with = "diagnostics_only")]
    pub template_only: bool,
    /// Print only success + diagnostics
    #[arg(long)]
    pub diagnostics_only: bool,
    /// Write the parameters JSON to this file (stdout then omits it)
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
    /// Write the ARM template to this file (stdout then omits it)
    #[arg(long, value_name = "FILE")]
    pub template_out: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct FormatArgs {
    /// Path to the .bicep or .bicepparam file
    pub file: PathBuf,
    /// Overwrite the file with the formatted content
    #[arg(long, conflicts_with = "check")]
    pub write: bool,
    /// Exit 1 if the file is not already formatted (prints nothing else)
    #[arg(long)]
    pub check: bool,
}

#[derive(Args, Debug)]
pub struct RefsArgs {
    /// Path to the .bicep or .bicepparam file
    pub file: PathBuf,
    /// Print only a JSON array of local paths
    #[arg(long)]
    pub paths: bool,
}

#[derive(Args, Debug)]
pub struct DecompileArgs {
    /// Path to the ARM JSON file (.json, .jsonc or .arm)
    pub file: PathBuf,
    /// Write the generated file(s) to disk
    #[arg(long)]
    pub write: bool,
    /// Allow --write to overwrite existing files
    #[arg(long, requires = "write")]
    pub force: bool,
    /// Directory to write into (default: next to the input file)
    #[arg(long, value_name = "DIR", requires = "write")]
    pub out_dir: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SnapshotArgs {
    /// Path to the .bicepparam file
    pub file: PathBuf,
    #[arg(long, value_name = "ID")]
    pub tenant: Option<String>,
    #[arg(long, value_name = "ID")]
    pub management_group: Option<String>,
    #[arg(long, value_name = "ID")]
    pub subscription: Option<String>,
    #[arg(long, value_name = "NAME")]
    pub resource_group: Option<String>,
    #[arg(long, value_name = "REGION")]
    pub location: Option<String>,
    #[arg(long, value_name = "NAME")]
    pub deployment_name: Option<String>,
}

#[derive(Args, Debug)]
pub struct ResourceTypesArgs {
    /// Provider namespace, e.g. Microsoft.Storage
    pub namespace: String,
    /// Keep only the newest API version of each type (stable preferred)
    #[arg(long)]
    pub latest: bool,
    /// With --latest, let preview versions win when newer
    #[arg(long, requires = "latest")]
    pub include_preview: bool,
    /// Case-insensitive substring filter on the type name
    #[arg(long, value_name = "TEXT")]
    pub filter: Option<String>,
}

#[derive(Args, Debug)]
pub struct SchemaArgs {
    /// Resource type, optionally with version: Microsoft.KeyVault/vaults[@2024-11-01]
    pub resource_type: String,
    /// API version (alternative to the @version suffix)
    #[arg(long, value_name = "VERSION")]
    pub api_version: Option<String>,
    /// Resolve the newest API version automatically
    #[arg(long, conflicts_with = "api_version")]
    pub latest: bool,
    /// With --latest, let preview versions win when newer
    #[arg(long, requires = "latest")]
    pub include_preview: bool,
    /// Omit description fields (smaller output)
    #[arg(long)]
    pub no_descriptions: bool,
    /// Omit read-only properties (smaller output)
    #[arg(long)]
    pub no_readonly: bool,
}

#[derive(Args, Debug)]
pub struct ExtensionsArgs {
    /// Print only extension names
    #[arg(long)]
    pub names: bool,
}

#[derive(Args, Debug)]
pub struct ExtTypesArgs {
    /// OCI reference incl. tag, e.g. br:mcr.microsoft.com/bicep/extensions/microsoftgraph/v1.0:1.0.0
    pub extension: String,
    /// Keep only the newest API version of each type
    #[arg(long)]
    pub latest: bool,
    /// With --latest, let preview versions win when newer
    #[arg(long, requires = "latest")]
    pub include_preview: bool,
    /// Case-insensitive substring filter on the type name
    #[arg(long, value_name = "TEXT")]
    pub filter: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExtSchemaArgs {
    /// OCI reference incl. tag, e.g. br:mcr.microsoft.com/bicep/extensions/microsoftgraph/v1.0:1.0.0
    pub extension: String,
    /// Resource type, optionally with version: Microsoft.Graph/groups[@v1.0]
    pub resource_type: String,
    /// API version (alternative to the @version suffix)
    #[arg(long, value_name = "VERSION")]
    pub api_version: Option<String>,
    /// Resolve the newest API version automatically
    #[arg(long, conflicts_with = "api_version")]
    pub latest: bool,
    /// With --latest, let preview versions win when newer
    #[arg(long, requires = "latest")]
    pub include_preview: bool,
    /// Omit description fields (smaller output)
    #[arg(long)]
    pub no_descriptions: bool,
    /// Omit read-only properties (smaller output)
    #[arg(long)]
    pub no_readonly: bool,
}

#[derive(Args, Debug)]
pub struct AvmArgs {
    /// Case-insensitive substring filter on module path and description
    #[arg(long, value_name = "TEXT")]
    pub filter: Option<String>,
    /// Print only module paths
    #[arg(long)]
    pub names: bool,
    /// Maximum number of modules to print
    #[arg(long, value_name = "N")]
    pub limit: Option<usize>,
    /// Include every version and the documentation URI (large)
    #[arg(long)]
    pub full: bool,
}

#[derive(Args, Debug)]
pub struct ToolsArgs {
    /// Print only tool names
    #[arg(long, conflicts_with_all = ["markdown", "map"])]
    pub names: bool,
    /// Render as markdown (for writing agent instructions/skills)
    #[arg(long, conflicts_with = "map")]
    pub markdown: bool,
    /// Print the subcommand -> tool mapping
    #[arg(long)]
    pub map: bool,
}

#[derive(Args, Debug)]
pub struct CallArgs {
    /// Tool name, e.g. build_bicep
    pub tool: String,
    /// Arguments as a JSON object
    #[arg(long, value_name = "JSON")]
    pub args: Option<String>,
    /// Single argument key=value (value parsed as JSON when possible); repeatable
    #[arg(long = "arg", value_name = "KEY=VALUE")]
    pub kv: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Exit after this many seconds without client connections (0 = never)
    #[arg(long, default_value_t = 0, value_name = "SECS")]
    pub idle: u64,
}

#[derive(Args, Debug)]
pub struct DaemonArgs {
    #[command(subcommand)]
    pub action: DaemonAction,
}

#[derive(Subcommand, Debug)]
pub enum DaemonAction {
    /// Report whether the daemon is running and its statistics
    Status,
    /// Ask the daemon to shut down
    Stop,
}

#[derive(Args, Debug)]
pub struct VersionArgs {
    /// Also start/contact the server and report its version
    #[arg(long)]
    pub server: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn grammar_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_typical_invocations() {
        let cli = Cli::try_parse_from(["bcp", "build", "main.bicep", "--template-only"]).unwrap();
        assert!(matches!(cli.command, Command::Build(ref a) if a.template_only));
        let cli = Cli::try_parse_from(["bcp", "--text", "types", "Microsoft.Storage", "--latest"])
            .unwrap();
        assert!(cli.global.text);
        let cli = Cli::try_parse_from(["bcp", "call", "build_bicep", "--arg", "filePath=a.bicep"])
            .unwrap();
        assert!(matches!(cli.command, Command::Call(ref a) if a.kv.len() == 1));
        assert!(
            Cli::try_parse_from(["bcp", "schema", "X", "--latest", "--api-version", "1"]).is_err()
        );
    }
}
