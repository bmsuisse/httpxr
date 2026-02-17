# How It Was Built

!!! info "🤖 100% AI-Generated"
    Every line of Rust, Python, and configuration in httpxr was autonomously written, debugged, and tested by an AI coding agent. No human wrote any code.

## The Goal

Build a **production-grade, httpx-compatible HTTP client** backed by Rust — and do it entirely with AI. The agent was given two clear objectives:

1. **Pass every original httpx test** — full behavioral compatibility, no shortcuts
2. **Beat httpx in benchmarks** — achieve state-of-the-art performance

The agent iterated on these two goals until both were achieved.

---

## Phase 1: Correctness — Pass All httpx Tests

The first priority was **correctness, not speed**. The complete httpx test suite (1300+ tests across 30+ modules) served as the specification.

The AI agent worked through each test module — `test_client.py`, `test_async_client.py`, `test_models.py`, `test_urls.py`, `test_content.py`, and many more — porting the expected behavior into Rust via [PyO3](https://pyo3.rs/). Each iteration followed the same loop:

```
┌─────────────────────────────────────────────┐
│  1. Run pytest against the httpx test suite │
│  2. Read the failures                       │
│  3. Fix the Rust implementation             │
│  4. Rebuild with maturin                    │
│  5. Repeat until green                      │
└─────────────────────────────────────────────┘
```

This phase covered the full API surface:

- **Clients** — `Client`, `AsyncClient`, connection pooling, auth flows, redirects, cookies, event hooks, proxy routing, transport mounts
- **Models** — `Request`, `Response`, `URL`, `Headers`, `QueryParams`, `Cookies`
- **Transports** — `MockTransport`, `ASGITransport`, `WSGITransport`
- **Streaming** — sync and async byte/text/line iterators with proper resource cleanup
- **Error handling** — the full exception hierarchy (`TimeoutException`, `ConnectError`, `TooManyRedirects`, etc.)
- **Edge cases** — multipart encoding, digest auth, SOCKS proxies, HTTP/2, international domain names

The result: **all 1303 tests pass**, with only [6 minor behavioral differences](https://github.com/bmsuisse/httpxr#behavioral-differences) documented (e.g., header ordering, which is unordered per RFC 9110).

---

## Phase 2: Performance — Beat the Benchmarks

With correctness locked in, the agent shifted focus to **performance**. A benchmark suite was set up comparing httpxr against 9 other Python HTTP libraries:

- httpx, niquests, aiohttp, urllib3, curl_cffi (Python-based)
- httpr, pyreqwest, ry, rnet (Rust-based)

Three scenarios were measured: **single GET**, **50 sequential GETs**, and **50 concurrent GETs** — all against a local ASGI server, 100 rounds each.

The agent then iterated on the Rust transport layer:

```
┌──────────────────────────────────────────────┐
│  1. Run the full benchmark suite             │
│  2. Profile the hot path (Python → Rust)     │
│  3. Identify bottlenecks                     │
│  4. Optimize (reduce allocations, GIL time,  │
│     batch operations, connection reuse)      │
│  5. Verify tests still pass                  │
│  6. Repeat until state-of-the-art            │
└──────────────────────────────────────────────┘
```

Key optimizations the agent discovered and applied:

| Optimization | Impact |
| :--- | :--- |
| Release GIL during network I/O | Enabled true concurrent HTTP |
| Connection pooling via reqwest | Eliminated per-request TLS handshakes |
| Minimize Python ↔ Rust boundary crossings | Reduced overhead per request |
| Batch header construction in Rust | Avoided per-header Python calls |
| Raw API bypass (`get_raw()`, etc.) | Near-zero overhead for latency-critical code |
| Tokio runtime reuse | Amortized async runtime startup |

---

## The Result

After dozens of iterations across both phases, httpxr achieved both goals:

- ✅ **1303/1303 httpx tests passing** — full API compatibility
- ✅ **2.4× faster** than httpx for sequential workloads
- ✅ **13× faster** than httpx under concurrency
- ✅ **#1 fastest** HTTP client under concurrency across all 10 libraries tested
- ✅ **Zero Python dependencies** — HTTP, TLS, compression, SOCKS, IDNA all in Rust

The iterative approach — correctness first, performance second, verify both continuously — produced a client that is both **fully compatible** and **genuinely fast**.

---

## Technology Stack

The AI agent selected and integrated:

| Layer | Technology | Why |
| :--- | :--- | :--- |
| Python bindings | [PyO3](https://pyo3.rs/) | Mature, zero-copy Python ↔ Rust bridge |
| Async HTTP | [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs/) | Production-proven async HTTP + runtime |
| Sync HTTP | [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs/) | Unified transport for both sync and async |
| TLS | rustls + native-tls | Fast default with platform TLS fallback |
| Compression | gzip, brotli, zstd, deflate | All handled natively in Rust |
| Build system | [maturin](https://github.com/PyO3/maturin) | Standard Rust → Python wheel builder |
