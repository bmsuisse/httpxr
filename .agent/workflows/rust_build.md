---
description: Build the Rust extension with maturin
---

# Rust Build

// turbo-all

1. Build the Rust extension:
```bash
maturin develop
```

2. Verify the build produced the .so:
```bash
ls -la httpr/_httpr*.so
```

3. Quick smoke test:
```bash
uv run python -c "import httpr; print(httpr.__version__)"
```
