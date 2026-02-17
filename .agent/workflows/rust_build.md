---
description: Build the Rust extension with maturin
---

# Rust Build

// turbo-all

1. Ensure pip is installed in the venv (maturin needs it):
```bash
uv pip install pip
```

2. Build the Rust extension:
```bash
maturin develop
```

3. Verify the build produced the .so with a **recent timestamp**:
```bash
ls -la httpr/_httpr*.so
```

4. Quick smoke test:
```bash
uv run python -c "import httpr; print(httpr.__version__)"
```

## Known Issue: Stale .so

`maturin develop` installs via pip into the venv, but Python resolves the
in-tree `httpr/_httpr.cpython-*-darwin.so` first (since the package is
installed in editable/develop mode from the project root). If `maturin develop`
doesn't update the in-tree `.so` timestamp, the old binary is loaded and your
Rust changes won't take effect.

**Fix:** Check `ls -la httpr/_httpr*.so` after building — the timestamp must
match the current time. If it's stale, re-run `maturin develop` (it should
overwrite the in-tree file). If it still doesn't update, try:
```bash
rm httpr/_httpr*.so && maturin develop
```
