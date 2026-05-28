# favai: sync, share, and discover favorites

**Status:** scope proposal. Not yet implemented.
**Owner:** ap@nube-io.com
**Date:** 2026-05-28
**Companion to:** [starter/DOCS/skills-as-mcp-tools.md](/home/user/code/rust/starter/DOCS/skills-as-mcp-tools.md)

## What this is

`favai` is the GitHub-side half of the favorites system. Where
[starter/DOCS/skills-as-mcp-tools.md](/home/user/code/rust/starter/DOCS/skills-as-mcp-tools.md)
covers exposing local skills as MCP tools, this doc covers:

- syncing **my own** favorites across multiple PCs,
- sharing favorites with friends or teammates,
- discovering public favorites repositories,
- pulling third-party favorites into the local store **safely**.

Repo: <https://github.com/NubeDev/favai>.

## Hard rule: disk is still the source of truth

`starter-skills` already loads from `load_dir(...)` roots on disk.
**Nothing in this proposal changes that.** No runtime HTTP fetch path
into the registry. No "load skill from URL." The only thing that ever
gets parsed into a `Skill` is a directory tree on the local
filesystem.

What this proposal adds is the layer **underneath** the load directory:
a sync agent that keeps that directory in step with one or more git
repositories and calls `SkillRegistry::reload()` after each
successful sync.

```
        GitHub repo(s)          ← favai-registry.json says what to track
              │
              │ git clone --depth=1  (on-demand / on-startup / opt-in periodic)
              ▼
   ~/.config/starter/favai/<source>.staging/    ← validate
              │
              │ two-rename swap
              ▼
   ~/.config/starter/favai/sources/<source>/    ← disk: the only source of truth
              │
              │ explicit call after swap completes
              ▼
   SkillRegistry::reload()              ← re-walk every load_dir_quarantined
              │
              │ broadcast::Sender<ReloadEvent>
              ▼
   ToolRegistry rebuild (skills-as-mcp-tools.md)
```

## Two distinct problems, kept separate

These got muddled in the initial sketch and the scope is cleaner with
them split:

### Problem 1 — sync my own favorites across 2 PCs

Just `git clone` a (possibly private) repo into a known directory,
`starter-skills` loads from it, the agent re-syncs on demand or at
startup, and an explicit `SkillRegistry::reload()` fires after each
successful sync.

No registry. No discovery. No third-party trust model. This is the
trivial base case and ships first.

### Problem 2 — share with friends / pull from public favorites repos

This is where a *registry* matters: a list of known sources, each
pointing at a public (or private) repo, each carrying a trust
classification. Anything pulled from one of these is **always
quarantined** when it lands in the registry — same R-skills-3
guarantee that already governs contributed skills.

This is the larger work and lands after Problem 1.

## The favai repo layout

A favai repo is just a directory tree of `SKILL.md` bundles, with one
manifest at the root:

```
favai-pack.toml          ← repo-level metadata
README.md
skills/
  ship-it-check/
    SKILL.md
    resources/
      checklist.md
  pr-review/
    SKILL.md
  safe-refactor/
    SKILL.md
```

`favai-pack.toml`:

```toml
[pack]
name        = "nubedev/favai"
description = "Personal AI favorites — workflows for shipping Rust + TS."
maintainer  = "ap@nube-io.com"
license     = "MIT"
version     = "0.3.0"
```

**`version`** is recorded on the approval row when an operator
approves a bundle from this pack, so `favai list` can show "approved
at pack v0.3.0" — without that, the field is purely informational
and not worth carrying. Requires extending `ApprovalRow` with an
optional `pack_version: Option<String>`; non-breaking addition.

The first draft had a `[trust]` section with `declared = "personal"
| "team" | "public"`. Removed: the new trust model ignores
pack-author claims entirely (see Trust model below). A field whose
value the consumer ignores is worse than no field — operators read
it and assume it does something. If a v2 registry ever needs to
classify packs, that classification lives in the registry's
`registry.json`, not in the pack itself.

No special skill format inside. Each `SKILL.md` is exactly what
`starter-skills` already parses — same frontmatter, same body, same
`deny_unknown_fields`. Anything else is a non-goal.

## The local sync state

```
~/.config/starter/favai/
  config.toml              ← which sources to track
  sources/
    nubedev-favai/         ← git clone of github.com/NubeDev/favai
      .git/
      favai-pack.toml
      skills/...
    alice-rust-skills/     ← git clone of a friend's repo
      .git/
      favai-pack.toml
      skills/...
```

No `state.json`. The first draft had one; on review it was a cache
of `git rev-parse HEAD` per source, which is cheap to re-derive on
startup. Persisting it adds a "what if the file is truncated"
failure mode for no real benefit. Last-fetch timestamp lives in
memory only — if it matters across restarts later, revisit then.

`config.toml`:

```toml
[[source]]
name        = "nubedev-favai"
url         = "https://github.com/NubeDev/favai.git"
branch      = "main"
skills_path = "skills"          # subdir within the repo to load from
# No load_mode field. Every synced source is quarantined-on-load.
# Approval is per-bundle, per-hash, recorded once per machine.

[[source]]
name        = "alice-rust-skills"
url         = "https://github.com/alice/rust-skills.git"
branch      = "main"
skills_path = "skills"
```

**Every synced source goes through `load_dir_quarantined`. There is no
`load_mode` knob.** This is a deliberate change from the first draft,
made after peer review surfaced that `load_dir(...)` honours
frontmatter `trust: approved` without consulting `ApprovalStore`
(see [registry.rs lines 552–573](/home/user/code/rust/starter/crates/starter-skills/src/registry.rs#L552-L573)).
Allowing a `load_mode = "approved"` path for "personal" sources would
mean a `git pull` could bring in a new commit whose bundles ship with
`trust: approved` in frontmatter and become live with **no approval
row check at all**. That contradicts the safety claim further down the
doc, so the path is removed.

The cost is one approval click per bundle per machine on first use,
even for my own repo. That is the right cost: it makes "did I review
this commit on this machine" an explicit yes, not a frontmatter
assertion the loader silently honours.

`skills_path` is optional; if omitted, defaults to `"skills"`. The
real safety net is the resolved-path-exists check at sync validation
(step 2.iii of the agent flow below), not the schema. The loader is
pointed at `<source_root>/<skills_path>`, not the repo root — because
`SkillRegistry::walk_load_dir` only scans **one level deep** for
`SKILL.md`-containing directories
(see [registry.rs:479](/home/user/code/rust/starter/crates/starter-skills/src/registry.rs#L479)).
Pointing it at the repo root would silently skip every bundle nested
under `skills/`.

## The new crate: `starter-favai`

One crate, added to the starter workspace at
`/home/user/code/rust/starter/crates/starter-favai/`, doing four things:

1. **Read** `config.toml` and produce a list of `Source` structs.
   Validate each `name` as a path-safe slug (`[a-z0-9_-]+`, no `.`,
   no `/`) and reject anything else at parse time.
2. **Sync — fresh-clone staging swap.** For each source:
   1. **Precondition:** if `<name>.staging/` exists from a prior
      failed sync, remove it (`std::fs::remove_dir_all`). The
      staging dir is always fresh at the start of a sync, so no
      `git clean` is needed — there is nothing for it to clean.
   2. `git clone --depth=1 --branch <branch> -- <url>
      <name>.staging/`. Shallow clone because two-way sync and
      history are explicitly out of scope (see Out of Scope §4);
      pulling full history wastes bytes per sync.
   3. Validate the staging dir: `favai-pack.toml` parses,
      `<skills_path>` exists, every `SKILL.md` inside it parses.
      If validation fails, leave the staging dir in place for
      operator inspection and return an error — do **not** touch
      the live `<name>/`.
   4. **Two-rename swap.** This is not atomic across both renames;
      crash recovery is documented below.
      a. `rename(<name>/, <name>.old/)`
      b. `rename(<name>.staging/, <name>/)`
      c. `remove_dir_all(<name>.old/)`
   5. Call `SkillRegistry::reload()` exactly once after step 4.c.
      No mutex needed: there is no watcher (see step 3).

   **Crash recovery rule**, applied at agent startup before any sync:
   - `<name>/` exists, `<name>.old/` doesn't, `<name>.staging/` doesn't
     → normal state, do nothing.
   - `<name>.staging/` exists, `<name>/` doesn't → crash between 4.a
     and 4.b. Finish the swap: rename staging → live.
   - `<name>.old/` exists → crash between 4.b and 4.c. Remove the
     leftover `.old` dir.
   - `<name>.staging/` exists and `<name>/` exists → crash before
     4.a, or a fresh sync hadn't reached the swap. Remove staging.

   On Linux a single atomic swap is available via `renameat2` with
   `RENAME_EXCHANGE`; out of scope for v1 (portability cost), but
   worth knowing. The crash-recovery rule above is portable and
   correct without it.
3. **No filesystem watcher.** The first draft proposed one. After
   review it earns no keep: sync-driven reloads come from step 2.v
   explicitly; the only other reload trigger would be the operator
   hand-editing a bundle under `~/.config/starter/favai/sources/`,
   which Out of Scope §5 already documents as discarded on next
   sync. Supporting an anti-pattern with a watcher (plus its
   debouncer, plus the mutex coordinating it with sync, plus the
   event storm the two-rename swap would generate) is complexity
   that defends a workflow the doc tells operators not to use.

   Dev workflow for editing bundles: edit in a **separate** git
   checkout of the favai repo (outside the sources cache),
   `git push`, then `favai sync <name>` on the consuming machine.
   The cache is read-only by convention; sync is the only writer.
4. **Translate config → builder** — every source contributes one
   `load_dir_quarantined(<source_root>/<skills_path>)` call.
   There is no `load_dir` path. The builder rejects any source whose
   `<source_root>/<skills_path>` does not canonicalize to a path
   under `~/.config/starter/favai/sources/`.

Public surface, roughly:

```rust
pub struct FavaiConfig { /* parsed config.toml */ }
pub struct FavaiAgent  { /* holds per-source sync task handles + reload broadcaster */ }

/// Fires after every successful sync-driven reload. Carries which
/// bundles changed so the MCP skills bridge can do an incremental
/// ToolRegistry update rather than a full rebuild. Without an event
/// API the diagram's "ToolRegistry rebuild" arrow has nothing behind
/// it; without the per-bundle diff the bridge does O(all skills) work
/// on every 15-min sync.
///
/// v1 the bridge ignores `added`/`removed`/`changed_hash` and does a
/// full rebuild — but the surface is frozen now so v2 can switch to
/// incremental updates without a breaking change.
#[derive(Debug, Clone)]
pub struct ReloadEvent {
    pub source:         String,                // source name that synced
    pub added:          Vec<SkillId>,          // bundles newly present
    pub removed:        Vec<SkillId>,          // bundles deleted upstream
    pub changed_hash:   Vec<SkillId>,          // same id, new bundle_hash
    pub at:             chrono::DateTime<chrono::Utc>,
}

impl FavaiAgent {
    pub async fn start(
        config: FavaiConfig,
        registry: Arc<SkillRegistry>,
    ) -> Result<Self, FavaiError>;

    pub async fn sync_now(&self, source_name: &str) -> Result<SyncReport, FavaiError>;
    pub fn sources(&self) -> Vec<SourceStatus>;
    pub fn subscribe_reloads(&self) -> tokio::sync::broadcast::Receiver<ReloadEvent>;
    pub async fn shutdown(self);
}

pub fn apply_to_builder(
    config: &FavaiConfig,
    builder: SkillRegistryBuilder,
) -> SkillRegistryBuilder;     // calls .load_dir_quarantined per source (only)
```

`SyncReport` carries: bytes pulled, files changed, new head sha,
duration, whether a reload was triggered.

The MCP skills bridge (companion doc) holds a `broadcast::Receiver<ReloadEvent>`
and, on each event, rebuilds its `ToolRegistry` from
`SkillRegistry::list()`. That is the API behind the "ToolRegistry
rebuild" arrow in the diagram above — without it, sync-driven changes
would land in the skill registry but never reach MCP clients until
process restart.

### Dependencies

- `starter-spi` — error type.
- `starter-skills` — `SkillRegistry`, `SkillRegistryBuilder`.
- `starter-observability` — metrics + tracing (R7).
- *No `notify` dep* — the filesystem watcher was dropped (see step 3
  of the agent flow).
- `tokio` — already pervasive.
- **For the git operation itself**: shell out to `git` on `PATH`.
  Rationale below.

### `git2` vs shelling out to `git`

I considered `git2` (libgit2 bindings). Three reasons to shell out:

1. **Cred handling.** Users already have working `git` config for
   ssh keys, credential helpers, 2FA, deploy tokens. Inheriting that
   environment is one line; reimplementing it through libgit2 is
   weeks.
2. **Binary size.** libgit2 is a substantial dep.
3. **Failure surface.** Users debugging "why didn't my sync work"
   can `cd` into the source dir and run `git status` / `git pull`
   themselves. With `git2` the failure mode is opaque.

Tradeoff: shelling out means a `git` binary on `PATH` is a hard
requirement. Documented in the crate README, validated at agent
startup with a two-step probe:

1. `git --version` — proves the binary is reachable.
2. `git config --get-regexp .` — proves git can read its config
   (catches corrupt `~/.gitconfig`, missing `HOME`, sandboxed CI
   runners). Empty output is tolerated; a non-zero exit is not.

Either failing produces a clear "git is not usable in this
environment" error pointing at the failed command, not a confusing
sync error two screens later.

### Shell-out safety rules

Every `git` invocation must follow these rules — they are checked in
review, not assumed:

- **`Command::new("git")` with arg arrays only.** No `sh -c`, no
  `format!()`-built command strings, no `bash`. Arguments go in as
  `&str` slices.
- **Source `name` is a path-safe slug.** Validated at config parse
  (`^[a-z0-9][a-z0-9_-]{0,63}$`). Used to build the staging and live
  directory names, so a malformed name cannot escape the sources
  root. Reserved names rejected: `state`, `config`, `tmp`, and
  anything starting with `.` — otherwise `[[source]] name = "config"`
  would silently overwrite layout assumptions.
- **Working directory is canonicalized and contained.** Before every
  `git` call the agent canonicalizes the working dir and rejects it
  if it does not have `~/.config/starter/favai/sources/` as a
  prefix. Defends against a symlink or a misconfigured rename.
- **`--` before URL/path positional args.** `git clone -- <url> <dir>`,
  `git fetch -- <remote>`, etc. Closes the "URL that starts with
  `--upload-pack=…`" attack class.
- **URL scheme allowlist.** Accepted: `https://`, `ssh://git@host/path`,
  `git@host:path` (the scp-like ssh form GitHub's UI emits).
  Rejected: `file://`, `ext::`, `git://` (unauthenticated, no
  integrity), anything else. Also rejected: any URL containing
  userinfo (`https://user:token@host/...`) — it embeds a secret in
  `config.toml`; the error message points at git's credential helper
  as the right place to put credentials.
- **Environment scrubbing as a deny-list.** The reviewer pointed out
  that allow-listing env vars produces an endless tail of "why
  doesn't sync work on my Mac/CI" reports — `USER`,
  `XDG_CONFIG_HOME`, platform-specific `SSH_AUTH_SOCK` namespacing,
  etc. Inherit by default; scrub names matching `AWS_*`, `*_TOKEN`,
  `*_SECRET`, `*_KEY`, `*_PASSWORD`. The remaining surface is what
  git needs.

## Why `starter-tool-github` does not extend

`starter-tool-github` is, per its own docs, "create-issue only" and
intentionally narrow (one bearer token with `repo` scope, no
`octocrab` SDK, R1/R4/R8 scope rules). Cloning repos is a different
shape of work: it uses the git wire protocol, not the REST API; it
uses ssh keys or credential helpers, not bearer tokens; and the
authentication surface is wider.

Folding sync into `starter-tool-github` would break its R1 (one
integration per crate) and its R8 (no transitive SDK explosion). The
two crates stay separate. `starter-tool-github` keeps its create-issue
job; `starter-favai` does sync.

**Where they do meet:** the `add-favorite` meta-tool from the
companion doc can optionally call `starter-tool-github` to open a PR
against the user's favai repo when a new favorite is created. That is
a thin compose, not a merge — and it is a later step, not v1.

## Trust model

There is exactly one trust gate: **per-bundle, per-hash approval in
`ApprovalStore`**, recorded by the operator on each machine. The
config file does not classify sources, the frontmatter `trust:` field
is ignored for synced bundles, and there is no way to mark a source
"safe enough to skip approval."

Three properties this gives us:

1. **No source-level escalation.** Every synced source is loaded
   through `load_dir_quarantined`, which forces `Trust::Quarantined`
   regardless of frontmatter
   (see [registry.rs:552–561](/home/user/code/rust/starter/crates/starter-skills/src/registry.rs#L552-L561)).
   A pack author cannot ship `trust: approved` and have it honoured.
2. **Approval drift on reload — really this time.** When sync pulls a
   new commit, every bundle's `bundle_hash` is recomputed. Any bundle
   whose hash no longer matches a recorded approval row stays
   `Quarantined` until the operator approves the new hash. This is
   already how `walk_load_dir` resolves `force_quarantine = true`.
3. **Per-machine approval.** `ApprovalStore` rows are local. Approving
   a bundle on PC A does not approve it on PC B; PC B's operator
   makes the same decision independently. For a single-user two-PC
   setup this is a small annoyance and the right one.

What this costs: an approval click per bundle per machine **per
change**, because the bundle hash changes on every edit. For a pack
of 20 bundles where I tweak one per day across 2 PCs, that's one
click per day per PC — fine. For "I refactored a shared resource
included by 8 bundles," it's 8 clicks per PC.

Mitigation deferred to v2: an `approve --source <name> --from-sha
<sha>` command that approves every currently-quarantined bundle in a
source whose **previous** hash was already approved at `<sha>`. Still
per-machine, still explicit, but collapses the bulk-edit case into
one decision per machine. v1 ships without it — the dogfood load
(one personal pack, two PCs) does not justify the approval-store
schema change yet.

What this buys: the safety claim is now true as stated — *a sync
cannot make an unreviewed change live, ever, on any source.*

The first draft of this doc had a `load_mode = "approved"` escape
hatch for "personal" sources. Peer review pointed out that path would
silently honour frontmatter `trust: approved` via `load_dir`'s
non-quarantine branch, defeating the whole approval model. The
escape hatch is gone.

## Public registry / discovery

Out of scope for v1, but worth scoping the shape so v1 doesn't
preclude it.

A "registry" in the discovery sense is just a static JSON file at a
well-known URL listing known packs:

```json
{
  "registry_version": 1,
  "packs": [
    {
      "name": "nubedev/favai",
      "url":  "https://github.com/NubeDev/favai.git",
      "description": "Personal AI favorites — workflows for shipping Rust + TS.",
      "maintainer": "ap@nube-io.com",
      "category": ["rust", "review"]   // free-form for v1, likely to become tag soup; v2 may controlled-vocab this
    }
  ]
}
```

A `favai search <term>` CLI would fetch one or more such JSON files
(the user configures which registries to trust — defaults to a single
NubeDev-hosted one) and grep them. `favai add <name>` resolves the
name through configured registries, writes a new `[[source]]` block,
triggers a sync.

The registry file itself does not need to live in `favai`. It can be
a separate `favai-registry` repo whose only contents are
`registry.json` and a CI job that validates it. Anyone can fork it
and add themselves; consumers point their config at whichever
registry fork(s) they trust. No central server, no auth, no API.

This is a v2 problem and the v1 design (per-source config, manual
URL entry) handles the bilateral-share case ("my friend Alice sent me
her repo URL") perfectly well without any of it.

## Out of scope explicitly

1. **Templating in favai packs.** Same R-skills-1 rule — bodies are
   literal text. A pack cannot interpolate variables, environment, or
   per-user values.
2. **Binary resources, secrets, .env files in packs.** A pack is text
   bundles only. The bundle loader already rejects anything outside
   the supported resource schemes.
3. **Inter-pack dependencies.** A favorite cannot `require` another
   favorite. Bundles are leaf nodes. If two favorites share content,
   they duplicate it. (This is the same trade `starter-skills` already
   makes.)
4. **Two-way sync.** Sync is one-way: GitHub → disk. Edits to local
   bundles are not pushed back. The user owns publishing via normal
   `git commit && git push` in the cloned source dir, or via the PR
   compose path mentioned under `starter-tool-github`.
5. **Conflict resolution.** The staging-swap sync model (clone fresh,
   `git clean -ffdx`, atomic rename) means **any** local edit under
   `~/.config/starter/favai/sources/<name>/` is discarded on next
   sync, including untracked files. These directories are caches,
   not workspaces. Documented loudly.

## What ships, in what order

Each step is small enough to land on its own and useful by itself.

**Step 1 — sync (no registry, no discovery).**
- `starter-favai` crate with `FavaiConfig`, `FavaiAgent`,
  `apply_to_builder`, `subscribe_reloads`.
- Two-step git probe at startup (`git --version` +
  `git config --get-regexp .`).
- Fresh-clone staging swap per source: `rm -rf staging`, shallow
  clone, validate, two-rename swap, explicit
  `SkillRegistry::reload()`.
- Crash-recovery sweep at startup applies the rule documented in
  step 2 of the agent flow.
- One source supported in v1 config: my own private favorites repo
  cloned across two PCs. Validates the whole machinery.

**Step 2 — multi-source.**
- Multiple `[[source]]` blocks. (All sources are quarantine-on-load;
  there is no trust matrix in config — see the Trust model section.)
- `SyncReport` and `sources()` introspection.
- `subscribe_reloads()` + the MCP skills bridge consuming reload
  events to rebuild its `ToolRegistry`.
- Documented "approval drift on reload" behaviour with a test that
  sync-pulls a modified bundle and asserts: (a) the registry sees
  the bundle as quarantined, (b) the MCP bridge has dropped the
  tool from `tools/list`, (c) re-approving the new hash brings it
  back.

**Step 3 — CLI ergonomics.**
- `favai add <url>` writes a new `[[source]]` block.
- `favai sync [<name>]` triggers an out-of-band pull.
- `favai list` shows sources, last-fetch ts, head sha, skill count.

**Step 4 — discovery (v2, deferred).**
- `favai-registry` repo with `registry.json` schema.
- `favai search`, `favai add <name>` against configured registries.
- Optional PR compose via `starter-tool-github` from the
  `add-favorite` meta-tool.

## Resolved during peer review

These were open in the first draft and the rewrite settled them:

- **Personal vs hash-approved.** Settled: every synced source is
  hash-approved per-machine. No `load_mode` knob. The convenience
  of "my own repo just works" was incompatible with the safety
  claim and the safety claim won.
- **One source or multi-source in v1.** Settled: config schema is
  multi-source from day one (the parser already needs to handle
  validation of slugs, schemes, paths), but Step 1 only dogfoods one
  source — my own private favai repo across two PCs.

## Still open

- **Sync cadence default.** Settled after review: **on-demand +
  on-startup**, periodic opt-in only. Always-polling is the wrong
  default for a tool the operator runs interactively, and a fleet of
  PCs polling `:00 :15 :30 :45` produces a sawtooth at GitHub. When
  periodic sync is enabled, jitter the interval ±10% so a fleet
  spreads out.
- **Private repos.** v1 assumes the user's existing `git` config
  works for the URLs in `[[source]]`. We document this; we do not
  build credential management. Phase 2 question: should we offer a
  `favai login` that wraps `gh auth login` for the GitHub case
  specifically?
- **Where does `favai` the binary live?** Proposal: a thin
  `crates/favai-cli` binary on top of `starter-favai`, mirroring the
  `examples/gh-report` shape. Not v1; the library is enough for the
  starter to wire in.
- **What lives in the `NubeDev/favai` repo itself?** Proposal: the
  first three favorites — `ship-it-check`, `pr-review`,
  `safe-refactor` — as real `SKILL.md` bundles, plus a
  `favai-pack.toml`. That repo doubles as the v1 dogfood target and
  the example referenced from this doc.
