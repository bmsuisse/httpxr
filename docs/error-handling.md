# Error Handling

## Exception Hierarchy

httpxr uses the same exception hierarchy as httpx:

```
HTTPError
├── HTTPStatusError          ← 4xx / 5xx responses
├── RequestError
│   ├── TransportError
│   │   ├── TimeoutException
│   │   │   ├── ConnectTimeout
│   │   │   ├── ReadTimeout
│   │   │   ├── WriteTimeout
│   │   │   └── PoolTimeout
│   │   ├── NetworkError
│   │   │   ├── ConnectError
│   │   │   ├── ReadError
│   │   │   ├── WriteError
│   │   │   └── CloseError
│   │   ├── ProtocolError
│   │   │   ├── LocalProtocolError
│   │   │   └── RemoteProtocolError
│   │   ├── ProxyError
│   │   ├── UnsupportedProtocol
│   │   └── DecodingError
│   └── TooManyRedirects
└── StreamError
    ├── StreamConsumed
    └── StreamClosed
```

## Handling HTTP Errors

### raise_for_status()

```python
import httpxr

with httpxr.Client() as client:
    response = client.get("https://httpbin.org/status/404")

    try:
        response.raise_for_status()
    except httpxr.HTTPStatusError as exc:
        print(f"Error: {exc}")
        print(f"  Request URL:     {exc.request.url}")
        print(f"  Response status: {exc.response.status_code}")
```

### Status Code Checks (no exceptions)

```python
response = client.get("https://httpbin.org/status/201")

if response.is_success:        # 2xx
    print("OK!")
elif response.is_redirect:     # 3xx
    print("Redirect")
elif response.is_client_error: # 4xx
    print("Client error")
elif response.is_server_error: # 5xx
    print("Server error")
```

## Handling Timeouts

```python
try:
    httpxr.get("https://httpbin.org/delay/10", timeout=httpxr.Timeout(1.0))
except httpxr.ConnectTimeout:
    print("Failed to connect in time")
except httpxr.ReadTimeout:
    print("Server took too long to respond")
except httpxr.TimeoutException:
    print("Some timeout occurred")
```

## Handling Network Errors

```python
try:
    httpxr.get("https://doesnotexist.example.com")
except httpxr.ConnectError:
    print("Could not connect to host")
except httpxr.NetworkError:
    print("Network-level failure")
```

## Catch-All Pattern

```python
try:
    response = httpxr.get("https://httpbin.org/status/503")
    response.raise_for_status()
except httpxr.HTTPError as exc:
    # Catches everything: status errors, timeouts, network errors, etc.
    print(f"{type(exc).__name__}: {exc}")
```

## Unsupported Protocols

```python
try:
    httpxr.get("ftp://example.org")
except httpxr.UnsupportedProtocol:
    print("Only HTTP and HTTPS are supported")
```
