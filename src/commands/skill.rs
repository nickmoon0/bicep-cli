//! The GitHub Copilot skill (`.github/skills/bicep`) embedded in the binary,
//! so `bcp skill install` can drop it onto any machine.

use crate::cli::SkillAction;
use crate::error::CliError;
use crate::output::{Format, Outcome};
use crate::tools::{absolute, path_string};
use crate::transport::Ctx;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const SKILL_NAME: &str = "bicep";

/// Relative path inside the skill folder -> content. The source of truth is
/// the folder in the repository; this table just mirrors it.
pub const SKILL_FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../.github/skills/bicep/SKILL.md"),
    ),
    (
        "reference/commands.md",
        include_str!("../../.github/skills/bicep/reference/commands.md"),
    ),
    (
        "reference/tools.md",
        include_str!("../../.github/skills/bicep/reference/tools.md"),
    ),
    (
        "reference/examples.md",
        include_str!("../../.github/skills/bicep/reference/examples.md"),
    ),
];

fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .find_map(|var| std::env::var_os(var).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
}

/// Directories where Copilot discovers skills: (label, skills dir).
pub fn discovery_dirs() -> Vec<(&'static str, PathBuf)> {
    let mut dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(("repository", cwd.join(".github").join("skills")));
    }
    if let Some(home) = home_dir() {
        dirs.push(("personal", home.join(".copilot").join("skills")));
        dirs.push(("personal (agents)", home.join(".agents").join("skills")));
    }
    dirs
}

fn target_dir(global: bool, dir: Option<&Path>) -> Result<PathBuf, CliError> {
    let base = if global {
        home_dir()
            .ok_or_else(|| CliError::usage("cannot find the home directory (HOME / USERPROFILE)"))?
            .join(".copilot")
            .join("skills")
    } else if let Some(dir) = dir {
        absolute(dir)?
    } else {
        absolute(Path::new(".github/skills"))?
    };
    Ok(base.join(SKILL_NAME))
}

pub fn run(ctx: &Ctx, action: SkillAction) -> Result<Outcome, CliError> {
    match action {
        SkillAction::Print { file } => print(file.as_deref()),
        SkillAction::Install { global, dir, force } => {
            let target = target_dir(global, dir.as_deref())?;
            let report = install_into(&target, force)?;
            Ok(Outcome::render(ctx.out(Format::Json), report, |v| {
                let mut lines = vec![format!(
                    "installed skill `{SKILL_NAME}` in {}",
                    v["dir"].as_str().unwrap_or("")
                )];
                for w in v["written"].as_array().into_iter().flatten() {
                    lines.push(format!("  wrote     {}", w.as_str().unwrap_or("")));
                }
                for u in v["unchanged"].as_array().into_iter().flatten() {
                    lines.push(format!("  unchanged {}", u.as_str().unwrap_or("")));
                }
                lines.join("\n")
            }))
        }
        SkillAction::Paths => {
            let entries: Vec<Value> = discovery_dirs()
                .into_iter()
                .map(|(label, dir)| {
                    let skill = dir.join(SKILL_NAME);
                    json!({
                        "kind": label,
                        "dir": path_string(&dir),
                        "installed": skill.join("SKILL.md").is_file(),
                        "current": is_current(&skill),
                    })
                })
                .collect();
            Ok(Outcome::render(
                ctx.out(Format::Json),
                Value::Array(entries),
                |v| {
                    v.as_array()
                        .map(|a| {
                            a.iter()
                                .map(|e| {
                                    let state =
                                        match (e["installed"].as_bool(), e["current"].as_bool()) {
                                            (Some(true), Some(true)) => "installed, current",
                                            (Some(true), _) => "installed, outdated",
                                            _ => "not installed",
                                        };
                                    format!(
                                        "{}\t{}\t{state}",
                                        e["kind"].as_str().unwrap_or(""),
                                        e["dir"].as_str().unwrap_or("")
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default()
                },
            ))
        }
    }
}

fn print(file: Option<&str>) -> Result<Outcome, CliError> {
    let wanted = file.unwrap_or("SKILL.md").replace('\\', "/");
    match SKILL_FILES.iter().find(|(name, _)| *name == wanted) {
        Some((_, content)) => Ok(Outcome::text((*content).to_owned())),
        None => Err(CliError::usage(format!(
            "unknown skill file `{wanted}`; available: {}",
            SKILL_FILES
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// True when every embedded file exists at `skill_dir` with identical content.
pub fn is_current(skill_dir: &Path) -> bool {
    SKILL_FILES.iter().all(|(name, content)| {
        std::fs::read_to_string(skill_dir.join(name))
            .map(|on_disk| on_disk == *content)
            .unwrap_or(false)
    })
}

/// Write the skill folder. Existing files with different content are only
/// overwritten with `force`.
pub fn install_into(skill_dir: &Path, force: bool) -> Result<Value, CliError> {
    let mut written = Vec::new();
    let mut unchanged = Vec::new();
    if !force {
        for (name, content) in SKILL_FILES {
            let path = skill_dir.join(name);
            if let Ok(existing) = std::fs::read_to_string(&path)
                && existing != *content
            {
                return Err(CliError::usage(format!(
                    "{} exists with different content; pass --force to overwrite",
                    path.display()
                )));
            }
        }
    }
    for (name, content) in SKILL_FILES {
        let path = skill_dir.join(name);
        if std::fs::read_to_string(&path).is_ok_and(|existing| existing == *content) {
            unchanged.push(Value::String(path_string(&path)));
            continue;
        }
        super::write_file(&path, content)?;
        written.push(Value::String(path_string(&path)));
    }
    Ok(json!({
        "dir": path_string(skill_dir),
        "written": written,
        "unchanged": unchanged,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::CommandFactory;
    use std::collections::HashSet;

    fn frontmatter(text: &str) -> Vec<(String, String)> {
        let mut lines = text.lines();
        assert_eq!(
            lines.next(),
            Some("---"),
            "SKILL.md must start with frontmatter"
        );
        lines
            .take_while(|l| *l != "---")
            .filter_map(|l| {
                l.split_once(':')
                    .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            })
            .collect()
    }

    #[test]
    fn frontmatter_meets_agent_skills_spec() {
        let skill = SKILL_FILES[0].1;
        let fm = frontmatter(skill);
        let get = |k: &str| fm.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        let name = get("name").expect("name");
        let description = get("description").expect("description");
        assert_eq!(name, SKILL_NAME);
        assert!(name.len() <= 64);
        assert!(name.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        assert!(
            !description.is_empty() && description.len() <= 1024,
            "description length {}",
            description.len()
        );
        assert!(
            description.contains("Use when"),
            "description must say when to use the skill"
        );
    }

    /// Every `bcp <word>` mentioned in the skill must be a real subcommand or
    /// alias, so the docs cannot drift from the CLI.
    #[test]
    fn every_mentioned_subcommand_exists() {
        let cmd = Cli::command();
        let mut known: HashSet<String> = HashSet::new();
        for sub in cmd.get_subcommands() {
            known.insert(sub.get_name().to_owned());
            known.extend(sub.get_all_aliases().map(str::to_owned));
        }
        let mut seen = 0;
        for (file, content) in SKILL_FILES {
            for (idx, _) in content.match_indices("`bcp ") {
                let rest = &content[idx + 5..];
                let word: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                    .collect();
                if word.is_empty() || word.starts_with('-') {
                    continue;
                }
                seen += 1;
                assert!(
                    known.contains(&word),
                    "{file} mentions unknown subcommand `bcp {word}`"
                );
            }
        }
        assert!(seen > 20, "expected many command mentions, found {seen}");
    }

    #[test]
    fn every_tool_is_documented() {
        let tools_md = SKILL_FILES
            .iter()
            .find(|(n, _)| *n == "reference/tools.md")
            .unwrap()
            .1;
        for tool in crate::tools::TOOL_NAMES {
            assert!(
                tools_md.contains(&format!("## `{tool}`")),
                "tools.md lacks {tool}"
            );
        }
    }

    #[test]
    fn install_round_trip_and_force() {
        let dir = std::env::temp_dir().join(format!("bcp-skill-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let skill = dir.join(SKILL_NAME);
        let first = install_into(&skill, false).unwrap();
        assert_eq!(
            first["written"].as_array().unwrap().len(),
            SKILL_FILES.len()
        );
        assert!(is_current(&skill));
        let second = install_into(&skill, false).unwrap();
        assert_eq!(
            second["unchanged"].as_array().unwrap().len(),
            SKILL_FILES.len()
        );
        std::fs::write(skill.join("SKILL.md"), "edited").unwrap();
        assert!(!is_current(&skill));
        assert_eq!(install_into(&skill, false).unwrap_err().exit_code(), 2);
        let forced = install_into(&skill, true).unwrap();
        assert_eq!(forced["written"].as_array().unwrap().len(), 1);
        assert!(is_current(&skill));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn print_unknown_file_is_usage_error() {
        assert_eq!(print(Some("nope.md")).unwrap_err().exit_code(), 2);
        assert!(matches!(
            print(None).unwrap().output,
            crate::output::Output::Text(_)
        ));
    }
}
