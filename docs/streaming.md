# Streaming

Stream large responses efficiently instead of loading everything into memory.

## Sync Streaming

```python
import httpxr

with httpxr.Client() as client:
    with client.stream("GET", "https://example.com/large-file.zip") as response:
        print(response.status_code)

        # Iterate over chunks
        total = 0
        for chunk in response.iter_bytes():
            total += len(chunk)
        print(f"Downloaded {total} bytes")
```

### Streaming Methods

| Method | Description |
|:---|:---|
| `iter_bytes()` | Yields decoded `bytes` chunks |
| `iter_raw()` | Yields raw `bytes` (no content decoding) |
| `iter_text()` | Yields decoded `str` chunks |
| `iter_lines()` | Yields text line by line |
| `read()` | Read the entire body at once |

### Download to File

```python
with httpxr.Client() as client:
    with client.stream("GET", "https://example.com/file.zip") as response:
        with open("file.zip", "wb") as f:
            for chunk in response.iter_bytes():
                f.write(chunk)
```

---

## Async Streaming

```python
import httpxr
import asyncio

async def download():
    async with httpxr.AsyncClient() as client:
        async with client.stream("GET", "https://example.com/file.zip") as response:
            # Read all at once
            body = await response.aread()
            print(f"Downloaded {len(body)} bytes")

            # Or iterate
            async for chunk in response.aiter_bytes():
                process(chunk)

asyncio.run(download())
```

### Async Streaming Methods

| Method | Description |
|:---|:---|
| `aiter_bytes()` | Async yields decoded `bytes` |
| `aiter_raw()` | Async yields raw `bytes` |
| `aiter_text()` | Async yields `str` chunks |
| `aiter_lines()` | Async yields text line by line |
| `aread()` | Async read entire body |

---

## Top-Level stream()

A convenience function that creates a one-off client:

```python
with httpxr.stream("GET", "https://httpbin.org/get") as response:
    response.read()
    print(response.status_code)
```

!!! tip "Performance"
    For multiple streamed requests, use a `Client` context manager instead
    of the top-level function to benefit from connection pooling.
