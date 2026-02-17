# Quick Start

## Installation

Install httpxr from PyPI:

=== "pip"

    ```bash
    pip install httpxr
    ```

=== "uv"

    ```bash
    uv add httpxr
    ```

=== "poetry"

    ```bash
    poetry add httpxr
    ```

=== "conda"

    ```bash
    pip install httpxr  # httpxr is not on conda-forge yet
    ```

To also install the **optional CLI**:

```bash
pip install "httpxr[cli]"
```

!!! tip "Coming from httpx?"
    httpxr is a **drop-in replacement**. In most cases you just need to change your import:
    ```python
    # Before
    import httpx
    # After
    import httpxr as httpx  # or just import httpxr
    ```
    See the full [Migration Guide](migration.md) for details.

## Your First Request

### Sync

The simplest way to make an HTTP request:

```python
import httpxr

response = httpxr.get("https://httpbin.org/get")
print(response.status_code)    # 200
print(response.json())         # {...}
```

### Async

For async code, use `AsyncClient`:

```python
import httpxr
import asyncio

async def main():
    async with httpxr.AsyncClient() as client:
        response = await client.get("https://httpbin.org/get")
        print(response.json())

asyncio.run(main())
```

## Using a Client

For production code, always use a `Client` (or `AsyncClient`) as a context manager.
This enables **connection pooling** and **keepalive** for much better performance:

=== "Sync"

    ```python
    import httpxr

    with httpxr.Client() as client:
        r = client.get("https://httpbin.org/get")
        print(r.status_code)
    ```

=== "Async"

    ```python
    import httpxr
    import asyncio

    async def main():
        async with httpxr.AsyncClient() as client:
            r = await client.get("https://httpbin.org/get")
            print(r.status_code)

    asyncio.run(main())
    ```

## Convenience Functions

httpxr provides top-level convenience functions that create a one-off client per call.
Great for scripts and quick tasks:

```python
import httpxr

# GET
r = httpxr.get("https://httpbin.org/get")

# POST with JSON
r = httpxr.post("https://httpbin.org/post", json={"key": "value"})

# POST with raw bytes
r = httpxr.post("https://httpbin.org/post", content=b"Hello!")

# PUT
r = httpxr.put("https://httpbin.org/put", content=b"updated")

# PATCH
r = httpxr.patch("https://httpbin.org/patch", content=b"partial")

# DELETE
r = httpxr.delete("https://httpbin.org/delete")

# HEAD
r = httpxr.head("https://httpbin.org/get")

# OPTIONS
r = httpxr.options("https://httpbin.org/get")
```

## Working with Responses

```python
response = httpxr.get("https://httpbin.org/get")

# Status
response.status_code       # 200
response.reason_phrase      # "OK"
response.is_success         # True

# Content
response.text               # Response body as string
response.content            # Response body as bytes
response.json()             # Parse JSON body as dict

# Metadata
response.url                # URL object
response.headers            # Headers object
response.encoding           # "utf-8"
response.elapsed            # Request duration in seconds
response.http_version       # "HTTP/1.1"
```

## What's Next?

- [Clients](clients.md) — Client lifecycle, base URLs, HTTP/2
- [Authentication](authentication.md) — BasicAuth, DigestAuth, custom auth
- [Extensions](extensions.md) — `gather()`, `paginate()`, raw API
- [Error Handling](error-handling.md) — Exception hierarchy
