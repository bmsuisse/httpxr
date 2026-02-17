# Timeouts & Limits

## Timeout

Control how long httpxr waits for various phases of the request:

```python
import httpxr

# Simple: single value for all phases
with httpxr.Client(timeout=10.0) as client:
    response = client.get("https://httpbin.org/get")
```

### Granular Timeouts

```python
timeout = httpxr.Timeout(
    connect=5.0,   # Time to establish a TCP connection
    read=10.0,     # Time to receive response data
    write=5.0,     # Time to send request data
    pool=5.0,      # Time waiting for a connection from the pool
)

with httpxr.Client(timeout=timeout) as client:
    response = client.get("https://httpbin.org/get")
```

### Disable Timeouts

```python
# No timeout at all (use with caution!)
with httpxr.Client(timeout=httpxr.Timeout(None)) as client:
    response = client.get("https://httpbin.org/delay/30")
```

### Per-Request Override

```python
with httpxr.Client(timeout=2.0) as client:
    # This one slow endpoint gets extra time
    response = client.get(
        "https://httpbin.org/delay/5",
        timeout=httpxr.Timeout(10.0),
    )
```

### Default Timeout

The default timeout is **5 seconds** for all phases:

```python
httpxr.DEFAULT_TIMEOUT_CONFIG  # Timeout(connect=5.0, read=5.0, write=5.0, pool=5.0)
```

---

## Limits

Control the connection pool:

```python
limits = httpxr.Limits(
    max_connections=100,           # Max total connections
    max_keepalive_connections=20,  # Max idle keepalive connections
)

with httpxr.Client(limits=limits) as client:
    response = client.get("https://httpbin.org/get")
```

### Default Limits

```python
httpxr.DEFAULT_LIMITS  # Limits(max_connections=100, max_keepalive_connections=20)
```

---

## Proxy

Route traffic through an HTTP or SOCKS proxy:

```python
# HTTP proxy
with httpxr.Client(proxy="http://proxy.example.com:8080") as client:
    response = client.get("https://httpbin.org/get")

# SOCKS5 proxy
with httpxr.Client(proxy="socks5://proxy.example.com:1080") as client:
    response = client.get("https://httpbin.org/get")
```

!!! tip "Environment Proxies"
    By default (`trust_env=True`), httpxr reads `HTTP_PROXY`, `HTTPS_PROXY`,
    and `NO_PROXY` environment variables.

---

## Handling Timeouts

```python
try:
    httpxr.get("https://httpbin.org/delay/10", timeout=httpxr.Timeout(1.0))
except httpxr.ConnectTimeout:
    print("Connection timed out")
except httpxr.ReadTimeout:
    print("Read timed out")
except httpxr.TimeoutException:
    print("Some timeout occurred")
```
