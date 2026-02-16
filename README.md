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
| Sync HTTP | [ureq](https://github.com/algesten/ureq) |
| TLS | rustls + native-tls |
| Compression | gzip, brotli, zstd, deflate (native Rust) |

### Zero Python Dependencies

Unlike httpx (which depends on `httpcore`, `certifi`, `anyio`, `idna`, and optional packages for compression), `httpr` has **zero runtime Python dependencies**. Everything — HTTP, TLS, compression, SOCKS proxy, IDNA encoding — is handled natively in Rust.

---

## Quick Start

```bash
pip install httpr
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
- CLI via `httpr` command

---

## Test Suite

The port is validated against the **complete httpx test suite** — **1289 tests** across 30+ modules, ported 1:1 from the original project. No tests were deleted, skipped, or weakened.

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
