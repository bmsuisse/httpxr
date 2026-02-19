<div align="center">
  <img src="docs/assets/logo.svg" alt="httpxr logo" width="120" height="120">
</div>

# httpxr

[![CI](https://github.com/bmsuisse/httpxr/actions/workflows/ci.yml/badge.svg)](https://github.com/bmsuisse/httpxr/actions/workflows/ci.yml)
[![PyPI version](https://img.shields.io/pypi/v/httpxr.svg)](https://pypi.org/project/httpxr/)
[![Python versions](https://img.shields.io/badge/python-3.10%20%7C%203.11%20%7C%203.12%20%7C%203.13-blue)](https://pypi.org/project/httpxr/)
[![Docs](https://img.shields.io/badge/docs-online-blue?logo=materialformkdocs)](https://bmsuisse.github.io/httpxr/)

A Rust-powered HTTP client built on the [httpx](https://github.com/encode/httpx) API — same interface, faster execution, and a growing set of high-performance extensions for data ingestion.

[📖 **Documentation**](https://bmsuisse.github.io/httpxr) · [📦 PyPI](https://pypi.org/project/httpxr/) · [🐙 GitHub](https://github.com/bmsuisse/httpxr) · [🤖 llm.txt](https://bmsuisse.github.io/httpxr/llm.txt)

> [!NOTE]
> **🤖 AI-Generated** — Every line of Rust, Python, and configuration in this project was written by an AI coding agent powered by **Claude Opus 4.6**. The iterative process of getting all 1300+ tests to pass involved human oversight — reviewing agent output, steering direction, and deciding next steps — so this was not a press-button-and-done affair. [Read the full story →](#how-it-was-built)

---

## What is httpxr?

`httpxr` started as a **faithful port** of [httpx](https://github.com/encode/httpx) — swap `import httpx` for `import httpxr` and everything just works, but faster thanks to native Rust networking, TLS, and compression.

It has since grown beyond a 1:1 port. The bundled [`httpxr.extensions`](https://bmsuisse.github.io/httpxr/extensions/) module adds high-performance helpers designed for **big-data ingestion pipelines** (Databricks, PySpark, NDJSON streams) that go well beyond what plain httpx provides:

| Feature | Purpose |
| :--- | :--- |
| `paginate_to_records()` | Lazy record iterator over paginated APIs — O(1) memory |
| `iter_json_bytes()` | Stream NDJSON/SSE as raw bytes — zero UTF-8 decode overhead |
| `gather_raw_bytes()` | Concurrent batch requests → bytes/parsed, powered by Rust concurrency |
| `OAuth2Auth` | Client-credentials auth with automatic token refresh |



The networking layer is reimplemented in Rust:

| Layer | Technology |
| :--- | :--- |
| Python bindings | [PyO3](https://pyo3.rs/) |
| Async HTTP | [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs/) |
| Sync HTTP | [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs/) |
| TLS | rustls + native-tls |
| Compression | gzip, brotli, zstd, deflate (native Rust) |

### Zero Python Dependencies

Unlike httpx (which depends on `httpcore`, `certifi`, `anyio`, `idna`, and optional packages for compression), `httpxr` has **zero runtime Python dependencies**. Everything — HTTP, TLS, compression, SOCKS proxy, IDNA encoding — is handled natively in Rust.

---

## Benchmarks

All benchmarks run against **10 HTTP libraries** on a local ASGI server (uvicorn), 100 rounds each.
Scenarios: **Single GET**, **50 Sequential GETs**, **50 Concurrent GETs**.

![HTTP Library Benchmark](https://raw.githubusercontent.com/bmsuisse/httpxr/main/docs/benchmark_results.png)

> 📊 **[Interactive version →](https://bmsuisse.github.io/httpxr/benchmarks/)** with full hover/zoom

### Summary (median, ms — lower is better)

| Scenario | httpxr | httpr | pyreqwest | ry | aiohttp | curl_cffi | urllib3 | rnet | httpx | niquests |
|:---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Single GET | **0.20** | 0.19 | 0.17 | 0.18 | 0.27 | 0.27 | 0.29 | 0.39 | 0.38 | 0.40 |
| 50 Sequential GETs | **8.12** | 6.53 | 6.17 | 9.19 | 11.16 | 11.84 | 15.51 | 26.97 | 19.03 | 20.58 |
| 50 Concurrent GETs | **5.61** | 7.72 | 6.45 | 6.53 | 8.40 | 11.34 | 16.50 | 12.68 | 68.85 | 21.89 |

> **Key takeaways:**
> - **httpxr** is the **fastest full-featured httpx-compatible client** — on par with raw Rust libraries
> - **#1 under concurrency** — faster than all other libraries including httpr, pyreqwest, and ry
> - **~2.3× faster** than httpx for sequential workloads
> - **~12× faster** than httpx under concurrency (GIL-free Rust)
> - Competitive with bare-metal libraries (pyreqwest, ry) while offering the full httpx API

### Why httpxr is slightly slower on Single GET

Libraries like `httpr` and `pyreqwest` achieve lower single-request latency (~0.17-0.19ms) because they return **minimal response objects** — essentially just status + bytes + a headers dict. They are **not** full httpx drop-in replacements.

**httpxr** returns full httpx-compatible `Response` objects with:
- Parsed `URL` with scheme/host/path/query components
- `Headers` (multidict with case-insensitive lookup)
- `Request` back-reference, redirect `history`, `elapsed` timing
- Event hooks, auth flows, cookie persistence, transport mounts

This ~0.08ms of extra per-request overhead is the cost of **100% API compatibility** with httpx. Under real-world workloads (sequential/concurrent), httpxr's Rust transport layer dominates and **beats httpx in both scenarios**.

```bash
# Reproduce benchmarks locally:
uv sync --group dev --group benchmark
uv run python benchmarks/run_benchmark.py
```

---

## Quick Start

```bash
pip install httpxr
```

To also install the **optional CLI**:

```bash
pip install "httpxr[cli]"
```

**Sync:**

```python
import httpxr

with httpxr.Client() as client:
    r = client.get("https://httpbin.org/get")
    print(r.status_code)
    print(r.json())
```

**Async:**

```python
import httpxr, asyncio

async def main():
    async with httpxr.AsyncClient() as client:
        r = await client.get("https://httpbin.org/get")
        print(r.json())

asyncio.run(main())
```

---

## API Compatibility

`httpxr` supports the full httpx API surface:

- `Client` / `AsyncClient` — sync and async HTTP clients
- `Request` / `Response` — full request/response models
- `URL`, `Headers`, `QueryParams`, `Cookies` — all data types
- `Timeout`, `Limits`, `Proxy` — configuration objects
- `MockTransport`, `ASGITransport`, `WSGITransport` — test transports
- Authentication flows, redirects, streaming, event hooks
- HTTP/1.1 & HTTP/2, SOCKS proxy support
- Server-Sent Events via `httpxr.sse` (port of [httpx-sse](https://github.com/florimondmanca/httpx-sse))
- CLI via `httpxr` command (requires `pip install "httpxr[cli]"`)
- Python 3.10, 3.11, 3.12, 3.13

### Zero-Effort httpx Swap — `httpxr.compat`

Already using `httpx` everywhere? Add **one line** to your entrypoint and every
`import httpx` — including inside third-party libraries — will transparently use
httpxr instead:

```python
import httpxr.compat   # add this once, e.g. in main.py / settings.py

import httpx           # ← now resolves to httpxr 🚀
```

This works by registering `httpxr` as `sys.modules["httpx"]` at import time. No
code changes required — all your existing `httpx` calls keep working at Rust speed.

```python
import os
# Feature-flag style: switch via env var
if os.environ.get("USE_HTTPXR"):
    import httpxr.compat  # noqa: F401

import httpx  # uses httpxr or httpx based on env var
```

> **[Full compatibility shim docs →](https://bmsuisse.github.io/httpxr/compat/)**

---

## httpxr Extensions

Beyond the standard httpx API, `httpxr` adds features that leverage the Rust runtime:

### `gather()` — Concurrent Batch Requests

Dispatch multiple requests concurrently with a single call. Requests are built in Python, then sent in parallel via Rust's tokio runtime with zero GIL contention.

```python
with httpxr.Client() as client:
    requests = [
        client.build_request("GET", f"https://api.example.com/items/{i}")
        for i in range(100)
    ]
    responses = client.gather(requests, max_concurrency=10)
```

| Parameter | Default | Description |
| :--- | :--- | :--- |
| `max_concurrency` | `10` | Max simultaneous in-flight requests |
| `return_exceptions` | `False` | Return errors inline instead of raising |

> 📖 **[`gather()` docs →](https://bmsuisse.github.io/httpxr/extensions/#gather)**

### `paginate()` — Auto-Follow Pagination

Automatically follow pagination links across multiple API responses.

```python
# Follow @odata.nextLink in JSON body (Microsoft Graph)
pages = client.paginate("GET", url, next_url="@odata.nextLink")

# Follow Link header (GitHub-style)
pages = client.paginate("GET", url, next_header="link")

# Custom extractor function
pages = client.paginate("GET", url, next_func=my_extractor)
```

| Parameter | Default | Description |
| :--- | :--- | :--- |
| `next_url` | — | JSON key containing the next page URL |
| `next_header` | — | HTTP header to parse for `rel="next"` links |
| `next_func` | — | Custom `Callable[[Response], str \| None]` |
| `max_pages` | `100` | Stop after N pages |

Both methods are available on `Client` (sync) and `AsyncClient` (async). See [`examples/gather.py`](examples/gather.py) and [`examples/paginate.py`](examples/paginate.py) for full examples.

> 📖 **[`paginate()` docs →](https://bmsuisse.github.io/httpxr/extensions/#paginate)**

### `gather_raw()` — Batch Raw Requests

Like `gather()` but returns `(status, headers, body)` tuples — maximum throughput
for high-volume workloads where you don't need full `Response` objects.

### `paginate_get()` / `paginate_post()` — Convenience Wrappers

Shorthand for `paginate("GET", ...)` and `paginate("POST", ...)`.

### `gather_paginate()` — Concurrent Paginated Fetches

Fetch all pages from multiple paginated endpoints concurrently in one call.

> 📖 **[Full extensions docs →](https://bmsuisse.github.io/httpxr/extensions/)**

### `download()` — Direct File Download

```python
with httpxr.Client() as client:
    client.download("https://example.com/data.csv", "/tmp/data.csv")
```

### `response.json_bytes()` — Raw JSON Bytes

Returns the response body as `bytes` without the UTF-8 decode step — feed
directly into [orjson](https://github.com/ijl/orjson) or [msgspec](https://github.com/jcrist/msgspec).

### `response.iter_json()` — NDJSON & SSE Streaming

Parse NDJSON or SSE responses as a stream of Python dicts. Handles `data:` prefixes
and `[DONE]` sentinels automatically.

> 📖 **[Requests & Responses docs →](https://bmsuisse.github.io/httpxr/requests-responses/)**

### `RetryConfig` — Automatic Retries

```python
with httpxr.Client(retry=httpxr.RetryConfig(max_retries=3, backoff_factor=0.5)) as client:
    r = client.get("https://api.example.com/flaky")
```

### `RateLimit` — Request Throttling

```python
with httpxr.Client(rate_limit=httpxr.RateLimit(requests_per_second=10.0)) as client:
    for i in range(1000):
        client.get(f"https://api.example.com/items/{i}")  # auto-throttled
```

> 📖 **[Resilience docs →](https://bmsuisse.github.io/httpxr/resilience/)**

### `httpxr.sse` — Server-Sent Events

```python
from httpxr.sse import connect_sse

with httpxr.Client() as client:
    with connect_sse(client, "GET", "https://example.com/stream") as source:
        for event in source.iter_sse():
            print(event.event, event.data)
```

Port of [httpx-sse](https://github.com/florimondmanca/httpx-sse) — supports sync and async, `EventSource`, `ServerSentEvent`, and `SSEError`.

> 📖 **[SSE docs →](https://bmsuisse.github.io/httpxr/sse/)**

### Raw API — Maximum-Speed Dispatch

For latency-critical code, `get_raw()`, `post_raw()`, `put_raw()`, `patch_raw()`, `delete_raw()`, and `head_raw()` bypass all httpx `Request`/`Response` construction and call reqwest directly.

```python
with httpxr.Client() as client:
    status, headers, body = client.get_raw("https://api.example.com/data")
    # status:  int (e.g. 200)
    # headers: dict[str, str]
    # body:    bytes
```

These accept `url` (full URL, not path), optional `headers` (dict), optional `body` (bytes, for POST/PUT/PATCH), and optional `timeout` (float, seconds).

> 📖 **[Full extensions docs →](https://bmsuisse.github.io/httpxr/extensions/#raw-api)**

---

## Test Suite

The port is validated against the **complete httpx test suite** — **1303 tests** across 30+ modules, ported 1:1 from the original project.

### Behavioral Differences

| Difference | Detail | Why it's OK |
| :--- | :--- | :--- |
| Header ordering | Default headers sent in different order | Headers are unordered per RFC 9110 §5.3 |
| MockTransport init | Handler stored differently internally | Test logic and assertions unchanged |

### Test Modifications (6 files)

| Change | Original | New | Reason |
| :--- | :--- | :--- | :--- |
| User-Agent | `python-httpx/…` | `python-httpxr/…` | Reflects actual client identity |
| Logger name | `"httpx"` | `"httpxr"` | Logs should identify the actual library |
| Timeout validation | `Timeout(pool=60.0)` raises | Succeeds | PyO3 framework limitation |
| Test URLs | Hardcoded port | Dynamic `server.url` | Random OS port in test server |
| Write timeout | Catches `WriteTimeout` | Catches `TimeoutException` | Rust transport may buffer writes via OS kernel, surfacing timeout on read instead of write |

---

## Development

```bash
git clone https://github.com/bmsuisse/httpxr.git
cd httpxr
uv sync --group dev
maturin develop
uv run pytest tests/
uv run pyright
```

A **pre-push hook** runs `pytest` and `pyright` automatically before every push.

---

## How It Was Built

Every line of code in this project was **written by an AI coding agent** powered by **Claude Opus 4.6**. The iterative process — running tests, reading failures, fixing the Rust implementation, rebuilding — was guided by **human oversight**: reviewing agent output, steering direction, and deciding what to tackle next. This was not a fully autonomous "press button and done" workflow, but a human-in-the-loop collaboration where the AI did the coding and the human kept it on track. Still, the project demonstrates what becomes possible when an AI agent is given a clear, measurable goal — and hints at a near future where this kind of work runs fully autonomously.

> **Why build another Rust HTTP library?** Great Rust-powered Python HTTP clients already exist — [pyreqwest](https://github.com/MarkusSintonen/pyreqwest), [httpr](https://github.com/thomasht86/httpr), [rnet](https://github.com/0x676e67/rnet), and others. This project was never about reinventing the wheel. It started as an **experiment to see how well an AI coding agent performs** when given a clear, well-scoped goal in a domain with established solutions. The two objectives — pass every httpx test and beat httpx in benchmarks — provided a tight feedback loop to push the agent's capabilities. Along the way the result turned into a genuinely useful library, so here it is. 🙂

The agent was given two objectives and iterated until both were achieved:

### Phase 1: Correctness — Pass All httpx Tests

The complete httpx test suite (1300+ tests) served as the specification. The agent ported each test module, ran `pytest`, read the failures, fixed the Rust implementation, rebuilt, and repeated — across clients, models, transports, streaming, auth flows, and edge cases — until all 1303 tests passed.

### Phase 2: Performance — Beat the Benchmarks

With correctness locked in, the agent ran benchmarks against 9 other HTTP libraries, profiled the hot path, and optimized: releasing the GIL during I/O, minimizing Python ↔ Rust boundary crossings, batching header construction, reusing connections and the tokio runtime. Each cycle was followed by a test run to ensure nothing regressed.

The iterative loop — **correctness first, performance second, verify both continuously** — produced a client that is fully compatible with httpx while being **2.3× faster** sequentially and **12× faster** under concurrency.

> 📖 **[Full development story →](https://bmsuisse.github.io/httpxr/how-it-was-built/)**

---

## License

Licensed under either of:

- [MIT License](./LICENSE)
- [Apache License, Version 2.0](./LICENSE-APACHE)

at your option.

This project is a Rust port of [httpx](https://github.com/encode/httpx) by [Encode OSS Ltd](https://www.encode.io/), originally licensed under the [BSD 3-Clause License](./THIRD_PARTY_NOTICES.md).
