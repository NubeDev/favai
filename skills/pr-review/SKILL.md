---
name: pr-review
description: Review a pull request for correctness, file-size rule compliance, and safety.
trust: quarantined
---

Given a PR number or a `git diff`, review for:
1. Logic correctness and obvious bugs.
2. File-size rule: flag any file approaching 300 lines, block above 400.
3. Security: no command injection, no secrets in code, no SQL injection.
4. Test coverage: new behaviour should have a test.

Return a structured report: blocking issues, non-blocking notes, and a
recommended verdict (approve / request-changes / needs-discussion).
