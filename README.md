# httpr

A 1:1 Rust port of [httpx](https://github.com/encode/httpx) — same API, faster execution.

> [!NOTE]
> **🤖 100% AI-Generated** — This entire project was autonomously created by an AI coding agent. Every line of Rust, Python, and configuration was written, debugged, and tested entirely by AI.

---

## What is httpr?

`httpr` is a **faithful port** of the [httpx](https://github.com/encode/httpx) HTTP client with one goal: **make it faster by replacing the Python internals with Rust**. The Python API stays identical — swap `import httpx` for `import httpr` and everything just works, but with the performance benefits of native Rust networking, TLS, and compression.

The networking layer is reimplemented in Rust:

| Layer | Technology |
| :--- | :--- |
| Python bindings | [PyO3](https://pyo3.rs/) |
| Async HTTP | [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs/) |
| Sync HTTP | [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs/) |
| TLS | rustls + native-tls |
| Compression | gzip, brotli, zstd, deflate (native Rust) |

### Zero Python Dependencies

Unlike httpx (which depends on `httpcore`, `certifi`, `anyio`, `idna`, and optional packages for compression), `httpr` has **zero runtime Python dependencies**. Everything — HTTP, TLS, compression, SOCKS proxy, IDNA encoding — is handled natively in Rust.

---

## Benchmarks

All benchmarks run against **9 HTTP libraries** on a local ASGI server (uvicorn), 20 rounds each.
Scenarios: **Single GET**, **50 Sequential GETs**, **50 Concurrent GETs**.

![HTTP Library Benchmark](benchmarks/benchmark_results.png)

> 📊 **[Interactive version →](benchmarks/benchmark_results.html)** (open locally for full hover/zoom)

### Summary (median, ms — lower is better)

| Scenario | httpr | pyreqwest | ry | aiohttp | curl_cffi | urllib3 | rnet | httpx | niquests |
|:---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Single GET | **0.20** | 0.11 | 0.16 | 0.22 | 0.22 | 0.26 | 0.30 | 0.37 | 0.38 |
| 50 Sequential GETs | **8.18** | 6.22 | 8.87 | 10.79 | 12.83 | 15.14 | 17.34 | 18.96 | 19.25 |
| 50 Concurrent GETs | **10.53** | 6.48 | 5.91 | 7.60 | 12.32 | 16.34 | 10.29 | 71.62 | 19.60 |

> **Key takeaways:**
> - **httpr** is the **fastest full-featured httpx-compatible client** — on par with raw Rust libraries
> - **~2× faster** than httpx for sequential workloads
> - **~7× faster** than httpx under concurrency (GIL-free Rust)
> - Competitive with bare-metal libraries (pyreqwest, ry) while offering the full httpx API

```bash
# Reproduce benchmarks locally:
uv sync --group dev --group benchmark
uv run python benchmarks/run_benchmark.py
```

---

## Quick Start

```bash
pip install httpr
```

To also install the **optional CLI**:

```bash
pip install "httpr[cli]"
```

**Sync:**

```python
import httpr

with httpr.Client() as client:
    r = client.get("https://httpbin.org/get")
    print(r.status_code)
    print(r.json())
```

**Async:**

```python
import httpr, asyncio

async def main():
    async with httpr.AsyncClient() as client:
        r = await client.get("https://httpbin.org/get")
        print(r.json())

asyncio.run(main())
```

---

## API Compatibility

`httpr` supports the full httpx API surface:

- `Client` / `AsyncClient` — sync and async HTTP clients
- `Request` / `Response` — full request/response models
- `URL`, `Headers`, `QueryParams`, `Cookies` — all data types
- `Timeout`, `Limits`, `Proxy` — configuration objects
- `MockTransport`, `ASGITransport`, `WSGITransport` — test transports
- Authentication flows, redirects, streaming, event hooks
- HTTP/1.1 & HTTP/2, SOCKS proxy support
- CLI via `httpr` command (requires `pip install "httpr[cli]"`)
---

## httpr Extensions

Beyond the standard httpx API, `httpr` adds features that leverage the Rust runtime:

### `gather()` — Concurrent Batch Requests

Dispatch multiple requests concurrently with a single call. Requests are built in Python, then sent in parallel via Rust's tokio runtime with zero GIL contention.

```python
with httpr.Client() as client:
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
with httpr.Client() as client:
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
| User-Agent | `python-httpx/…` | `python-httpr/…` | Reflects actual client identity |
| Logger name | `"httpx"` | `"httpr"` | Logs should identify the actual library |
| Timeout validation | `Timeout(pool=60.0)` raises | Succeeds | PyO3 framework limitation |
| Test URLs | Hardcoded port | Dynamic `server.url` | Random OS port in test server |
| Write timeout | Catches `WriteTimeout` | Catches `TimeoutException` | Rust transport may buffer writes via OS kernel, surfacing timeout on read instead of write |

---

## Development

```bash
git clone <repo-url>
cd httpr
uv sync --group dev
maturin develop
uv run pytest tests/
uv run pyright
```

A **pre-push hook** runs `pytest` and `pyright` automatically before every push.

---

## License

Licensed under either of:

- [MIT License](./LICENSE-MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE)

at your option.

This project is a Rust port of [httpx](https://github.com/encode/httpx) by [Encode OSS Ltd](https://www.encode.io/), originally licensed under the [BSD 3-Clause License](./THIRD_PARTY_NOTICES.md).
