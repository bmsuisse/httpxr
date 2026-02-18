# Migration from httpx

httpxr is a **drop-in replacement** for httpx. In most cases, migration is a
single import change.

## Step 1: Install

```bash
pip install httpxr
```

## Step 2: Replace Imports

```diff
- import httpx
+ import httpxr
```

Or, for an even more seamless transition:

```python
import httpxr as httpx  # Everything works as before
```

## Step 3: That's It

The full httpx API is supported:

- ✅ `Client` / `AsyncClient`
- ✅ `Request` / `Response`
- ✅ `URL`, `Headers`, `QueryParams`, `Cookies`
- ✅ `Timeout`, `Limits`, `Proxy`
- ✅ `BasicAuth`, `DigestAuth`, `FunctionAuth`, `NetRCAuth`
- ✅ `MockTransport`, `ASGITransport`, `WSGITransport`
- ✅ Authentication flows, redirects, streaming, event hooks
- ✅ HTTP/1.1 & HTTP/2, SOCKS proxy
- ✅ `codes` status code constants
- ✅ Full exception hierarchy
- ✅ CLI via `httpxr` command

---

## Zero-Effort Migration with `httpxr.compat`

Don't want to change any imports? Use the **compatibility shim** to redirect
all `import httpx` calls — including from third-party libraries — to httpxr:

```python
import httpxr.compat   # add this one line

import httpx            # now resolves to httpxr 🚀
```

This is great for trying httpxr in an existing project with zero risk.
See the [full compat docs](compat.md) for details.

---

## Behavioral Differences

There are a few minor behavioral differences to be aware of. None of these
should affect real-world usage:

| Difference | Detail | Why it's OK |
| :--- | :--- | :--- |
| Header ordering | Default headers may be sent in a different order | Headers are unordered per RFC 9110 §5.3 |
| User-Agent | `python-httpxr/...` instead of `python-httpx/...` | Reflects the actual client identity |
| Logger name | `"httpxr"` instead of `"httpx"` | Logs should identify the actual library |
| Transport layer | Rust (reqwest/tokio) instead of Python (httpcore) | Same HTTP semantics, faster execution |

---

## Zero Dependencies

Unlike httpx, httpxr has **no runtime Python dependencies**:

| httpx dependency | httpxr equivalent |
|:---|:---|
| `httpcore` | Rust reqwest transport |
| `certifi` | Rust native-tls / rustls |
| `anyio` / `sniffio` | Rust tokio runtime |
| `idna` | Rust IDNA encoding |
| `h2` (optional) | Rust reqwest HTTP/2 |
| `brotli` (optional) | Rust native brotli |
| `zstandard` (optional) | Rust native zstd |

---

## Bonus: httpxr Extensions

After migrating, you gain access to features not available in httpx:

- [**`gather()`**](extensions.md#gather) — Concurrent batch requests via Rust's tokio
- [**`paginate()`**](extensions.md#paginate) — Auto-follow pagination links
- [**Raw API**](extensions.md#raw-api) — Maximum-speed dispatch bypassing Response construction
