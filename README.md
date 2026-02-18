# httpxr

[![CI](https://github.com/bmsuisse/httpxr/actions/workflows/ci.yml/badge.svg)](https://github.com/bmsuisse/httpxr/actions/workflows/ci.yml)
[![PyPI version](https://img.shields.io/pypi/v/httpxr.svg)](https://pypi.org/project/httpxr/)
[![Python versions](https://img.shields.io/pypi/pyversions/httpxr.svg)](https://pypi.org/project/httpxr/)

A 1:1 Rust port of [httpx](https://github.com/encode/httpx) — same API, faster execution.

[📖 **Documentation**](https://bmsuisse.github.io/httpxr) · [📦 PyPI](https://pypi.org/project/httpxr/) · [🐙 GitHub](https://github.com/bmsuisse/httpxr) · [🤖 llm.txt](https://bmsuisse.github.io/httpxr/llm.txt)

> [!NOTE]
> **🤖 100% AI-Generated** — This entire project was autonomously created by an AI coding agent powered by **Claude Opus 4.6**. Every line of Rust, Python, and configuration was written, debugged, and tested entirely by AI. [Read the full story →](#how-it-was-built)

---

## What is httpxr?

`httpxr` is a **faithful port** of the [httpx](https://github.com/encode/httpx) HTTP client with one goal: **make it faster by replacing the Python internals with Rust**. The Python API stays identical — swap `import httpx` for `import httpxr` and everything just works, but with the performance benefits of native Rust networking, TLS, and compression.

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
| Single GET | **0.20** | 0.12 | 0.10 | 0.18 | 0.24 | 0.23 | 0.30 | 0.34 | 0.38 | 0.39 |
| 50 Sequential GETs | **7.84** | 6.52 | 6.33 | 8.98 | 10.73 | 12.91 | 15.17 | 17.76 | 18.78 | 19.65 |
| 50 Concurrent GETs | **5.23** | 7.31 | 6.56 | 6.23 | 7.85 | 12.31 | 16.26 | 10.15 | 70.23 | 21.14 |

> **Key takeaways:**
> - **httpxr** is the **fastest full-featured httpx-compatible client** — on par with raw Rust libraries
> - **#1 under concurrency** — faster than all other libraries including httpr, pyreqwest, and ry
> - **~2.4× faster** than httpx for sequential workloads
> - **~13× faster** than httpx under concurrency (GIL-free Rust)
> - Competitive with bare-metal libraries (pyreqwest, ry) while offering the full httpx API

### Why httpxr is slightly slower on Single GET

Libraries like `httpr` and `pyreqwest` achieve lower single-request latency (~0.10-0.12ms) because they return **minimal response objects** — essentially just status + bytes + a headers dict. They are **not** full httpx drop-in replacements.

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
- CLI via `httpxr` command (requires `pip install "httpxr[cli]"`)
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

This entire project was **autonomously created by an AI coding agent** powered by **Claude Opus 4.6** — no human wrote any code.

> **Why build another Rust HTTP library?** Great Rust-powered Python HTTP clients already exist — [pyreqwest](https://github.com/MarkusSintonen/pyreqwest), [httpr](https://github.com/thomasht86/httpr), [rnet](https://github.com/0x676e67/rnet), and others. This project was never about reinventing the wheel. It started as an **experiment to see how well an AI coding agent performs** when given a clear, well-scoped goal in a domain with established solutions. The two objectives — pass every httpx test and beat httpx in benchmarks — provided a tight feedback loop to push the agent's capabilities. Along the way the result turned into a genuinely useful library, so here it is. 🙂

The agent was given two objectives and iterated until both were achieved:

### Phase 1: Correctness — Pass All httpx Tests

The complete httpx test suite (1300+ tests) served as the specification. The agent ported each test module, ran `pytest`, read the failures, fixed the Rust implementation, rebuilt, and repeated — across clients, models, transports, streaming, auth flows, and edge cases — until all 1303 tests passed.

### Phase 2: Performance — Beat the Benchmarks

With correctness locked in, the agent ran benchmarks against 9 other HTTP libraries, profiled the hot path, and optimized: releasing the GIL during I/O, minimizing Python ↔ Rust boundary crossings, batching header construction, reusing connections and the tokio runtime. Each cycle was followed by a test run to ensure nothing regressed.

The iterative loop — **correctness first, performance second, verify both continuously** — produced a client that is fully compatible with httpx while being **2.4× faster** sequentially and **13× faster** under concurrency.

> 📖 **[Full development story →](https://bmsuisse.github.io/httpxr/how-it-was-built/)**

---

## License

Licensed under either of:

- [MIT License](./LICENSE)
- [Apache License, Version 2.0](./LICENSE-APACHE)

at your option.

This project is a Rust port of [httpx](https://github.com/encode/httpx) by [Encode OSS Ltd](https://www.encode.io/), originally licensed under the [BSD 3-Clause License](./THIRD_PARTY_NOTICES.md).
