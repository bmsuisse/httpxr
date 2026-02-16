---
description: Pre-commit verification — build, test, type-check
---

# Safe Commit Workflow

// turbo-all

1. Build the Rust extension:
```bash
maturin develop --features arrow
```

2. Run the test suite:
```bash
uv run pytest tests/ -x -q
```

3. Type-check Python:
```bash
uv run pyright
```

4. Check Rust compiles clean:
```bash
cargo check
```

5. If all pass, commit:
```bash
git add -A && git commit -m "<your message>"
```
