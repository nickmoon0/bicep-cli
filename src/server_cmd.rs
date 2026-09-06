//! Resolution of the command used to launch the upstream Bicep MCP server.

use crate::error::CliError;

/// `dotnet dnx` is what the `dnx` shim runs on every OS; on Windows `dnx`
/// itself is a `.cmd` file that a process cannot spawn directly, so we go
/// through `dotnet` explicitly.
pub const DEFAULT_SERVER_CMD: &str = "dotnet dnx -y Azure.Bicep.McpServer";

/// Split the configured command line into program + args.
pub fn resolve(explicit: Option<&str>) -> Result<Vec<String>, CliError> {
    let raw = explicit.unwrap_or(DEFAULT_SERVER_CMD);
    let parts: Vec<String> = raw.split_whitespace().map(str::to_owned).collect();
    if parts.is_empty() {
        return Err(CliError::usage(
            "server command is empty (check --server-cmd / BICEP_MCP_COMMAND)",
        ));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_dotnet_dnx() {
        let parts = resolve(None).unwrap();
        assert_eq!(parts, ["dotnet", "dnx", "-y", "Azure.Bicep.McpServer"]);
    }

    #[test]
    fn explicit_is_split_on_whitespace() {
        let parts = resolve(Some("  dotnet   C:\\x\\Azure.Bicep.McpServer.dll ")).unwrap();
        assert_eq!(parts, ["dotnet", "C:\\x\\Azure.Bicep.McpServer.dll"]);
    }

    #[test]
    fn empty_is_usage_error() {
        assert_eq!(resolve(Some("   ")).unwrap_err().exit_code(), 2);
    }
}
