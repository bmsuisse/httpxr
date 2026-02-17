# Requests & Responses

## The Response Object

Every HTTP call returns a `Response` object with a rich set of properties:

```python
import httpxr

response = httpxr.get("https://httpbin.org/get")

# ── Status ─────────────────────────────────────────────
response.status_code       # 200
response.reason_phrase     # "OK"
response.http_version      # "HTTP/1.1"

# ── Content ────────────────────────────────────────────
response.content           # b'...' (raw bytes)
response.text              # '...' (decoded string)
response.json()            # {...} (parsed JSON)
response.encoding          # "utf-8"

# ── Metadata ───────────────────────────────────────────
response.url               # URL("https://httpbin.org/get")
response.headers           # Headers({...})
response.cookies           # Cookies({...})
response.elapsed           # 0.123 (seconds)
response.request           # The original Request object
response.history           # List of redirect responses

# ── Status checks ──────────────────────────────────────
response.is_success        # True for 2xx
response.is_redirect       # True for 3xx
response.is_client_error   # True for 4xx
response.is_server_error   # True for 5xx
response.is_error          # True for 4xx or 5xx
```

## raise_for_status()

Raise an `HTTPStatusError` for 4xx/5xx responses:

```python
response = client.get("https://httpbin.org/status/404")

try:
    response.raise_for_status()
except httpxr.HTTPStatusError as exc:
    print(exc)                     # "Client error '404 Not Found' for url ..."
    print(exc.request.url)         # URL("https://httpbin.org/status/404")
    print(exc.response.status_code) # 404
```

For 2xx responses, `raise_for_status()` returns the response itself, enabling chaining:

```python
data = client.get("https://httpbin.org/get").raise_for_status().json()
```

## Constructing Responses

You can create `Response` objects directly — useful for testing and mocking:

```python
# From text
r = httpxr.Response(200, text="Hello, world!")

# From bytes
r = httpxr.Response(200, content=b'{"key": "value"}')

# From JSON (auto-serialized)
r = httpxr.Response(200, json={"users": [{"id": 1, "name": "Alice"}]})

# With custom headers
r = httpxr.Response(
    201,
    content=b"Created",
    headers={"X-Request-Id": "abc-123", "Content-Type": "text/plain"},
)
```

## The Request Object

Every `Response` has a `.request` back-reference. You can also construct requests manually:

```python
request = httpxr.Request(
    "POST",
    "https://api.example.com/data",
    content=b'{"hello": "world"}',
    headers={"Content-Type": "application/json"},
)

print(request.method)              # "POST"
print(request.url)                 # URL("https://api.example.com/data")
print(request.headers)             # Headers({...})
print(request.content)             # b'{"hello": "world"}'
```

## Status Code Constants

httpxr provides the `codes` object for readable status code checks:

```python
response = client.get("https://httpbin.org/get")

if response.status_code == httpxr.codes.OK:
    print("Success!")
```
