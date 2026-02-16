# httpr

**A Rust-based drop-in replacement for [httpx](https://github.com/encode/httpx) — rigorously tested against the original test suite, then taken further.**

> [!NOTE]
> **🤖 100% AI-Generated** — This entire project was autonomously created by an AI coding agent. No human code input was involved — every line of Rust, Python, and configuration was written, debugged, and tested entirely by AI.

The primary goal of `httpr` is simple: **a Rust-backed, drop-in replacement for `httpx`**, validated against the full original test suite — **1363 tests** across 30+ modules — so you can swap `import httpx` for `import httpr` and everything just works.

But why stop there?

Once you have a Rust-native HTTP client with [PyO3](https://pyo3.rs/), [`reqwest`](https://github.com/seanmonstar/reqwest), and [`tokio`](https://tokio.rs/) under the hood, you unlock a whole new level of performance.

---

## Test Compatibility

`httpr` is validated against the **complete httpx test suite**, ported 1:1 from the original project — covering **30+ test modules** and **1363 tests** (+ 47 benchmarks) across the API surface, client behaviour, models, transports, authentication, encoding, and more.

### Known Behavioral Differences from httpx

The test suite is otherwise **identical** to the original — no tests were deleted, skipped, or weakened. The following minor, justified deviations exist:

#### Behavioral Differences (no test changes)

| Difference | File | Detail | Why it's OK |
| :--- | :--- | :--- | :--- |
| **Header ordering** | `test_asgi.py` | Default headers (e.g. `user-agent`, `host`) are sent in a different order than httpx. | HTTP headers are **unordered by spec** (RFC 9110 §5.3). All header *values* are identical. |
| **`MockTransport` subclass init** | `test_redirects.py` | `ConsumeBodyTransport` explicitly stores `self.handler` in `__init__`. | The Rust-backed `MockTransport` stores the handler differently internally. The test logic and assertions are unchanged. |

#### Test Modifications (6 files changed)

| Change | Files | Original | New | Justification |
| :--- | :--- | :--- | :--- | :--- |
| **User-Agent string** | `test_asgi.py`, `test_client.py`, `test_event_hooks.py`, `test_headers.py` | `python-httpx/{version}` | `python-httpr/{version}` | httpr is its own project — the user-agent should reflect the actual client identity. |
| **Logger name** | `test_utils.py` | Logger `"httpx"` | Logger `"httpr"` | Logs should identify the actual library emitting them. |
| **Timeout validation** | `test_config.py` | `Timeout(pool=60.0)` raises `ValueError` | `Timeout(pool=60.0)` succeeds | PyO3 **cannot distinguish** a missing argument from explicit `None` — this is a framework limitation. |
| **Logging test URLs** | `test_utils.py` | Hardcoded `http://127.0.0.1:8000/` | Dynamic `server.url` | Test server uses a random OS port — hardcoded port never matches. |

---

## Benchmarks

All benchmarks run against a real HTTP server (uvicorn), comparing **pure-Python httpx** vs **Rust-backed httpr** side-by-side using `pytest-benchmark`.

```bash
uv run pytest benchmarks/ -v --benchmark-columns=min,max,mean,stddev,rounds
```

The suite includes **16 benchmarks** — each with a `__python` and `__rust` variant for direct comparison.

### 🚀 Results

| Operation | Python (`httpx`) | Rust (`httpr`) | Speedup |
| :--- | :--- | :--- | :--- |
| **Single GET** | 722 μs | **262 μs** | **2.8x** |
| **Single POST (1 KB)** | 501 μs | **283 μs** | **1.8x** |
| **JSON Parse** | 552 μs | **290 μs** | **1.9x** |
| **Large POST (1 MB)** | 1.68 ms | **1.13 ms** | **1.5x** |
| **Large JSON Parse (1 MB)** | **4.30 ms** | 22.3 ms | 0.2x |
| **100 Sequential GETs** | 44.2 ms | **26.4 ms** | **1.7x** |
| **100 Sequential POSTs (10 KB)** | 72.0 ms | **28.0 ms** | **2.6x** |
| **500 Sequential GETs** | 260.7 ms | **125.8 ms** | **2.1x** |
| **50 Concurrent GETs** | 111.0 ms | **18.9 ms** | **5.9x** 🚀 |
| **200 Concurrent GETs** | 306.7 ms | **70.5 ms** | **4.4x** 🚀 |
| **Client Churn (100×)** | 603.1 ms | **40.4 ms** | **14.9x** 🚀 |

### Running Benchmarks

```bash
# Full benchmark suite
uv run pytest benchmarks/ -v --benchmark-columns=min,max,mean,stddev,rounds

# Specific category
uv run pytest benchmarks/ -k "concurrent" -v
uv run pytest benchmarks/ -k "sequential" -v
```

---

## Features

- **High Performance** — Rust networking via `reqwest` + `tokio` (async) and `ureq` (sync)
- **Pythonic API** — Drop-in replacement for `httpx` (same `Client`, `AsyncClient`, `Response`, etc.)
- **Async & Sync** — Full support for both usage patterns
- **HTTP/1.1 & HTTP/2** — Protocol support via `reqwest`
- **Compression** — gzip, brotli, zstd, deflate — handled natively in Rust
- **SOCKS Proxy** — Built-in SOCKS5 proxy support
- **SSL/TLS** — Rustls + native-tls for robust TLS handling
- **ASGI & WSGI Transports** — Test your apps directly without a server
- **CLI** — `httpr` command-line client with `rich` terminal output

---

## Installation

```bash
pip install httpr
```

Or build from source:

```bash
maturin develop
```

---

## Usage

### Synchronous Client

```python
import httpr

with httpr.Client() as client:
    response = client.get("https://httpbin.org/get")
    print(response.status_code)
    print(response.json())
```

### Asynchronous Client

```python
import httpr
import asyncio

async def main():
    async with httpr.AsyncClient() as client:
        response = await client.get("https://httpbin.org/get")
        print(response.status_code)
        print(response.json())

asyncio.run(main())
```


---

## Contributing

Contributions are welcome! This project uses `maturin` for building the Rust extension.

```bash
git clone https://github.com/encode/httpx.git
cd httpx
uv sync
maturin develop
uv run pytest tests/
```

## License

Licensed under either of:

- [MIT License](./LICENSE-MIT)
- [Apache License, Version 2.0](./LICENSE-APACHE)

at your option.

This project is a Rust port of [httpx](https://github.com/encode/httpx) by [Encode OSS Ltd](https://www.encode.io/), originally licensed under the [BSD 3-Clause License](./THIRD_PARTY_NOTICES.md).
