---
name: safe-refactor
description: Refactor code while keeping behaviour identical — one verb per file, ≤400 lines.
trust: quarantined
---

Given a file or module to refactor:
1. Identify distinct verb responsibilities.
2. Propose a split plan (folder-of-verbs layout).
3. Move one verb per commit, run `cargo check` after each.
4. Co-locate tests with each new file.
5. Delete any dead code uncovered by the split.

Do not change behaviour. If a behaviour change is needed, open a separate PR.
