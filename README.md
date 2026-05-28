# favai

GitHub-side half of the starter favourites system. Syncs `SKILL.md`
bundles from one or more git repositories into the local
`starter-skills` registry, and exposes the approved set as MCP
tools so Claude Code, Codex CLI, and Copilot can call them.

The starter side of the design (the adapter that turns an
`Arc<Skill>` into an MCP `Tool`, plus `register_approved_skills` and
`AddFavoriteTool`) lives in
[`starter-mcp`](../starter/crates/starter-mcp). This repo is the
consumer that wires it together with a sync agent + a runnable
binary. See [favai-sync-and-registry.md](favai-sync-and-registry.md)
for the full design.

## Crates

- **`favai`** — library. `FavaiAgent`, `FavaiConfig`,
  `McpBridgeConfig`, `apply_to_builder`. Used directly by consumers
  that want to embed the sync + registry surface in a larger binary.
- **`favai-cli`** — `favai` binary. Boots the agent and serves MCP
  over stdio. Default config path:
  `$HOME/.config/starter/favai/config.toml`.

```sh
cargo build -p favai-cli
cargo test  -p favai
```

## Quick demo (no git, no host)

The fastest way to see the idea work end-to-end is the
self-contained demo. It pre-stages a skill bundle on disk (skipping
clone/sync entirely) and walks the operator flow: quarantined →
approve → list → revoke, with the approval row persisted to a
tempdir JSONL file.

```sh
bash demo/run-demo.sh
```

For the MCP wire proof (host's view of `tools/list` + `tools/call`),
run the in-process integration test:

```sh
cargo test -p favai --test demo_e2e
```

## Running the binary

```sh
mkdir -p ~/.config/starter/favai
cat > ~/.config/starter/favai/config.toml <<'EOF'
[[source]]
name        = "nubedev-favai"
url         = "https://github.com/NubeDev/favai.git"
branch      = "main"
skills_path = "skills"

# Optional. When omitted, favai only syncs on demand
# ('favai sync <name>') and on startup. With this block, the agent
# also syncs every source every ~15 min (jittered ±10% so a fleet
# of PCs spreads out at the origin).
# [periodic]
# interval_secs = 900
EOF

# Pull the latest from upstream once, then exit.
favai sync nubedev-favai

# Boot the MCP stdio server. Logs go to stderr; stdin/stdout carry
# the JSON-RPC frame stream the host parses.
favai serve
```

### Pointing a host at favai

The fastest path is `favai doctor`, which checks and writes the MCP
config for each supported host. It only touches the `favai` key; any
other MCP servers in the same file are left alone.

```sh
# Show what's wired where, across all hosts and scopes:
favai doctor

# Add favai to a host (default scope is --global):
favai doctor install copilot --local            # → ./.vscode/mcp.json
favai doctor install copilot --global           # → ~/.config/Code/User/mcp.json
favai doctor install claude  --global           # → ~/.config/claude-code/config.json
favai doctor install claude  --local            # → ./.mcp.json
favai doctor install codex   --global           # → ~/.config/codex/mcp.toml

# Remove again:
favai doctor uninstall copilot --local
```

`--local` writes inside the current working directory by default;
use `--scope <dir>` to target another project root. The doctor
records the *absolute path* of whichever `favai` binary is running
the command, so reinstall after moving the binary.

For reference, here is what each host config ends up containing:

**Copilot (VS Code)** — `.vscode/mcp.json` (local) or
`~/.config/Code/User/mcp.json` (global):

```json
{ "servers": { "favai": { "command": "/abs/path/favai", "args": ["serve"] } } }
```

**Claude Code** — `~/.config/claude-code/config.json` (global) or
`.mcp.json` in the project root (local):

```json
{ "mcpServers": { "favai": { "command": "/abs/path/favai", "args": ["serve"] } } }
```

**Codex CLI** — `~/.config/codex/mcp.toml`:

```toml
[servers.favai]
command = "/abs/path/favai"
args    = ["serve"]
```

### Running favai as a background daemon

The MCP stdio transport is usually driven by the host — Claude Code,
Codex, and Copilot all spawn `favai serve` themselves. If you also
want periodic sync running independently of any attached host
(useful on a shared machine, or to keep mirrors warm), run favai as
a detached daemon:

```sh
favai start     # spawn favai in the background; writes a pid file
favai status    # is it alive?
favai stop      # SIGTERM the pid file
```

State lives under `~/.config/starter/favai/`:

- `favai.pid` — process id of the running daemon
- `favai.log` — combined stdout/stderr of the daemon process


## Trust model in one paragraph

Every synced bundle is loaded with `load_dir_quarantined(...)`. A
bundle's frontmatter `trust: approved` field is **ignored** for
synced sources; promotion is per-bundle, per-hash, per-machine,
recorded by an operator action on `ApprovalStore`. A `git pull` can
never make an unreviewed change live. See
[favai-sync-and-registry.md §Trust model](favai-sync-and-registry.md#trust-model).

### Approving bundles

Approvals are stored at `~/.config/starter/favai/approvals.jsonl`
(append-only, fsync-on-write) so an `Allow` decision survives a
`favai-cli` restart.

```sh
# See what's waiting on you:
favai quarantined

# Promote one bundle. With no hash arg, favai uses the currently
# quarantined hash for that id — typically what you want.
favai approve com.example.skills.refactor

# Revoke later if a follow-up sync brings in a hash you no longer
# trust:
favai revoke com.example.skills.refactor
```