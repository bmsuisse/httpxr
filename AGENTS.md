# httpxr Agent Guidelines

## Project Overview
httpxr is a 1:1 Rust port of httpx for performance. It uses:
- **Rust** (via PyO3 + maturin) for the core HTTP client
- **Python** (3.14+) for the public API surface
- **uv** for Python package management
- **reqwest** for async HTTP, **ureq** for sync HTTP (transport layer)

## Critical Rules

### Always Use `uv`
- **All Python commands MUST use `uv run`** — never use `.venv/bin/python` or bare `python`
- Tests: `uv run pytest tests/ -x -q`
- Benchmarks: `uv run python benchmarks/run_benchmark.py`
- Type checking: `uv run pyright`
- Smoke test: `uv run python -c "import httpxr; print(httpxr.__version__)"`

### Build Workflow
1. `cargo check` — fast compilation check
2. `maturin develop --release` — build optimized `.so`
3. Copy `.so` to in-tree location (stale `.so` issue):
   ```bash
   cp .venv/lib/python3.14/site-packages/_httpxr/_httpxr.cpython-314-darwin.so httpxr/_httpxr.cpython-314-darwin.so
   ```

### Pre-Commit Checklist
1. `cargo check` — no Rust errors
2. `maturin develop --release` + copy `.so`
3. `uv run pytest tests/ -x -q` — all tests pass
4. `uv run pyright` — type checking passes

### File Size Limit
- No file should exceed 1000 lines of code

### Testing
- Always run tests before committing
- Run e2e tests after implementing new features
- Create a branch for each feature

## Releasing a New Version

The CI automatically builds wheels for all platforms and publishes to PyPI when a **git tag** is pushed.

### Steps
1. **Bump version** in `Cargo.toml` (the `version` field under `[package]`)
2. **Commit** the version bump: `git commit -am "release: v0.X.Y"`
3. **Tag** the commit: `git tag v0.X.Y`
4. **Push** the tag: `git push origin main && git push origin v0.X.Y`
5. CI will build all wheels (linux, musllinux, macos, windows) and publish to PyPI

### How it works
- The `ci.yml` workflow builds wheels for all targets on every push
- The **Publish to PyPI** step only runs when `github.ref` starts with `refs/tags/`
- It uses the `PYPI_API_TOKEN` secret stored in the GitHub repo settings
- The tag name should match the version in `Cargo.toml` (e.g. `v0.28.1`)

### Verify before releasing
```bash
cargo check
uv run maturin develop --release
uv run pytest tests/ -x -q
uv run pyright
```

## Architecture Notes
- Sync client: `src/client/sync_client.rs` — has ultra-fast path for simple GETs
- Async client: `src/client/async_client.rs` — has ultra-fast path in `get()`
- Transport layer: `src/transports/default.rs`
- Models (Request, Response, Headers): `src/models.rs`
