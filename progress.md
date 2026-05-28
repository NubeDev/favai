# favai consumer-side wiring — progress

**Session:** 2026-05-28
**Scope:** `feature = "skills"` consumer wiring — `starter-mcp` ×
`starter-skills` glued through `FavaiAgent` and shipped as a runnable
`favai` binary.

## Commits landed

| sha | what |
|-----|------|
| `2a51b01` | baseline of the pre-integration tree (sync state machine + bridge scaffolding from prior work) |
| `8916027` | drift cleanup vs `favai-sync-and-registry.md` (see §Drift fixes below) |
| `db47952` | Step F — `FavaiAgent` owns `Arc<SkillRegistry>`; `sync_now` drives `reload()`; accessors `skill_registry()` + `tool_registry()` |
| `ac70d87` | Step G — `crates/favai-cli` with `favai {serve,sync,list,help}` over `starter_mcp::run_stdio` |
| `57083e8` | Step H — integration test for the quarantine → approve → re-register loop + README quickstart |
| `7505890` | `favai help` no longer requires a config file; unit tests for `sync::sweep_source` crash recovery |

Step E (`McpBridgeConfig::from_favai_config`) folded into `8916027`
because the same `mcp_bridge.rs` rewrite removed the `repo_dirs`
trust escape hatch and added the constructor in one edit.

## Acceptance check

- `cargo build -p favai -p favai-cli` — green.
- `cargo test  -p favai` — 11 passed, 1 ignored (doctest), 0 failed.
- `cargo build --workspace` — zero warnings.
- `target/debug/favai help` — prints usage without touching `config.toml`.
- Adapter logic (`SkillTool`, `register_approved_skills`,
  `AddFavoriteTool`) is **not** duplicated; this crate only calls into
  `starter-mcp::skills_bridge`.

## Drift fixes against `favai-sync-and-registry.md`

The session opened with a stop-and-flag of seven drift points between
the design doc and the code that existed on disk. All were
resolved in `8916027` aligned with the doc:

1. **Watcher removed.** Doc §"Agent flow" step 3 deletes the
   filesystem watcher; the code had a live `src/watch/` module and
   a `ReloadTrigger::WatcherDebounced` variant. Both gone; `notify`
   dep dropped.
2. **`ReloadEvent` to doc-frozen shape.** Switched from
   `{trigger, sources, at}` to the doc's
   `{source, added, removed, changed_hash, at}`. v1 emits empty
   diffs and rebuilds the full `ToolRegistry`; the surface is
   frozen so v2 can switch to incremental updates without breakage.
3. **`apply_to_builder` to doc signature.** Now
   `(&FavaiConfig, SkillRegistryBuilder) -> SkillRegistryBuilder`,
   one `load_dir_quarantined(...)` per source.
4. **`McpBridgeConfig::repo_dirs` dropped.** The previous field
   honoured frontmatter `trust: approved` — the exact escape hatch
   the doc deleted in §"Trust model". `quarantined_dirs` is the
   only load path.
5. **Sync algorithm matches doc.** Always-fresh shallow clone
   (`--depth=1 --single-branch`), no fetch path, no `git clean`
   (staging is rebuilt fresh each sync). `sync/fetch.rs` removed.
6. **Crash-recovery sweep added.** `sync::sweep_source` implements
   the four-case table from §"Agent flow" step 2.iv. Called once
   per source by `FavaiAgent::start`. Unit-tested in
   `tests/sync/recovery_test.rs`.
7. **Two-step git probe.** Per the doc: `git --version` *plus*
   `git config --get-regexp .` (exit 0 or 1 both treated as healthy
   so a clean install with no rows passes the probe).

## Documented deviations from the design doc

Two places this work ships ahead of what the doc said. Both are
recorded in the relevant commit messages, no quiet drift:

- **`favai-cli` ships now**, not v2. The doc's §"Still open"
  deferred a binary to a later phase; the acceptance criteria for
  the consumer-side wiring required an end-to-end runnable binary,
  so the deferral closes early. The library remains the long-lived
  API.
- **`add_favorite_dir` default location.** The doc names the
  meta-tool but does not pin a directory. `McpBridgeConfig::from_favai_config`
  defaults it to `$HOME/.config/starter/favai/user-skills`
  (outside `sources/` so syncs cannot clobber it).

## Open / follow-up

- **Mid-session revoke visibility.** `starter_mcp::run_stdio`
  consumes `ToolRegistry` by value. After a mid-session `revoke()`
  or a sync that re-quarantines a bundle, the affected tool refuses
  to fire (the `SkillTool` adapter re-checks
  `SkillRegistry::list()` at invoke time) but still shows up in
  `tools/list` until the binary restarts. A future commit can swap
  stdio for a `Arc<RwLock<ToolRegistry>>`-shaped transport or wire
  the reload event into a rebuild on the active registry. Not
  required by v1.
- **Persistent `ApprovalStore`.** `favai-cli` currently wires an
  `InMemoryApprovalStore`. v1 dogfood (one personal pack across two
  PCs) is fine with that — every restart re-approves — but the
  "approval click per bundle per machine per change" promise from
  the doc only holds with a persistent store. Wiring one is a
  config knob away when the persistent variant lands in
  `starter-skills`.
- **Periodic sync.** v1 ships on-demand + on-startup only. The doc
  reserved a jittered periodic interval as opt-in; not implemented.
- **`SyncReport` diff fields.** `files_changed` / `bytes_pulled`
  are hardcoded to zero. Filling them requires diffing the
  pre-swap live tree against the post-swap staging, which is cheap
  but wasn't required by the acceptance criteria. Surface is
  there; populating it is a follow-up.
- **`approval_drift_test`'s `#[ignore]` placeholder** is now a real
  test (`sync_reload_reregister_loop`). The `#[ignore]` stub from
  the baseline tree was removed.

## How to point a host at favai

Both wiring snippets are in [README.md](README.md). The short
version:

```sh
favai serve     # default config at $HOME/.config/starter/favai/config.toml
```

…then add a `mcpServers.favai` entry pointing at the `favai` binary
with `serve` as its only arg, in whichever host config you use.
