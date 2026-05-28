//! `favai doctor` — check / install / uninstall MCP server entries
//! for the three common desktop hosts:
//!
//! - **Copilot (VS Code)** — `mcp.json`, key `servers`
//! - **Claude Code**       — `config.json`, key `mcpServers`
//! - **Codex CLI**         — `mcp.toml`, key `[servers.favai]`
//!
//! Two scopes per host:
//!
//! - `global` — the user-level config in `$HOME/.config/...`
//! - `local`  — `.vscode/mcp.json` / `.mcp.json` / `.codex/mcp.toml`
//!   inside the current working directory (or `--scope <path>`)
//!
//! The doctor never edits any key other than `favai`. It is safe to
//! re-run against a config that already has other MCP servers
//! registered.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Copilot,
    Claude,
    Codex,
}

impl Host {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "copilot" | "vscode" => Ok(Host::Copilot),
            "claude" | "claude-code" => Ok(Host::Claude),
            "codex" => Ok(Host::Codex),
            other => Err(format!("unknown host '{other}' (expected: copilot|claude|codex)")),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Host::Copilot => "copilot (vscode)",
            Host::Claude  => "claude-code",
            Host::Codex   => "codex",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Global,
    Local,
}

impl Scope {
    fn label(self) -> &'static str {
        match self {
            Scope::Global => "global",
            Scope::Local  => "local",
        }
    }
}

/// Resolved arguments parsed from `argv` after the `doctor` token.
pub struct DoctorArgs {
    pub action: Action,
    pub host:   Option<Host>,
    pub scope:  Option<Scope>,
    pub project_dir: PathBuf,
}

pub enum Action {
    Check,
    Install,
    Uninstall,
}

impl DoctorArgs {
    pub fn parse(args: &[String]) -> Result<Self, String> {
        // args = ["doctor", ...subcommand and flags...]
        let mut it = args.iter().skip(1);
        let sub = it.next().map(String::as_str).unwrap_or("check");
        let action = match sub {
            "check" | "status" => Action::Check,
            "install"          => Action::Install,
            "uninstall" | "remove" => Action::Uninstall,
            other => return Err(format!("unknown doctor subcommand '{other}'")),
        };

        let mut host: Option<Host> = None;
        let mut scope: Option<Scope> = None;
        let mut project_dir: Option<PathBuf> = None;

        while let Some(a) = it.next() {
            match a.as_str() {
                "--global"  => scope = Some(Scope::Global),
                "--local"   => scope = Some(Scope::Local),
                "--scope" | "--project" => {
                    let p = it.next().ok_or_else(|| "--scope requires a path".to_string())?;
                    project_dir = Some(PathBuf::from(p));
                    scope = Some(Scope::Local);
                }
                s if !s.starts_with("--") && host.is_none() => {
                    host = Some(Host::parse(s)?);
                }
                other => return Err(format!("unexpected argument '{other}'")),
            }
        }

        let project_dir = project_dir
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        Ok(Self { action, host, scope, project_dir })
    }
}

/// Entry point called from `main.rs`.
pub fn run(args: DoctorArgs) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    match args.action {
        Action::Check => {
            check_all(&args.project_dir, args.host);
            Ok(std::process::ExitCode::SUCCESS)
        }
        Action::Install => {
            let host = args.host.ok_or("usage: favai doctor install <host> [--global|--local]")?;
            let scope = args.scope.unwrap_or(Scope::Global);
            let path = config_path(host, scope, &args.project_dir)?;
            let bin  = favai_binary()?;
            install(host, &path, &bin)?;
            println!("installed {} ({}) → {}", host.label(), scope.label(), path.display());
            Ok(std::process::ExitCode::SUCCESS)
        }
        Action::Uninstall => {
            let host = args.host.ok_or("usage: favai doctor uninstall <host> [--global|--local]")?;
            let scope = args.scope.unwrap_or(Scope::Global);
            let path = config_path(host, scope, &args.project_dir)?;
            let removed = uninstall(host, &path)?;
            if removed {
                println!("removed favai from {}", path.display());
            } else {
                println!("favai not present in {}", path.display());
            }
            Ok(std::process::ExitCode::SUCCESS)
        }
    }
}

/// Print a status table for every (host, scope) combination.
fn check_all(project_dir: &Path, only: Option<Host>) {
    let bin = favai_binary().ok();
    println!(
        "favai binary: {}",
        bin.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(unknown)".into())
    );
    println!();
    println!(
        "{:<22}  {:<6}  {:<10}  {}",
        "HOST", "SCOPE", "STATUS", "PATH"
    );

    let hosts: &[Host] = match only {
        Some(h) => match h {
            Host::Copilot => &[Host::Copilot],
            Host::Claude  => &[Host::Claude],
            Host::Codex   => &[Host::Codex],
        },
        None => &[Host::Copilot, Host::Claude, Host::Codex],
    };

    for &host in hosts {
        for scope in [Scope::Global, Scope::Local] {
            let path = match config_path(host, scope, project_dir) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let status = host_status(host, &path, bin.as_deref());
            println!(
                "{:<22}  {:<6}  {:<10}  {}",
                host.label(),
                scope.label(),
                status,
                path.display(),
            );
        }
    }
}

fn host_status(host: Host, path: &Path, want_bin: Option<&Path>) -> &'static str {
    if !path.exists() {
        return "missing";
    }
    let Ok(text) = fs::read_to_string(path) else {
        return "unreadable";
    };
    let registered_cmd = match host {
        Host::Copilot => json_lookup(&text, "servers", "favai"),
        Host::Claude  => json_lookup(&text, "mcpServers", "favai"),
        Host::Codex   => toml_lookup_favai_command(&text),
    };
    match (registered_cmd, want_bin) {
        (Some(cmd), Some(want)) if Path::new(&cmd) == want => "installed",
        (Some(_), _) => "stale-bin",
        (None, _)    => "absent",
    }
}

fn json_lookup(text: &str, root_key: &str, name: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    v.get(root_key)?
        .get(name)?
        .get("command")?
        .as_str()
        .map(str::to_string)
}

fn toml_lookup_favai_command(text: &str) -> Option<String> {
    // Tiny scanner — we only need `[servers.favai]` → `command = "..."`.
    let mut in_section = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_section = line == "[servers.favai]";
            continue;
        }
        if in_section {
            if let Some(rest) = line.strip_prefix("command") {
                let rest = rest.trim_start().trim_start_matches('=').trim();
                let rest = rest.trim_matches(|c| c == '"' || c == '\'');
                if !rest.is_empty() {
                    return Some(rest.to_string());
                }
            }
        }
    }
    None
}

fn install(host: Host, path: &Path, bin: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match host {
        Host::Copilot => install_json(path, "servers", bin),
        Host::Claude  => install_json(path, "mcpServers", bin),
        Host::Codex   => install_toml(path, bin),
    }
}

fn install_json(path: &Path, root_key: &str, bin: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut root: Value = if path.exists() {
        let text = fs::read_to_string(path)?;
        if text.trim().is_empty() {
            json!({})
        } else {
            serde_json::from_str(&text)?
        }
    } else {
        json!({})
    };

    let obj = root.as_object_mut().ok_or("config root is not a JSON object")?;
    let servers = obj
        .entry(root_key.to_string())
        .or_insert_with(|| json!({}));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| format!("'{root_key}' is not a JSON object"))?;

    servers.insert(
        "favai".to_string(),
        json!({
            "command": bin.to_string_lossy(),
            "args":    ["serve"],
        }),
    );

    atomic_write(path, serde_json::to_string_pretty(&root)?.as_bytes())?;
    Ok(())
}

fn install_toml(path: &Path, bin: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let existing = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    // Strip any prior [servers.favai] block then append a fresh one.
    let pruned = strip_toml_section(&existing, "servers.favai");
    let mut out = pruned.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str("[servers.favai]\n");
    out.push_str(&format!("command = \"{}\"\n", bin.to_string_lossy()));
    out.push_str("args    = [\"serve\"]\n");

    atomic_write(path, out.as_bytes())?;
    Ok(())
}

fn uninstall(host: Host, path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(false);
    }
    match host {
        Host::Copilot => uninstall_json(path, "servers"),
        Host::Claude  => uninstall_json(path, "mcpServers"),
        Host::Codex   => uninstall_toml(path),
    }
}

fn uninstall_json(path: &Path, root_key: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(false);
    }
    let mut root: Value = serde_json::from_str(&text)?;
    let removed = root
        .get_mut(root_key)
        .and_then(Value::as_object_mut)
        .and_then(|m| m.remove("favai"))
        .is_some();
    if removed {
        atomic_write(path, serde_json::to_string_pretty(&root)?.as_bytes())?;
    }
    Ok(removed)
}

fn uninstall_toml(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    if !text.contains("[servers.favai]") {
        return Ok(false);
    }
    let pruned = strip_toml_section(&text, "servers.favai");
    atomic_write(path, pruned.as_bytes())?;
    Ok(true)
}

/// Remove the `[section]` block (until the next `[` or EOF). Whitespace-tolerant.
fn strip_toml_section(text: &str, section: &str) -> String {
    let header = format!("[{section}]");
    let mut out = String::with_capacity(text.len());
    let mut skipping = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == header {
            skipping = true;
            continue;
        }
        if skipping {
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                skipping = false;
                out.push_str(line);
                out.push('\n');
            }
            // else: drop the line
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("favai-tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

fn favai_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
    Ok(std::env::current_exe()?)
}

fn config_path(host: Host, scope: Scope, project_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").map_err(|_| "HOME is not set")?;
    let home = PathBuf::from(home);
    let p = match (host, scope) {
        // VS Code / Copilot
        (Host::Copilot, Scope::Local)  => project_dir.join(".vscode").join("mcp.json"),
        (Host::Copilot, Scope::Global) => {
            // Linux default; macOS would be ~/Library/Application Support/Code/User/mcp.json
            if cfg!(target_os = "macos") {
                home.join("Library").join("Application Support").join("Code").join("User").join("mcp.json")
            } else {
                home.join(".config").join("Code").join("User").join("mcp.json")
            }
        }

        // Claude Code — global config lives at ~/.claude.json (mcpServers at root).
        (Host::Claude, Scope::Local)   => project_dir.join(".mcp.json"),
        (Host::Claude, Scope::Global)  => home.join(".claude.json"),

        // Codex CLI
        (Host::Codex, Scope::Local)    => project_dir.join(".codex").join("mcp.toml"),
        (Host::Codex, Scope::Global)   => home.join(".config").join("codex").join("mcp.toml"),
    };
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn install_then_uninstall_json_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        let bin = PathBuf::from("/usr/local/bin/favai");
        install_json(&path, "servers", &bin).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["servers"]["favai"]["command"], "/usr/local/bin/favai");
        assert!(uninstall_json(&path, "servers").unwrap());
        let v2: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(v2["servers"].get("favai").is_none());
    }

    #[test]
    fn install_json_preserves_siblings() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(&path, r#"{"servers":{"other":{"command":"x"}}}"#).unwrap();
        install_json(&path, "servers", Path::new("/bin/favai")).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["servers"]["other"]["command"], "x");
        assert_eq!(v["servers"]["favai"]["command"], "/bin/favai");
    }

    #[test]
    fn toml_install_and_strip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("mcp.toml");
        fs::write(&path, "[servers.other]\ncommand = \"x\"\n").unwrap();
        install_toml(&path, Path::new("/bin/favai")).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[servers.other]"));
        assert!(text.contains("[servers.favai]"));
        assert_eq!(toml_lookup_favai_command(&text).as_deref(), Some("/bin/favai"));
        assert!(uninstall_toml(&path).unwrap());
        let after = fs::read_to_string(&path).unwrap();
        assert!(!after.contains("[servers.favai]"));
        assert!(after.contains("[servers.other]"));
    }
}
