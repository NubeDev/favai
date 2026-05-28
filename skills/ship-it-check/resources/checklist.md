# Ship-it checklist

- [ ] `cargo check` passes with no warnings
- [ ] `cargo test` passes
- [ ] No file exceeds 400 lines
- [ ] New public items have doc comments
- [ ] CHANGELOG updated if user-visible change
- [ ] No `.unwrap()` on user-supplied input
- [ ] No secrets committed (check with `git diff --name-only HEAD~1`)
