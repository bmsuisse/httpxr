# Extensions

These features are **exclusive to httpxr** — not available in httpx.
They leverage the Rust runtime for performance that's impossible in pure Python.

!!! tip "When to use what"

    | Need | Solution | API |
    |:---|:---|:---|
    | Fetch 100 URLs in parallel | `gather()` | Full `Response` objects |
    | Batch raw requests (max speed) | `gather_raw()` | `(status, headers, body)` tuples |
    | Batch requests → bytes/parsed | `gather_raw_bytes()` | `list[bytes | Any]` |
    | Auto-follow pagination | `paginate()` / `paginate_get()` | Lazy iterator of pages |
    | Paginate + yield records | `paginate_to_records()` | Individual records, O(1) memory |
    | Paginate + gather concurrently | `gather_paginate()` | All pages at once |
    | Minimum latency per request | Raw API | `(status, headers, body)` tuple |
    | Download file to disk | `download()` | One-liner file save |
    | Raw JSON bytes for fast parsers | `json_bytes()` | `bytes` |
    | Parse NDJSON / SSE streams | `iter_json()` | Iterator of dicts |
    | Stream NDJSON as raw bytes | `iter_json_bytes()` | Iterator of `bytes` |
    | Server-Sent Events | `httpxr.sse` | `EventSource` |
    | OAuth2 client credentials | `OAuth2Auth` | Auto-refresh auth |
    | Automatic retries | `RetryConfig` | Exponential backoff |
    | Request throttling | `RateLimit` | Token bucket |
    | Connection pool introspection | `pool_status()` | Pool metrics dict |
    | Standard HTTP calls | `Client.get()` etc. | Full httpx API |

---

## gather() — Concurrent Batch Requests { #gather }

Dispatch multiple requests concurrently with a single call. Requests are built
in Python, then sent in parallel via Rust's tokio runtime with zero GIL contention.

=== "Sync"

    ```python
    import httpxr

    with httpxr.Client() as client:
        requests = [
            client.build_request("GET", f"https://api.example.com/items/{i}")
            for i in range(100)
        ]
        responses = client.gather(requests, max_concurrency=10)

        for resp in responses:
            print(resp.status_code)
    ```

=== "Async"

    ```python
    import httpxr
    import asyncio

    async def main():
        async with httpxr.AsyncClient() as client:
            requests = [
                client.build_request("GET", f"https://api.example.com/items/{i}")
                for i in range(100)
            ]
            responses = await client.gather(requests, max_concurrency=10)

    asyncio.run(main())
    ```

### Parameters

| Parameter | Type | Default | Description |
|:---|:---|:---|:---|
| `requests` | `list[Request]` | required | List of requests to send |
| `max_concurrency` | `int` | `10` | Max simultaneous in-flight requests |
| `return_exceptions` | `bool` | `False` | Return errors inline instead of raising |

### Error Handling

```python
# return_exceptions=True: errors are returned in the result list
responses = client.gather(requests, return_exceptions=True)

for resp in responses:
    if isinstance(resp, Exception):
        print(f"Failed: {resp}")
    else:
        print(f"OK: {resp.status_code}")
```

### Mixed Methods

```python
requests = [
    client.build_request("GET", "https://httpbin.org/get"),
    client.build_request("POST", "https://httpbin.org/post", json={"key": "value"}),
    client.build_request("PUT", "https://httpbin.org/put", content=b"data"),
    client.build_request("DELETE", "https://httpbin.org/delete"),
]
responses = client.gather(requests)
```

### Performance

`gather()` is significantly faster than sequential requests because:

1. **GIL release** — All HTTP work happens in Rust, so Python threads aren't blocked
2. **Tokio runtime** — Requests use async I/O under the hood, even from sync Python
3. **Connection pooling** — Connections are shared across concurrent requests

---

## gather_raw() — Concurrent Raw Requests { #gather-raw }

Like `gather()`, but returns raw `(status, headers, body)` tuples instead of
full `Response` objects. Maximum throughput for high-volume workloads.

```python
with httpxr.Client() as client:
    requests = [
        {"method": "GET", "url": f"https://api.example.com/items/{i}"}
        for i in range(1000)
    ]
    results = client.gather_raw(requests, max_concurrency=50)

    for status, headers, body in results:
        if status == 200:
            import json
            data = json.loads(body)
```

!!! note "Trade-off"
    `gather_raw()` sacrifices `Response` objects (no cookies, auth, redirects,
    event hooks) for lower per-request overhead. Use it when throughput matters
    more than API compatibility.

---

## paginate() — Auto-Follow Pagination { #paginate }

Automatically follow pagination links across API responses. Returns a **lazy
iterator** (sync) or **async iterator** (async) — pages are fetched one at a time.

### Strategy 1: JSON Key

Extract the next URL from a JSON field in the response body:

```python
# Follow @odata.nextLink (Microsoft Graph APIs)
for page in client.paginate(
    "GET",
    "https://graph.microsoft.com/v1.0/users",
    next_url="@odata.nextLink",
    max_pages=10,
    headers={"Authorization": "Bearer ..."},
):
    users = page.json()["value"]
    print(f"Got {len(users)} users")
```

### Strategy 2: Link Header

Parse `rel="next"` from HTTP Link headers (GitHub, many REST APIs):

```python
for page in client.paginate(
    "GET",
    "https://api.github.com/repos/python/cpython/issues",
    next_header="link",
    max_pages=5,
):
    issues = page.json()
    print(f"Got {len(issues)} issues")
```

### Strategy 3: Custom Function

Use any logic to extract the next URL:

```python
def get_next(response: httpxr.Response) -> str | None:
    data = response.json()
    return data.get("pagination", {}).get("next")

for page in client.paginate("GET", url, next_func=get_next):
    process(page)
```

### collect() — Get All Pages at Once

```python
pages = client.paginate(
    "GET", url, next_header="link", max_pages=5
).collect()

print(f"Got {len(pages)} pages")
```

### Track Progress

```python
paginator = client.paginate("GET", url, next_header="link", max_pages=10)

for page in paginator:
    print(f"Page {paginator.pages_fetched}: {page.status_code}")
```

### Async Pagination

```python
async with httpxr.AsyncClient() as client:
    async for page in client.paginate(
        "GET", url, next_header="link", max_pages=5
    ):
        items = page.json()
        print(f"Got {len(items)} items")

    # Or collect all:
    pages = await client.paginate("GET", url, next_header="link").collect()
```

### Parameters

| Parameter | Type | Default | Description |
|:---|:---|:---|:---|
| `method` | `str` | required | HTTP method |
| `url` | `str` | required | Starting URL |
| `next_url` | `str` | — | JSON key containing next page URL |
| `next_header` | `str` | — | HTTP header to parse for `rel="next"` |
| `next_func` | `Callable` | — | Custom `(Response) → str | None` |
| `max_pages` | `int` | `100` | Stop after N pages |

!!! note "Exactly one strategy required"
    You must provide exactly one of `next_url`, `next_header`, or `next_func`.

### Convenience Wrappers

`paginate_get()` and `paginate_post()` are shorthand for common cases:

```python
# Equivalent to paginate("GET", url, ...)
for page in client.paginate_get(url, next_url="@odata.nextLink"):
    ...

# POST-based pagination (e.g. GraphQL cursor)
for page in client.paginate_post(url, json={"cursor": None}, next_func=get_cursor):
    ...
```

---

## gather_paginate() — Concurrent Paginated Fetches { #gather-paginate }

Fetch all pages from multiple paginated endpoints concurrently:

```python
endpoints = [
    {"url": "https://api.example.com/users", "next_url": "@odata.nextLink"},
    {"url": "https://api.example.com/orders", "next_url": "@odata.nextLink"},
    {"url": "https://api.example.com/products", "next_url": "@odata.nextLink"},
]

with httpxr.Client() as client:
    all_results = client.gather_paginate(endpoints, max_concurrency=3)
    # Returns list of lists — one list of pages per endpoint
    for pages in all_results:
        for page in pages:
            print(page.json())
```

---

## Raw API — Maximum-Speed Dispatch { #raw-api }

For latency-critical code, the raw API bypasses all httpx `Request`/`Response`
construction and calls reqwest directly. Returns a simple tuple.

```python
with httpxr.Client() as client:
    status, headers, body = client.get_raw("https://api.example.com/data")
    # status:  int (e.g. 200)
    # headers: dict[str, str]
    # body:    bytes
```

### Available Methods

```python
status, headers, body = client.get_raw(url)
status, headers, body = client.post_raw(url, body=b"data")
status, headers, body = client.put_raw(url, body=b"data")
status, headers, body = client.patch_raw(url, body=b"data")
status, headers, body = client.delete_raw(url)
status, headers, body = client.head_raw(url)
```

### Optional Parameters

All raw methods accept:

| Parameter | Type | Default | Description |
|:---|:---|:---|:---|
| `url` | `str` | required | Full URL (not relative) |
| `headers` | `dict[str, str]` | `None` | Request headers |
| `body` | `bytes` | `None` | Request body (POST/PUT/PATCH only) |
| `timeout` | `float` | `None` | Timeout in seconds |

!!! warning "Trade-offs"
    The raw API sacrifices httpx compatibility (no `Response` object, cookies,
    auth, redirects, or event hooks) for ~2× lower per-request latency. Use it
    only when every microsecond counts.

---

## download() — Direct File Download { #download }

Download a URL directly to a file on disk in one line. Returns the `Response`
for status/header inspection.

```python
with httpxr.Client() as client:
    resp = client.download("https://example.com/data.csv", "/tmp/data.csv")
    print(f"{resp.status_code} — {resp.headers.get('content-type')}")
```

Compared to manual streaming:

```python
# download() — one line
client.download(url, "/tmp/file.bin")

# Equivalent manual streaming
with client.stream("GET", url) as response:
    with open("/tmp/file.bin", "wb") as f:
        for chunk in response.iter_bytes():
            f.write(chunk)
```

---

## response.json_bytes() — Raw JSON Bytes { #json-bytes }

Returns the response body as raw `bytes` without the UTF-8 decode step.
Feed directly into fast JSON parsers like [orjson](https://github.com/ijl/orjson)
or [msgspec](https://github.com/jcrist/msgspec).

```python
import orjson
import httpxr

with httpxr.Client() as client:
    response = client.get("https://api.example.com/data")

    # Standard: bytes → str → parse (two copies)
    data = response.json()

    # Fast path: bytes → parse directly (one copy)
    data = orjson.loads(response.json_bytes())
```

!!! tip "When to use"
    `json_bytes()` is most useful when combined with a bytes-native parser like
    `orjson` or `msgspec`. With the standard `json` module, the difference is
    minimal since `json.loads()` accepts both `str` and `bytes`.

---

## response.iter_json() — NDJSON & SSE Streaming { #iter-json }

Parse newline-delimited JSON (NDJSON) or Server-Sent Events (SSE) responses
as a stream of Python objects.

### NDJSON

```python
with client.stream("GET", "https://api.example.com/events") as response:
    for obj in response.iter_json():
        print(obj)  # each line parsed as a dict
```

### SSE / OpenAI-style streaming

`iter_json()` automatically strips `data:` prefixes and skips `[DONE]` sentinels:

```python
with client.stream("POST", "https://api.openai.com/v1/chat/completions",
                   json={"model": "gpt-4o", "stream": True, ...}) as response:
    for chunk in response.iter_json():
        delta = chunk["choices"][0]["delta"]
        print(delta.get("content", ""), end="", flush=True)
```

!!! note "Full SSE support"
    For complete SSE support including `event`, `id`, and `retry` fields,
    use [`httpxr.sse`](sse.md) instead.

---

## pool_status() — Connection Pool Introspection { #pool-status }

Inspect the current state of the connection pool:

```python
with httpxr.Client() as client:
    # Make some requests...
    client.get("https://api.example.com/a")
    client.get("https://api.example.com/b")

    status = client.pool_status()
    print(status)
    # {
    #   "idle_connections": 2,
    #   "active_connections": 0,
    #   "max_connections": 100,
    # }
```

Useful for debugging connection exhaustion or verifying pool configuration.

---

## RetryConfig & RateLimit { #resilience }

Built-in retry and rate-limiting without external dependencies.

```python
client = httpxr.Client(
    retry=httpxr.RetryConfig(max_retries=3, backoff_factor=0.5),
    rate_limit=httpxr.RateLimit(requests_per_second=10.0, burst=20),
)
```

See the full [Resilience guide](resilience.md) for all options and patterns.

---

## Server-Sent Events { #sse }

```python
from httpxr.sse import connect_sse

with httpxr.Client() as client:
    with connect_sse(client, "GET", "https://example.com/stream") as source:
        for event in source.iter_sse():
            print(event.event, event.data)
```

See the full [SSE guide](sse.md) for async support, OpenAI streaming, and more.

---

## httpxr.extensions — Big-Data Ingestion Helpers { #extensions-module }

The `httpxr.extensions` module bundles helpers designed for high-throughput,
low-memory data ingestion pipelines such as Databricks or PySpark. All helpers
are importable from the package root **or** from `httpxr.extensions`.

```python
from httpxr import paginate_to_records, iter_json_bytes, gather_raw_bytes, OAuth2Auth
# or:
import httpxr.extensions
```

### paginate_to_records() { #paginate-to-records }

A lazy iterator that unwraps the records array from every page, yielding
individual records rather than full `Response` objects.  O(1) memory.

```python
import httpxr
from httpxr import paginate_to_records, OAuth2Auth

auth = OAuth2Auth(
    token_url="https://login.microsoftonline.com/<tenant>/oauth2/v2.0/token",
    client_id="...", client_secret="...",
    scope="https://graph.microsoft.com/.default",
)

with httpxr.Client(auth=auth) as client:
    for user in paginate_to_records(
        client, "GET",
        "https://graph.microsoft.com/v1.0/users",
        records_key="value",
        next_url="@odata.nextLink",
    ):
        save(user)
```

| Parameter | Default | Description |
|:---|:---|:---|
| `records_key` | `"value"` | JSON key containing the records list. `None` = yield whole page |
| `next_url` | — | JSON field with next-page URL (OData style) |
| `next_header` | — | HTTP header carrying `rel="next"` link (GitHub style) |
| `next_func` | — | Custom `(Response) → str | None` |
| `max_pages` | `100` | Safety cap |

When no pagination strategy is given, a single request is made (useful for
paging-unaware endpoints).

Async variant: `apaginate_to_records(client: AsyncClient, …)`.

---

### iter_json_bytes() { #iter-json-bytes }

Stream an NDJSON or SSE response as **raw bytes** lines — no UTF-8 decode, no
intermediate Python strings. Feed directly into `orjson.loads` or
`spark.read.json()`.

```python
import orjson
with httpxr.Client() as client:
    with client.stream("GET", "https://api.example.com/events.ndjson") as r:
        batch = []
        for raw in iter_json_bytes(r):
            batch.append(orjson.loads(raw))
            if len(batch) >= 1000:
                spark.createDataFrame(batch).write.saveAsTable("events")
                batch = []
```

Handles: NDJSON, SSE `data:` prefix, `[DONE]` sentinel, `event:` / `id:` / `retry:` lines.

Async variant: `aiter_json_bytes(response)`.

---

### gather_raw_bytes() { #gather-raw-bytes }

Concurrent batch requests backed by Rust tokio, returning each response body
as `bytes` (or a parsed object when `parser` is given).

```python
import orjson
from httpxr import gather_raw_bytes

with httpxr.Client() as client:
    reqs = [
        client.build_request("GET", f"https://api.example.com/item/{i}")
        for i in range(500)
    ]
    records = gather_raw_bytes(client, reqs, parser=orjson.loads, max_concurrency=50)
    df = spark.createDataFrame(records)
```

| Parameter | Default | Description |
|:---|:---|:---|
| `max_concurrency` | `10` | Rust-level parallel request cap |
| `return_exceptions` | `False` | Include errors inline vs raising |
| `parser` | `None` | Callable `(bytes) → Any`; `None` returns raw bytes |

---

### OAuth2Auth { #oauth2-auth }

Client-credentials token with transparent refresh.  Thread-safe.

```python
from httpxr import OAuth2Auth
auth = OAuth2Auth(
    token_url="https://login.microsoftonline.com/<tenant>/oauth2/v2.0/token",
    client_id="CLIENT_ID",
    client_secret="SECRET",
    scope="https://graph.microsoft.com/.default",
    leeway_seconds=60,   # refresh this many seconds before actual expiry
)
with httpxr.Client(auth=auth) as client:
    data = client.get("https://graph.microsoft.com/v1.0/me").json()
```

Works with `AsyncClient` too — `async_auth_flow` is implemented.
Triggers a one-time re-fetch on a `401` response.

!!! tip "Databricks examples"
    See [`examples/databricks/`](https://github.com/bmsuisse/httpxr/tree/main/examples/databricks)
    for full notebook-ready examples using `OAuth2Auth` + `paginate_to_records`
    with Microsoft Graph and Salesforce APIs.
