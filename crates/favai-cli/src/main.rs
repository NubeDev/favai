//! `favai` — runnable consumer binary that boots [`FavaiAgent`] and
//! serves MCP over stdio.
//!
//! ```text
//! favai serve                  # default; reads ~/.config/starter/favai/config.toml
//! favai sync <name>            # one-shot sync
//! favai list                   # list configured sources
//! favai quarantined            # list bundles awaiting operator approval
//! favai approve <skill-id> [bundle-hash]
//!                              # approve a quarantined bundle (hash optional;
//!                              # if omitted, uses the current quarantined hash)
//! favai revoke  <skill-id> [bundle-hash]
//!                              # revoke an approval; same hash defaulting rule
//!
//! favai doctor                 # check MCP host wiring (copilot/claude/codex)
//! favai doctor install <host> [--global|--local] [--scope <dir>]
//! favai doctor uninstall <host> [--global|--local]
//!
//! favai start                  # spawn a detached daemon (periodic sync)
//! favai stop                   # signal the daemon to exit
//! favai status                 # report daemon liveness
//!
//! favai help                   # this message
//! ```
//!
//! The stdio transport is the MCP norm for desktop hosts (Claude
//! Code, Codex CLI, Copilot). Point the host at the `favai` binary
//! with `serve` as the only arg and it speaks JSON-RPC over
//! stdin/stdout. `favai doctor install` writes that wiring for you.

mod daemon;
mod doctor;

use std::path::PathBuf;
use std::sync::Arc;

use favai::approvals::{default_approvals_path, JsonlApprovalStore};
use favai::mcp_bridge::{build_tool_registry, McpBridgeConfig};
use favai::{FavaiAgent, FavaiConfig};
use starter_flow_spi::skill::SkillId;
use starter_mcp::run_stdio;
use starter_skills::{ApprovalStore, SkillRegistry};
use starter_spi::auth::{Principal, Role};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        // MCP stdio is JSON-RPC on stdout; logs must go to stderr or
        // the host's frame parser chokes.
        .with_writer(std::io::stderr)
        .init();

    match run().await {
        Ok(code) => code,
        Err(e) => {
            eprintln!("favai: {e}");
            std::process::ExitCode::from(1)
        }
    }
}

async fn run() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let (config_flag, args) = strip_config_flag(&raw_args)?;
    let cmd = args.first().map(String::as_str).unwrap_or("serve");

    // `help` and the host-wiring / daemon-control commands must not
    // require a config file — they are the user's recovery paths
    // when the config is wrong or missing.
    if matches!(cmd, "help" | "--help" | "-h") {
        print_help();
        return Ok(std::process::ExitCode::SUCCESS);
    }
    if cmd == "doctor" {
        let parsed = doctor::DoctorArgs::parse(&args)?;
        return doctor::run(parsed);
    }
    if matches!(cmd, "stop" | "status") {
        return match cmd {
            "stop"   => daemon::stop(),
            "status" => daemon::status(),
            _        => unreachable!(),
        };
    }
    if cmd == "start" {
        let config_path = config_flag.clone().unwrap_or_else(default_config_path);
        return daemon::start(&config_path);
    }

    let config_path = config_flag.unwrap_or_else(default_config_path);
    let config = FavaiConfig::from_file(&config_path)?;

    match cmd {
        "serve"        => serve(config).await,
        "daemon-run"   => daemon_run(config).await,
        "sync"         => sync(config, args.get(1).cloned()).await,
        "list"         => list(config).await,
        "quarantined"  => quarantined(config).await,
        "approve"      => approve(config, args.get(1).cloned(), args.get(2).cloned()).await,
        "revoke"       => revoke(config, args.get(1).cloned(), args.get(2).cloned()).await,
        other => {
            eprintln!("favai: unknown command '{other}'");
            print_help();
            Ok(std::process::ExitCode::from(2))
        }
    }
}

/// Build the agent + skill registry against the persistent
/// [`JsonlApprovalStore`]. Every command except `help` goes through
/// here so they all see the same approval state.
async fn boot(config: FavaiConfig) -> Result<(FavaiAgent, Arc<SkillRegistry>), Box<dyn std::error::Error>> {
    let bridge_config = McpBridgeConfig::from_favai_config(&config)?;
    let approvals: Arc<dyn ApprovalStore> =
        Arc::new(JsonlApprovalStore::open(default_approvals_path()?)?);
    let (skills, _) = build_tool_registry(&bridge_config, approvals).await?;
    let skills = Arc::new(skills);
    let agent =
        FavaiAgent::start(config, Arc::clone(&skills), bridge_config.add_favorite_dir).await?;
    Ok((agent, skills))
}

async fn serve(config: FavaiConfig) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let (agent, skills) = boot(config).await?;
    let tool_registry = agent.tool_registry();
    tracing::info!(
        sources = agent.sources().len(),
        approved_skills = skills.list().len(),
        quarantined_skills = skills.list_quarantined().len(),
        "favai: starting MCP stdio loop"
    );
    run_stdio(tool_registry).await?;
    agent.shutdown().await;
    Ok(std::process::ExitCode::SUCCESS)
}

/// `daemon-run` is the body of the detached child spawned by
/// `favai start`. It boots the agent (which kicks off the
/// periodic-sync loop) and then parks on SIGTERM / SIGINT. No MCP
/// stdio — the host is not attached here.
async fn daemon_run(config: FavaiConfig) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let (agent, skills) = boot(config).await?;
    tracing::info!(
        sources = agent.sources().len(),
        approved_skills = skills.list().len(),
        "favai: daemon up; awaiting shutdown signal"
    );

    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate())?;
        let mut intr = signal(SignalKind::interrupt())?;
        tokio::select! {
            _ = term.recv() => tracing::info!("favai: SIGTERM"),
            _ = intr.recv() => tracing::info!("favai: SIGINT"),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
    }

    agent.shutdown().await;
    Ok(std::process::ExitCode::SUCCESS)
}

async fn sync(
    config: FavaiConfig,
    name: Option<String>,
) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let name = name.ok_or("usage: favai sync <source-name>")?;
    let (agent, _) = boot(config).await?;
    let report = agent.sync_now(&name).await?;
    println!(
        "synced {}: head={} ({} ms)",
        report.source_name, report.new_head_sha, report.duration_ms
    );
    Ok(std::process::ExitCode::SUCCESS)
}

async fn list(config: FavaiConfig) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    if config.sources.is_empty() {
        println!("(no sources configured)");
        return Ok(std::process::ExitCode::SUCCESS);
    }
    let (agent, skills) = boot(config).await?;
    // BUNDLES counts SKILL.md directories on disk for each source —
    // i.e. what a sync produced. The APPROVED / QUARANTINED totals
    // at the bottom are global (the registry does not attribute
    // skills back to a single source).
    println!("{:<20}  {:<8}  {:<10}  {:>7}  {}", "NAME", "BRANCH", "HEAD", "BUNDLES", "URL");
    for s in agent.sources() {
        let head = s.head_sha.as_deref().map(|h| &h[..h.len().min(8)]).unwrap_or("-");
        println!(
            "{:<20}  {:<8}  {:<10}  {:>7}  {}",
            s.name, s.branch, head, s.skill_count, s.url
        );
    }
    println!(
        "\n{} approved, {} quarantined (across all sources)",
        skills.list().len(),
        skills.list_quarantined().len(),
    );
    Ok(std::process::ExitCode::SUCCESS)
}

async fn quarantined(config: FavaiConfig) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let (_, skills) = boot(config).await?;
    let rows = skills.list_quarantined();
    if rows.is_empty() {
        println!("(no quarantined bundles)");
        return Ok(std::process::ExitCode::SUCCESS);
    }
    println!("{:<40}  {}", "SKILL_ID", "BUNDLE_HASH");
    for skill in rows {
        println!("{:<40}  {}", skill.id, skill.bundle_hash);
    }
    Ok(std::process::ExitCode::SUCCESS)
}

async fn approve(
    config: FavaiConfig,
    skill_id: Option<String>,
    bundle_hash: Option<String>,
) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let raw_id = skill_id.ok_or("usage: favai approve <skill-id> [bundle-hash]")?;
    let id = SkillId::new(raw_id)?;
    let (_, skills) = boot(config).await?;

    // Resolve hash: explicit arg wins; otherwise look up the bundle
    // currently quarantined under this id.
    let hash = match bundle_hash {
        Some(h) => h,
        None => skills
            .list_quarantined()
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.bundle_hash.clone())
            .ok_or_else(|| format!("no quarantined bundle with id '{id}'"))?,
    };

    skills.approve(&id, &hash, &operator_principal()).await?;
    println!("approved {id} @ {hash}");
    Ok(std::process::ExitCode::SUCCESS)
}

async fn revoke(
    config: FavaiConfig,
    skill_id: Option<String>,
    bundle_hash: Option<String>,
) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let raw_id = skill_id.ok_or("usage: favai revoke <skill-id> [bundle-hash]")?;
    let id = SkillId::new(raw_id)?;
    let (_, skills) = boot(config).await?;

    let hash = match bundle_hash {
        Some(h) => h,
        None => skills
            .list()
            .into_iter()
            .find(|s| s.id == id)
            .map(|s| s.bundle_hash.clone())
            .ok_or_else(|| format!("no approved bundle with id '{id}'"))?,
    };

    skills.revoke(&id, &hash, &operator_principal()).await?;
    println!("revoked {id} @ {hash}");
    Ok(std::process::ExitCode::SUCCESS)
}

fn operator_principal() -> Principal {
    Principal {
        subject:   std::env::var("USER").unwrap_or_else(|_| "favai-cli".into()),
        role:      Role::Admin,
        scopes:    vec![],
        tenant_id: None,
        teams:     vec![],
        extra:     serde_json::Value::Null,
    }
}

/// Pull `--config <path>` / `--config=<path>` out of the argv and
/// return the remaining tokens. Returns the chosen config path (if
/// any) plus the trimmed arg vector — subcommands can then index
/// positionally without worrying about the flag's location.
fn strip_config_flag(args: &[String]) -> Result<(Option<PathBuf>, Vec<String>), Box<dyn std::error::Error>> {
    let mut out = Vec::with_capacity(args.len());
    let mut cfg: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--config" {
            let p = it.next().ok_or("--config requires a path")?;
            cfg = Some(PathBuf::from(p));
        } else if let Some(rest) = a.strip_prefix("--config=") {
            cfg = Some(PathBuf::from(rest));
        } else {
            out.push(a.clone());
        }
    }
    Ok((cfg, out))
}

fn default_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".config")
        .join("starter")
        .join("favai")
        .join("config.toml")
}

fn print_help() {
    eprintln!(
        "favai — MCP server for synced skill favourites\n\
         \n\
         USAGE:\n  \
         favai [--config <path>] <command>\n\
         \n\
         CORE COMMANDS:\n  \
         serve                    Boot the agent and serve MCP over stdio (default)\n  \
         sync <name>              Run one sync against the named source and exit\n  \
         list                     List configured sources\n  \
         quarantined              List bundles awaiting operator approval\n  \
         approve <id> [<hash>]    Approve a quarantined bundle (hash defaults to\n  \
                                  the currently-quarantined hash for that id)\n  \
         revoke  <id> [<hash>]    Revoke an approval; same hash defaulting rule\n  \
         help                     Show this message\n\
         \n\
         DOCTOR (host wiring):\n  \
         doctor                       Status of MCP entries across all hosts\n  \
         doctor install <host>        Add favai to a host's MCP config\n  \
         doctor uninstall <host>      Remove favai from a host's MCP config\n  \
           hosts:  copilot | claude | codex\n  \
           flags:  --global (default) | --local [--scope <project-dir>]\n  \
         \n\
         DAEMON (background process):\n  \
         start                    Spawn favai as a detached background daemon\n  \
         stop                     Signal the running daemon to exit\n  \
         status                   Report whether the daemon is running\n  \
         \n\
         CONFIG:\n  \
         Defaults to $HOME/.config/starter/favai/config.toml.\n  \
         Approvals persist in $HOME/.config/starter/favai/approvals.jsonl.\n  \
         Daemon pid:  $HOME/.config/starter/favai/favai.pid\n  \
         Daemon log:  $HOME/.config/starter/favai/favai.log\n  \
         See favai-sync-and-registry.md for the schema.\n\
         "
    );
}
