# favai — usage

Operator manual for the `favai` binary. For design and trust-model
background see [favai-sync-and-registry.md](favai-sync-and-registry.md);
for project overview see [README.md](README.md).

## TL;DR

```sh
# 1. Build & install
cargo build -p favai-cli --release
install -m 0755 target/release/favai ~/.local/bin/favai

# 2. Tell favai which git repo(s) carry your skill bundles
mkdir -p ~/.config/starter/favai
$EDITOR  ~/.config/starter/favai/config.toml

# 3. Wire favai into your hosts (Copilot / Claude Code / Codex)
favai doctor install copilot --global
favai doctor install claude  --global
favai doctor install codex   --global

# 4. First sync, then approve what you want exposed as tools
favai sync my-skills
favai quarantined
favai approve com.example.skills.ship-it-check

# 5. (Optional) keep periodic sync running independent of any host
favai start
```

## Command index

| Command | Purpose |
|---|---|
| `favai serve` | Boot the agent and serve MCP over stdio (what hosts spawn). |
| `favai sync <name>` | One-shot sync against a configured source, then exit. |
| `favai list` | List configured sources + global approval counts. |
| `favai quarantined` | List bundles waiting on operator approval. |
| `favai approve <id> [<hash>]` | Promote a quarantined bundle to live. |
| `favai revoke  <id> [<hash>]` | Revoke a previously-approved bundle. |
| `favai doctor` | Show MCP wiring status for every host × scope. |
| `favai doctor install <host> [--global\|--local]` | Add favai to a host's MCP config. |
| `favai doctor uninstall <host> [--global\|--local]` | Remove favai from a host's MCP config. |
| `favai start` / `stop` / `status` | Background daemon controls. |
| `favai help` | Print built-in help. |

All commands accept `--config <path>` to override the default config
location. The flag can appear anywhere in the argv.

## Configuration

Default path: `$HOME/.config/starter/favai/config.toml`.

```toml
# Each source is one git repo holding SKILL.md bundles.
[[source]]
name        = "my-skills"
url         = "https://github.com/me/my-skills.git"
branch      = "main"
skills_path = "skills"          # subdir inside the repo

# Optional. Without this block, favai only syncs on demand
# ('favai sync <name>') and on startup. With it, the agent runs
# every source on a jittered interval — useful for the daemon.
[periodic]
interval_secs = 900             # ~15 min, ±10% jitter per source
```

Persistent state lives next to the config:

- `~/.config/starter/favai/approvals.jsonl` — append-only approval log
- `~/.config/starter/favai/sources/<name>/` — on-disk bundles for each source
- `~/.config/starter/favai/favai.pid` / `favai.log` — daemon state

## Trust model in one paragraph

Every synced bundle is loaded with `load_dir_quarantined(...)`. A
bundle's frontmatter `trust: approved` field is **ignored** for
synced sources — promotion is per-bundle, per-hash, per-machine,
recorded by an operator action. A `git pull` can never make an
unreviewed change live. After every sync that introduces a new hash,
the bundle drops back into quarantine until you re-approve.

## Day-one walkthrough

### 1. Build the binary

```sh
cargo build -p favai-cli --release
install -m 0755 target/release/favai ~/.local/bin/favai   # or anywhere on $PATH
```

### 2. Write a config

```sh
mkdir -p ~/.config/starter/favai
cat > ~/.config/starter/favai/config.toml <<'EOF'
[[source]]
name        = "my-skills"
url         = "https://github.com/me/my-skills.git"
branch      = "main"
skills_path = "skills"
EOF
```

### 3. Wire favai into your hosts

```sh
favai doctor                      # see what's wired (everything is 'missing')
favai doctor install copilot --global
favai doctor install claude  --global
favai doctor install codex   --global
favai doctor                      # all three should now read 'installed'
```

The doctor records the **absolute path** of whichever `favai` binary
is running. If you move the binary, re-run `favai doctor install`.

### 4. Pull bundles, then approve

```sh
favai sync my-skills
favai quarantined
# SKILL_ID                                  BUNDLE_HASH
# com.example.skills.ship-it-check          sha256:7f3c…
favai approve com.example.skills.ship-it-check
favai list
```

Approved bundles show up in `tools/list` for any host that has favai
wired. Hosts will pick up new tools the next time they reconnect
their MCP client.

### 5. (Optional) keep periodic sync running

```sh
favai start                       # spawn detached daemon
favai status                      # 'favai running (pid N)'
favai stop                        # SIGTERM the daemon
```

`favai start` is idempotent — running it while the daemon is already
up prints the existing pid and exits 0.

## `favai doctor` reference

### Status output

```
favai binary: /home/user/.local/bin/favai

HOST                    SCOPE   STATUS      PATH
copilot (vscode)        global  installed   /home/user/.config/Code/User/mcp.json
copilot (vscode)        local   missing     /cwd/.vscode/mcp.json
claude-code             global  installed   /home/user/.claude.json
claude-code             local   missing     /cwd/.mcp.json
codex                   global  installed   /home/user/.config/codex/mcp.toml
codex                   local   missing     /cwd/.codex/mcp.toml
```

| STATUS | Meaning |
|---|---|
| `missing`    | The config file does not exist. |
| `absent`     | File exists; no `favai` entry inside. |
| `installed`  | `favai` entry present and points at the *current* binary. |
| `stale-bin`  | `favai` entry present but points at a different binary path. Re-run `favai doctor install` to fix. |
| `unreadable` | File exists but cannot be parsed. |

### Hosts

| Token | Host |
|---|---|
| `copilot` (alias `vscode`) | GitHub Copilot in VS Code |
| `claude` (alias `claude-code`) | Anthropic Claude Code CLI |
| `codex` | OpenAI Codex CLI |

### Scopes

| Flag | Path written |
|---|---|
| `--global` *(default)* | Per-user config in `$HOME/.config/...` |
| `--local`              | Per-project file inside the current directory |
| `--scope <dir>`        | Implies `--local`; treat `<dir>` as the project root |

Resolved paths per (host, scope):

| Host | Global | Local |
|---|---|---|
| copilot | `~/.config/Code/User/mcp.json` (Linux), `~/Library/Application Support/Code/User/mcp.json` (macOS) | `<dir>/.vscode/mcp.json` |
| claude  | `~/.claude.json` | `<dir>/.mcp.json` |
| codex   | `~/.config/codex/mcp.toml` | `<dir>/.codex/mcp.toml` |

### What the doctor writes

JSON hosts (Copilot, Claude Code) end up with:

```json
{
  "servers": {
    "favai": { "command": "/abs/path/favai", "args": ["serve"] }
  }
}
```

(Copilot uses `servers`; Claude Code uses `mcpServers`. Doctor uses
the right key for each host and leaves all other keys untouched.)

Codex (TOML):

```toml
[servers.favai]
command = "/abs/path/favai"
args    = ["serve"]
```

### Safety notes

- Every install / uninstall is an **atomic** write (tmp file + `rename`).
- The doctor only ever creates, updates, or removes the `favai` key
  inside the existing root section. Other MCP servers in the same
  file survive untouched.
- Re-running `favai doctor install` is the supported way to refresh
  the binary path after a `cargo install` / move.

## Daemon reference

### Lifecycle

```sh
favai start    # spawns 'favai daemon-run' detached, writes pid file
favai status   # exits 0 if alive, 1 if stale / missing
favai stop     # SIGTERM the pid, removes the pid file
```

State paths:

- `~/.config/starter/favai/favai.pid` — pid of the running daemon
- `~/.config/starter/favai/favai.log` — combined stdout/stderr

### What the daemon does (and does not do)

The daemon boots the same agent that `favai serve` does, which kicks
off the `[periodic]` sync loop if configured. **It does not speak MCP
stdio** — no host is attached, so JSON-RPC would have nowhere to go.
The daemon is for keeping mirrors warm and approvals fresh on a
shared machine.

If you want both periodic sync *and* a host-facing MCP server, the
host's own `favai serve` invocation already provides both — the
daemon is only needed when no host is in the picture.

### Recovery

- Stale pid file (process died but file remains): `favai start` and
  `favai stop` both detect this and clear it.
- Crash mid-sync: the next `favai serve` / `favai start` runs the
  crash-recovery sweep before exposing tools, so half-completed
  source swaps get rolled forward or back automatically.

## Approval workflow

```sh
# After every sync that introduces new content:
favai quarantined

# Promote one bundle. With no hash arg, favai uses the currently
# quarantined hash for that id — which is what you almost always
# want.
favai approve com.example.skills.refactor

# A follow-up sync brings in a new hash for an already-approved id:
# the bundle drops back to quarantine. Inspect the diff in the
# source repo, then either:
favai approve com.example.skills.refactor              # re-approve the new hash
# or
favai revoke  com.example.skills.refactor              # drop the old approval too
```

Approvals are append-only in `approvals.jsonl` and survive restarts.
A single `id` can only have one currently-approved hash at a time.

## Troubleshooting

| Symptom | Likely cause / fix |
|---|---|
| Host doesn't see the `favai` tool | Reconnect the MCP server in the host (Copilot: "MCP: Restart Server"; Claude Code: `/mcp restart`; Codex: restart the CLI). The host caches `tools/list` on connect. |
| `favai doctor` shows `stale-bin` | Re-run `favai doctor install <host>` from the location of the new binary. |
| `favai status` says `stale pid` | Previous daemon died ungracefully. Run `favai stop` (clears the file) then `favai start` again. |
| `favai sync` errors with `invalid URL scheme` | Sources must be `https://…` or `git@…:…`; local paths are rejected. |
| Approved bundle disappeared after `sync` | New hash dropped it back into quarantine. Run `favai quarantined` to see it, then `favai approve <id>` again. |
| Logs nowhere visible during `favai start` | The daemon writes to `~/.config/starter/favai/favai.log`; `tail -f` it. |

## Environment variables

| Variable | Effect |
|---|---|
| `HOME` | Root of every default path (config, approvals, pid, log, host configs). |
| `RUST_LOG` | Standard `tracing` filter, e.g. `RUST_LOG=favai=debug,starter_skills=info`. |
| `USER` | Recorded as the operator principal on every approval row. |
