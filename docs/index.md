---
hide:
  - navigation
  - toc
---

<div class="hero" markdown>

# ⚡ httpxr

**A 1:1 Rust port of [httpx](https://github.com/encode/httpx) — same API, faster execution.**

Swap `import httpx` for `import httpxr` and everything just works,
with the performance of native Rust networking.

<div class="hero-buttons">
  <a href="quickstart/" class="btn-primary">Get Started</a>
  <a href="https://github.com/bmsuisse/httpxr" class="btn-secondary">View on GitHub</a>
  <a href="llm.txt" class="btn-secondary">llm.txt</a>
</div>

</div>

---

<div class="feature-grid" markdown>

<div class="feature-card" markdown>

### :material-lightning-bolt: Blazing Fast

2.3× faster than httpx sequentially, **12× faster** under concurrency.
The Rust transport layer releases the GIL for true parallel HTTP.

</div>

<div class="feature-card" markdown>

### :material-swap-horizontal: Drop-In Replacement

100% httpx API compatible. `Client`, `AsyncClient`, `Response`, `Headers`,
auth flows, streaming, event hooks — it's all there.

</div>

<div class="feature-card" markdown>

### :material-package-variant-closed: Zero Dependencies

No `httpcore`, `certifi`, `anyio`, or `idna`. HTTP, TLS, compression,
SOCKS proxy, and IDNA — all handled natively in Rust.

</div>

<div class="feature-card" markdown>

### :material-language-rust: Rust-Powered

Built with [PyO3](https://pyo3.rs/), [reqwest](https://github.com/seanmonstar/reqwest),
and [tokio](https://tokio.rs/). Native gzip, brotli, zstd, and deflate compression.

</div>

<div class="feature-card" markdown>

### :material-rocket-launch: Exclusive Extensions

`gather()` for concurrent batch requests, `paginate()` for auto-pagination,
and raw API methods for maximum-speed dispatch.

</div>

<div class="feature-card" markdown>

### :material-test-tube: Battle Tested

Validated against the **complete httpx test suite** — 1300+ tests ported
1:1 from the original project. [AI-built with human oversight](how-it-was-built.md) via an iterative agent loop.

</div>

</div>

---

## Quick Install

```bash
pip install httpxr
```

## Hello World

=== "Sync"

    ```python
    import httpxr

    with httpxr.Client() as client:
        r = client.get("https://httpbin.org/get")
        print(r.status_code)  # 200
        print(r.json())
    ```

=== "Async"

    ```python
    import httpxr
    import asyncio

    async def main():
        async with httpxr.AsyncClient() as client:
            r = await client.get("https://httpbin.org/get")
            print(r.json())

    asyncio.run(main())
    ```

---

## Benchmarks

All benchmarks run against **10 HTTP libraries** on a local ASGI server, 100 rounds each.

### Single GET

<iframe src="benchmark_single.html" width="100%" height="360" frameborder="0"></iframe>

### 50 Sequential GETs

<iframe src="benchmark_sequential.html" width="100%" height="360" frameborder="0"></iframe>

### 50 Concurrent GETs

<iframe src="benchmark_concurrent.html" width="100%" height="360" frameborder="0"></iframe>

| Scenario | httpxr | httpr | pyreqwest | ry | aiohttp | curl_cffi | urllib3 | rnet | httpx | niquests |
|:---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Single GET | **0.20** | 0.12 | 0.10 | 0.18 | 0.24 | 0.23 | 0.30 | 0.34 | 0.38 | 0.39 |
| 50 Sequential GETs | **7.84** | 6.52 | 6.33 | 8.98 | 10.73 | 12.91 | 15.17 | 17.76 | 18.78 | 19.65 |
| 50 Concurrent GETs | **5.23** | 7.31 | 6.56 | 6.23 | 7.85 | 12.31 | 16.26 | 10.15 | 70.23 | 21.14 |

!!! tip "Key takeaways"
    - **#1 under concurrency** — faster than all other libraries
    - **~2.3× faster** than httpx for sequential workloads
    - **~12× faster** than httpx under concurrency (GIL-free Rust)
    - Competitive with bare-metal libraries while offering the full httpx API

---

## Technology Stack

| Layer | Technology |
| :--- | :--- |
| Python bindings | [PyO3](https://pyo3.rs/) |
| Async HTTP | [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs/) |
| Sync HTTP | [reqwest](https://github.com/seanmonstar/reqwest) + [tokio](https://tokio.rs/) |
| TLS | rustls + native-tls |
| Compression | gzip, brotli, zstd, deflate (native Rust) |
