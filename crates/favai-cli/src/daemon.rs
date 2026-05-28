//! `favai start` / `stop` / `status` — manage a long-running favai
//! process via a PID file.
//!
//! The MCP stdio transport is normally driven by the host (Claude
//! Code, Codex, Copilot all spawn `favai serve` themselves). These
//! commands exist for the standalone-daemon case: you want
//! periodic sync running in the background regardless of whether a
//! host is attached. The daemon boots the agent, services its
//! periodic sync loop, and parks on a signal handler until stopped.
//!
//! Unix-only. Non-unix builds will report an error.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Default PID file location next to the config.
pub fn pid_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".config")
        .join("starter")
        .join("favai")
        .join("favai.pid")
}

/// Default log file used when the daemon detaches from the terminal.
pub fn log_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home)
        .join(".config")
        .join("starter")
        .join("favai")
        .join("favai.log")
}

pub fn start(config_path: &Path) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let pid_file = pid_path();
    if let Some(pid) = read_pid(&pid_file) {
        if pid_alive(pid) {
            println!("favai already running (pid {pid})");
            return Ok(std::process::ExitCode::SUCCESS);
        }
        // Stale pid file — fall through and overwrite.
        let _ = fs::remove_file(&pid_file);
    }
    if let Some(parent) = pid_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let exe = std::env::current_exe()?;
    let log = log_path();
    if let Some(parent) = log.parent() {
        fs::create_dir_all(parent)?;
    }
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)?;
    let log_err = log_file.try_clone()?;

    let child = Command::new(&exe)
        .arg("--config")
        .arg(config_path)
        .arg("daemon-run")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_err))
        .spawn()?;

    let pid = child.id();
    fs::write(&pid_file, pid.to_string())?;
    println!("favai daemon started (pid {pid}); logs: {}", log.display());
    Ok(std::process::ExitCode::SUCCESS)
}

pub fn stop() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let pid_file = pid_path();
    let Some(pid) = read_pid(&pid_file) else {
        println!("favai not running (no pid file)");
        return Ok(std::process::ExitCode::SUCCESS);
    };
    if !pid_alive(pid) {
        let _ = fs::remove_file(&pid_file);
        println!("favai not running (stale pid file removed)");
        return Ok(std::process::ExitCode::SUCCESS);
    }
    #[cfg(unix)]
    {
        // SIGTERM via the kill(1) binary keeps us libc-free.
        let status = Command::new("kill")
            .arg(pid.to_string())
            .status()?;
        if !status.success() {
            return Err(format!("kill {pid} failed with {status}").into());
        }
        let _ = fs::remove_file(&pid_file);
        println!("favai stopped (pid {pid})");
        Ok(std::process::ExitCode::SUCCESS)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Err("favai stop is unix-only".into())
    }
}

pub fn status() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let pid_file = pid_path();
    match read_pid(&pid_file) {
        Some(pid) if pid_alive(pid) => {
            println!("favai running (pid {pid})");
            println!("pid file: {}", pid_file.display());
            println!("log file: {}", log_path().display());
            Ok(std::process::ExitCode::SUCCESS)
        }
        Some(pid) => {
            println!("favai not running (stale pid {pid} in {})", pid_file.display());
            Ok(std::process::ExitCode::from(1))
        }
        None => {
            println!("favai not running");
            Ok(std::process::ExitCode::from(1))
        }
    }
}

fn read_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // kill -0 returns 0 if the process exists and we can signal it.
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn pid_alive(_pid: u32) -> bool { false }
