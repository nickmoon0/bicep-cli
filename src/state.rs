//! Daemon state file: where it lives per OS, and how the token is made.

use serde::{Deserialize, Serialize};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};

pub const STATE_FILE_NAME: &str = "daemon.json";

/// Everything a client needs to reach a running `bcp serve`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonState {
    pub port: u16,
    pub token: String,
    pub pid: u32,
    pub server_cmd: Vec<String>,
    pub cli_version: String,
}

/// Directory holding the state file.
///
/// Windows: `%LOCALAPPDATA%\bcp`. Unix: `$XDG_RUNTIME_DIR/bcp`, else
/// `$HOME/.cache/bcp`. Last resort on any OS: the system temp dir.
pub fn state_dir(explicit: Option<&Path>) -> PathBuf {
    if let Some(dir) = explicit {
        return dir.to_path_buf();
    }
    let from_env = |var: &str| {
        std::env::var_os(var)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        from_env("LOCALAPPDATA")
    } else {
        from_env("XDG_RUNTIME_DIR").or_else(|| from_env("HOME").map(|h| h.join(".cache")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("bcp")
}

pub fn state_file(dir: &Path) -> PathBuf {
    dir.join(STATE_FILE_NAME)
}

pub fn read(dir: &Path) -> Option<DaemonState> {
    let text = std::fs::read_to_string(state_file(dir)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write(dir: &Path, state: &DaemonState) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = state_file(dir);
    let text = serde_json::to_string_pretty(state).expect("state serializes");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    std::io::Write::write_all(&mut file, text.as_bytes())
}

pub fn remove(dir: &Path) {
    let _ = std::fs::remove_file(state_file(dir));
}

/// 64 hex chars from the std hasher's per-process random keys mixed with
/// the pid and clock. Adequate for authenticating loopback connections on a
/// single-user machine; no extra crate needed.
pub fn new_token() -> String {
    let mut out = String::with_capacity(64);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    for round in 0..4u64 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u128(now);
        hasher.write_u32(std::process::id());
        hasher.write_u64(round);
        let h = hasher.finish();
        out.push_str(&format!("{h:016x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bcp-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn round_trip() {
        let dir = temp_dir("state");
        let state = DaemonState {
            port: 4242,
            token: new_token(),
            pid: 1,
            server_cmd: vec!["dotnet".into(), "x.dll".into()],
            cli_version: "0.1.0".into(),
        };
        write(&dir, &state).unwrap();
        assert_eq!(read(&dir).unwrap(), state);
        remove(&dir);
        assert!(read(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn token_is_64_hex_and_unique() {
        let a = new_token();
        let b = new_token();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn explicit_dir_wins() {
        assert_eq!(state_dir(Some(Path::new("/x"))), PathBuf::from("/x"));
        assert!(state_dir(None).ends_with("bcp"));
    }
}
