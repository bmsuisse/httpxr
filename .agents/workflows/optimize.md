---
description: Research → Build → Test → Benchmark → Repeat (httpxr performance optimisation loop)
---

# Research → Build → Test → Benchmark → Repeat

This is the core AI-driven optimisation loop for httpxr.
Each iteration identifies a bottleneck, implements a Rust fix, validates correctness, and measures the gain.

> **Rule:** never skip the Test step. A faster but broken HTTP client is worthless.

---

## 🔍 Step 1 — Research

Get a baseline and identify the hottest path to improve next.

Run the benchmark suite first to see where time is spent:
```
uv run pytest tests/benchmarks/ -v --benchmark-only --benchmark-sort=mean 2>&1 | head -60
```

For function-level profiling, use `py-spy` on a representative workload:
```
uv run py-spy record -o profile.svg -- python -c "
import httpxr
import asyncio

async def bench():
    async with httpxr.AsyncClient() as c:
        for _ in range(1000):
            r = await c.get('http://httpbin.org/get')

asyncio.run(bench())
"
```

Identify the specific Rust function or Python↔Rust boundary causing overhead.
Document the hypothesis: _"the bottleneck is X because Y"_.

Check the architecture notes before diving in:
- **Sync fast path**: `src/client/sync_client.rs` — ultra-fast path for simple GETs
- **Async fast path**: `src/client/async_client.rs` — ultra-fast path in `get()`
- **Transport layer**: `src/transports/default.rs`
- **Models**: `src/models.rs`
- **Common helpers**: `src/client/common.rs`

---

## 🦀 Step 2 — Build

Implement the fix in Rust and rebuild the extension module.

Make your Rust changes in `src/`, then build in release mode:
// turbo
```
uv run maturin develop --release
```

Copy the freshly built `.so` into the in-tree location (avoids stale-module issues):
// turbo
```
cp .venv/lib/python3.14/site-packages/_httpxr/_httpxr.cpython-314-darwin.so httpxr/_httpxr.cpython-314-darwin.so
```

Check that the Rust code compiles cleanly with no warnings:
// turbo
```
cargo check 2>&1
```

---

## ✅ Step 3 — Test

**All tests must pass before proceeding. No exceptions.**

// turbo
```
uv run pytest tests/ -x -q
```

If any test fails, go back to Step 2 and fix the regression before moving on.

Also run pyright to catch any type errors introduced in Python stubs or wrappers:
// turbo
```
uv run pyright
```

Also verify the smoke test still works:
// turbo
```
uv run python -c "import httpxr; print(httpxr.__version__)"
```

---

## 📊 Step 4 — Benchmark

Run the full benchmark suite and compare against the saved baseline.

```
uv run pytest tests/benchmarks/ -v --benchmark-only --benchmark-compare --benchmark-sort=mean
```

Key metrics to track per function/path (`sync GET`, `async GET`, `fast path`, `headers`, `streaming`):

| Metric | Target |
|--------|--------|
| Mean time | ≤ previous iteration mean |
| Min time | competitive with httpx |
| Throughput (req/s) | ≥ httpx baseline |

Save results for the next iteration comparison:
```
uv run pytest tests/benchmarks/ -v --benchmark-only --benchmark-save=iteration_N
```

Interpret the results:
- **Better** → document the gain with a `perf:` conventional commit, pick the next bottleneck → Step 5.
- **Worse / no change** → revisit the approach in Step 1.

---

## 🔁 Step 5 — Repeat

Pick the next biggest gap from the benchmark output and go back to Step 1.

Useful questions to guide the next Research phase:
- Which code path is still slower than httpx?
- Is the bottleneck in Rust logic, the Python↔Rust boundary (PyO3 overhead), or string/bytes allocation?
- Can the async fast-path in `get()` be extended to other methods (`post`, `put`)?
- Are we paying for unnecessary header copying or URL parsing on every call?
- Can connection pooling be tightened to reduce handshake overhead?
- Is `reqwest` (async) or `ureq` (sync) being given optimal connection pool settings?

---

## Quick-reference commands

| Command | Purpose |
|---------|---------|
| `uv run maturin develop --release` | Rebuild Rust extension (optimised) |
| `cargo check` | Fast compile check without linking |
| `uv run pytest tests/ -x -q` | Full test suite, stop on first failure |
| `uv run pyright` | Type-check Python stubs |
| `uv run ruff format .` | Format Python before committing |
| `uv run pytest tests/benchmarks/ -v --benchmark-only` | Full benchmark run |
| `uv run pytest tests/benchmarks/ -k get --benchmark-only` | Benchmark a single path |
| `uv run pytest tests/benchmarks/ --benchmark-compare` | Compare vs saved baseline |
| `uv run pytest tests/benchmarks/ --benchmark-save=iteration_N` | Save results for next round |
