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

**Claude Code** (`~/.config/claude-code/config.json`):

```json
{
  "mcpServers": {
    "favai": {
      "command": "favai",
      "args": ["serve"]
    }
  }
}
```

**Codex CLI** (`~/.config/codex/mcp.toml`):

```toml
[servers.favai]
command = "favai"
args    = ["serve"]
```

Copilot uses the same shape — point its MCP server config at the
`favai` binary with `serve` as the only arg.

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