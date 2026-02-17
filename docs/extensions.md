# Extensions

These features are **exclusive to httpxr** — not available in httpx.
They leverage the Rust runtime for performance that's impossible in pure Python.

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
