#!/usr/bin/env bash
#
# run-demo.sh — prove favai end-to-end without git, without a host.
#
# What this proves:
#   - favai discovers a skill bundle on disk and quarantines it.
#   - 'favai approve' promotes it (operator-driven, not frontmatter).
#   - 'favai list' reflects on-disk state, not just config.
#   - 'favai revoke' un-promotes it; approval state persists across
#     restarts (we run favai-cli many times in this script).
#
# What this DOES NOT do:
#   - Run 'favai sync' against a real git remote (deferred per
#     "we can do the git syncing later"). Instead we pre-stage the
#     post-swap live tree by hand. The skills-loading + approval
#     path is identical either way.
#   - Drive the MCP stdio loop. The in-process test
#     'tests/demo_e2e_test.rs' covers tools/list + tools/call.
#
# Run from the repo root:
#   bash demo/run-demo.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "==> building favai-cli (release-ish: dev profile)"
cargo build -q -p favai-cli

# Isolated tempdir so the demo never touches real $HOME state.
DEMO_ROOT="$(mktemp -d -t favai-demo-XXXXXX)"
trap 'rm -rf "$DEMO_ROOT"' EXIT

export FAVAI_SOURCES_ROOT="$DEMO_ROOT/sources"
export HOME="$DEMO_ROOT/home"   # redirects ~/.config/starter/favai/...
mkdir -p "$HOME/.config/starter/favai" "$FAVAI_SOURCES_ROOT"

# Minimal config.toml. The URL is never reached — we don't sync.
cat >"$HOME/.config/starter/favai/config.toml" <<'EOF'
[[source]]
name        = "demo"
url         = "https://example.invalid/demo.git"
branch      = "main"
skills_path = "skills"
EOF

# Pre-stage the post-swap live tree exactly as 'favai sync' would
# have produced it. Layout: <sources_root>/<name>/<skills_path>/<bundle>/SKILL.md
DEST="$FAVAI_SOURCES_ROOT/demo/skills/hello"
mkdir -p "$DEST"
cp "$REPO_ROOT/demo/skills/hello/SKILL.md" "$DEST/SKILL.md"

FAVAI="$REPO_ROOT/target/debug/favai"

banner() { printf '\n\033[1;36m== %s ==\033[0m\n' "$*"; }

banner "1. quarantined bundles (operator sees what's pending)"
"$FAVAI" quarantined

banner "2. list (no approved skills yet)"
"$FAVAI" list

banner "3. approve the demo bundle (no hash arg — uses current quarantined hash)"
"$FAVAI" approve com.demo.skills.hello

banner "4. quarantined again (should now be empty)"
"$FAVAI" quarantined

banner "5. list (approved total should now read 1)"
"$FAVAI" list

banner "6. approval row persisted to disk:"
ls -l "$HOME/.config/starter/favai/approvals.jsonl"
cat "$HOME/.config/starter/favai/approvals.jsonl"

banner "7. revoke and confirm it sticks across a fresh CLI invocation"
"$FAVAI" revoke com.demo.skills.hello
"$FAVAI" quarantined        # back in quarantine
"$FAVAI" list               # SKILLS back to 0

printf '\n\033[1;32mDemo passed.\033[0m The same approval state machine is what '
printf 'an MCP host sees via tools/list.\nFor the stdio wire proof, run:\n'
printf '   cargo test -p favai --test demo_e2e\n\n'
