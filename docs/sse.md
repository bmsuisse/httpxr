# Server-Sent Events (SSE)

`httpxr` ships with a built-in SSE module — a port of
[httpx-sse](https://github.com/florimondmanca/httpx-sse) — that works
seamlessly with the Rust-backed streaming transport.

!!! tip "Installation"
    SSE support is included in the base install. No extra dependencies needed:
    ```bash
    pip install httpxr
    ```

---

## Quick Start

=== "Sync"

    ```python
    import httpxr
    from httpxr.sse import connect_sse

    with httpxr.Client() as client:
        with connect_sse(client, "GET", "https://example.com/events") as source:
            for event in source.iter_sse():
                print(event.event, event.data)
    ```

=== "Async"

    ```python
    import asyncio
    import httpxr
    from httpxr.sse import aconnect_sse

    async def main() -> None:
        async with httpxr.AsyncClient() as client:
            async with aconnect_sse(client, "GET", "https://example.com/events") as source:
                async for event in source.aiter_sse():
                    print(event.event, event.data)

    asyncio.run(main())
    ```

---

## API Reference

### `connect_sse(client, method, url, **kwargs)`

A context manager that opens a streaming connection and yields an
`EventSource`. Sets the required `Accept: text/event-stream` and
`Cache-Control: no-store` headers automatically.

```python
from httpxr.sse import connect_sse

with connect_sse(client, "GET", url, headers={"Authorization": "Bearer ..."}) as source:
    for event in source.iter_sse():
        ...
```

### `aconnect_sse(client, method, url, **kwargs)`

Async version of `connect_sse`. Use with `AsyncClient`.

### `EventSource`

Wraps the streaming `Response` and exposes SSE iterators.

| Method | Returns | Description |
|:---|:---|:---|
| `iter_sse()` | `Iterator[ServerSentEvent]` | Sync SSE iterator |
| `aiter_sse()` | `AsyncIterator[ServerSentEvent]` | Async SSE iterator |
| `response` | `Response` | The underlying streaming response |

### `ServerSentEvent`

Each yielded event has these fields:

| Field | Type | Description |
|:---|:---|:---|
| `event` | `str` | Event type (default: `"message"`) |
| `data` | `str` | Event payload |
| `id` | `str \| None` | Event ID for reconnection |
| `retry` | `int \| None` | Reconnection delay in ms |

```python
for event in source.iter_sse():
    print(f"type={event.event!r}")
    print(f"data={event.data!r}")
    print(f"id={event.id!r}")
```

### `SSEError`

Raised when the server returns a non-`text/event-stream` content type.

```python
from httpxr.sse import SSEError

try:
    with connect_sse(client, "GET", url) as source:
        for event in source.iter_sse():
            ...
except SSEError as e:
    print(f"SSE error: {e}")
```

---

## Real-World Examples

### OpenAI Chat Streaming

```python
import httpxr
from httpxr.sse import connect_sse

with httpxr.Client(timeout=60.0) as client:
    with connect_sse(
        client,
        "POST",
        "https://api.openai.com/v1/chat/completions",
        headers={"Authorization": "Bearer YOUR_KEY"},
        json={
            "model": "gpt-4o",
            "stream": True,
            "messages": [{"role": "user", "content": "Hello!"}],
        },
    ) as source:
        for event in source.iter_sse():
            if event.data == "[DONE]":
                break
            import json
            chunk = json.loads(event.data)
            delta = chunk["choices"][0]["delta"]
            print(delta.get("content", ""), end="", flush=True)
    print()
```

### Async OpenAI Streaming

```python
import asyncio
import json
import httpxr
from httpxr.sse import aconnect_sse


async def stream_chat(prompt: str) -> str:
    result = []
    async with httpxr.AsyncClient(timeout=60.0) as client:
        async with aconnect_sse(
            client,
            "POST",
            "https://api.openai.com/v1/chat/completions",
            headers={"Authorization": "Bearer YOUR_KEY"},
            json={
                "model": "gpt-4o",
                "stream": True,
                "messages": [{"role": "user", "content": prompt}],
            },
        ) as source:
            async for event in source.aiter_sse():
                if event.data == "[DONE]":
                    break
                chunk = json.loads(event.data)
                delta = chunk["choices"][0]["delta"]
                if content := delta.get("content"):
                    result.append(content)
                    print(content, end="", flush=True)
    print()
    return "".join(result)


asyncio.run(stream_chat("Tell me a joke"))
```

### Generic Event Stream

```python
import httpxr
from httpxr.sse import connect_sse

with httpxr.Client() as client:
    with connect_sse(client, "GET", "https://example.com/stream") as source:
        for event in source.iter_sse():
            match event.event:
                case "ping":
                    pass  # heartbeat, ignore
                case "update":
                    print(f"Update: {event.data}")
                case "error":
                    print(f"Server error: {event.data}")
                    break
```

---

## Alternative: `response.iter_json()`

For simpler cases where you just want to extract JSON from SSE or NDJSON
responses without the full `EventSource` API, use [`response.iter_json()`](extensions.md#iter-json):

```python
with client.stream("GET", url) as response:
    for obj in response.iter_json():
        print(obj)  # each parsed JSON object
```

`iter_json()` handles `data:` prefixes and `[DONE]` sentinels automatically.
