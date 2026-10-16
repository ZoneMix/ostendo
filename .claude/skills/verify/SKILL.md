---
name: verify
description: Run full pre-commit verification — format check, clippy, and tests. Use before committing or when you want to confirm the build is clean.
---

Run all three checks sequentially, stopping on first failure:

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Report results as pass/fail per check. If any check fails, show the relevant output and stop.
