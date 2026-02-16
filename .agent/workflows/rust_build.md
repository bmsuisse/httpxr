---
description: Build the Rust extension with maturin
---

# Rust Build

// turbo-all

1. Build with Arrow features enabled:
```bash
maturin develop --features arrow
```

2. Verify the build produced the .so:
```bash
ls -la httpr/_httpr*.so
```

3. Quick smoke test:
```bash
uv run python -c "import httpr; print(httpr.__version__)"
```
