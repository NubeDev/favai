//! `favai` — runnable consumer binary that boots [`FavaiAgent`] and
//! serves MCP over stdio.
//!
//! Usage:
//!
//! ```text
//! favai serve            # default: read ~/.config/starter/favai/config.toml
//! favai serve --config /path/to/config.toml
//! favai sync <name>      # one-shot sync; prints SyncReport and exits
//! favai list             # list configured sources
//! ```
//!
//! The stdio transport is the MCP norm for desktop hosts (Claude
//! Code, Codex CLI, Copilot). Point the host at the `favai`
//! binary with no args after `serve` and it speaks JSON-RPC over
//! stdin/stdout.

use std::path::PathBuf;
use std::sync::Arc;

use favai::mcp_bridge::{build_tool_registry, McpBridgeConfig};
use favai::{FavaiAgent, FavaiConfig};
use starter_mcp::run_stdio;
use starter_skills::InMemoryApprovalStore;

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("serve");

    // `help` must not require a config file — it is the user's
    // recovery path when their config is wrong or missing.
    if matches!(cmd, "help" | "--help" | "-h") {
        print_help();
        return Ok(std::process::ExitCode::SUCCESS);
    }

    let config_path = parse_config_flag(&args)?.unwrap_or_else(default_config_path);
    let config = FavaiConfig::from_file(&config_path)?;

    match cmd {
        "serve" => serve(config).await,
        "sync"  => sync(config, args.get(1).cloned()).await,
        "list"  => list(config),
        other => {
            eprintln!("favai: unknown command '{other}'");
            print_help();
            Ok(std::process::ExitCode::from(2))
        }
    }
}

async fn serve(config: FavaiConfig) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let bridge_config = McpBridgeConfig::from_favai_config(&config)?;
    let approvals = Arc::new(InMemoryApprovalStore::new());
    let (skills, _initial_tools) =
        build_tool_registry(&bridge_config, approvals.clone()).await?;
    let skills = Arc::new(skills);

    let agent = FavaiAgent::start(
        config,
        Arc::clone(&skills),
        bridge_config.add_favorite_dir.clone(),
    )
    .await?;

    // run_stdio consumes the ToolRegistry by value. Build it after
    // FavaiAgent::start so the crash-recovery sweep has already
    // settled any half-completed swaps from a previous run.
    let tool_registry = agent.tool_registry();
    tracing::info!(
        sources = agent.sources().len(),
        approved_skills = skills.list().len(),
        "favai: starting MCP stdio loop"
    );
    run_stdio(tool_registry).await?;
    agent.shutdown().await;
    Ok(std::process::ExitCode::SUCCESS)
}

async fn sync(
    config: FavaiConfig,
    name: Option<String>,
) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let name = name.ok_or("usage: favai sync <source-name>")?;
    let bridge_config = McpBridgeConfig::from_favai_config(&config)?;
    let approvals = Arc::new(InMemoryApprovalStore::new());
    let (skills, _) = build_tool_registry(&bridge_config, approvals).await?;
    let skills = Arc::new(skills);
    let agent =
        FavaiAgent::start(config, Arc::clone(&skills), bridge_config.add_favorite_dir).await?;

    let report = agent.sync_now(&name).await?;
    println!("synced {}: head={} ({} ms)", report.source_name, report.new_head_sha, report.duration_ms);
    Ok(std::process::ExitCode::SUCCESS)
}

fn list(config: FavaiConfig) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    if config.sources.is_empty() {
        println!("(no sources configured)");
    }
    for source in &config.sources {
        println!("{}\t{}\t{}\t{}", source.name, source.branch, source.url, source.skills_path);
    }
    Ok(std::process::ExitCode::SUCCESS)
}

fn parse_config_flag(args: &[String]) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if a == "--config" {
            let p = it.next().ok_or("--config requires a path")?;
            return Ok(Some(PathBuf::from(p)));
        }
        if let Some(rest) = a.strip_prefix("--config=") {
            return Ok(Some(PathBuf::from(rest)));
        }
    }
    Ok(None)
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
         COMMANDS:\n  \
         serve            Boot the agent and serve MCP over stdio (default)\n  \
         sync <name>      Run one sync against the named source and exit\n  \
         list             List configured sources\n  \
         help             Show this message\n\
         \n\
         CONFIG:\n  \
         Defaults to $HOME/.config/starter/favai/config.toml. See\n  \
         favai-sync-and-registry.md for the schema.\n\
         "
    );
}
