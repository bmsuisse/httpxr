# Clients

httpxr provides `Client` (sync) and `AsyncClient` (async) with identical APIs.
Always use them as **context managers** for proper connection lifecycle management.

## Basic Usage

=== "Sync"

    ```python
    import httpxr

    with httpxr.Client() as client:
        response = client.get("https://httpbin.org/get")
        print(response.status_code)
    ```

=== "Async"

    ```python
    import httpxr
    import asyncio

    async def main():
        async with httpxr.AsyncClient() as client:
            response = await client.get("https://httpbin.org/get")
            print(response.status_code)

    asyncio.run(main())
    ```

## HTTP Methods

Both clients support all standard HTTP methods:

```python
response = client.get(url)
response = client.post(url, json={"key": "value"})
response = client.put(url, content=b"data")
response = client.patch(url, content=b"partial")
response = client.delete(url)
response = client.head(url)
response = client.options(url)
response = client.request("CUSTOM", url)
```

## Base URL

Set a `base_url` to avoid repeating the domain:

```python
with httpxr.Client(base_url="https://api.example.com/v2") as client:
    # Sends GET https://api.example.com/v2/users
    r = client.get("/users")

    # Absolute URLs are sent as-is
    r = client.get("https://other.example.com/health")
```

## HTTP/2

Enable HTTP/2 with a single flag:

```python
with httpxr.Client(http2=True) as client:
    response = client.get("https://httpbin.org/get")
    print(response.http_version)  # "HTTP/2"
```

## Redirects

By default, redirects are **not** followed. Enable with `follow_redirects`:

```python
# Client-level
with httpxr.Client(follow_redirects=True) as client:
    r = client.get("https://httpbin.org/redirect/3")
    print(len(r.history))  # 3 redirect hops

# Per-request override
with httpxr.Client() as client:
    r = client.get("https://httpbin.org/redirect/1", follow_redirects=True)
```

## Sending Data

### JSON

```python
response = client.post(
    "https://httpbin.org/post",
    json={"name": "Alice", "age": 30},
)
```

### Form Data

```python
response = client.post(
    "https://httpbin.org/post",
    data={"username": "alice", "password": "secret"},
)
```

### Raw Bytes

```python
response = client.post(
    "https://httpbin.org/post",
    content=b"raw binary payload",
)
```

### File Upload

```python
response = client.post(
    "https://httpbin.org/post",
    files={"upload": ("report.csv", open("report.csv", "rb"), "text/csv")},
)
```

## Building Requests Manually

Use `build_request()` to create a request without sending it — useful for
inspection, logging, or batch operations with [`gather()`](extensions.md#gather):

```python
request = client.build_request("GET", "https://httpbin.org/get")
print(request.method)   # "GET"
print(request.url)      # URL("https://httpbin.org/get")

response = client.send(request)
```

## Event Hooks

Register callbacks to run on every request or response:

```python
def log_request(request):
    print(f"→ {request.method} {request.url}")

def log_response(response):
    print(f"← {response.status_code}")

with httpxr.Client(
    event_hooks={"request": [log_request], "response": [log_response]}
) as client:
    client.get("https://httpbin.org/get")
    # → GET https://httpbin.org/get
    # ← 200
```

## Client Configuration

| Parameter | Type | Default | Description |
|:---|:---|:---|:---|
| `base_url` | `str \| URL` | `None` | Prepended to relative URLs |
| `auth` | `Auth` | `None` | Default auth for all requests |
| `headers` | `dict` | `None` | Default headers merged into every request |
| `cookies` | `dict` | `None` | Default cookies |
| `params` | `dict` | `None` | Default query parameters |
| `timeout` | `float \| Timeout` | `5.0` | Default timeout |
| `follow_redirects` | `bool` | `False` | Follow HTTP redirects |
| `max_redirects` | `int` | `20` | Maximum redirect hops |
| `http2` | `bool` | `False` | Enable HTTP/2 |
| `verify` | `bool \| str` | `True` | SSL verification |
| `limits` | `Limits` | default | Connection pool limits |
| `transport` | `BaseTransport` | `None` | Custom transport |
| `trust_env` | `bool` | `True` | Read proxy from env vars |
| `default_encoding` | `str` | `"utf-8"` | Fallback text encoding |
| `event_hooks` | `dict` | `None` | Request/response callbacks |
