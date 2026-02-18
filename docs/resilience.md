# Resilience — Retry & Rate Limiting

`httpxr` ships with built-in retry and rate-limiting primitives that require
zero external dependencies. Both can be set at the **client level** (applied to
every request) or **per-request** via the `extensions` dict.

---

## RetryConfig — Automatic Retries

Automatically retry failed requests with exponential backoff and optional jitter.

### Basic Usage

```python
import httpxr

retry = httpxr.RetryConfig(
    max_retries=3,
    backoff_factor=0.5,       # delays: 0.5s, 1.0s, 2.0s
    retry_on_status=[429, 500, 502, 503, 504],
    jitter=True,              # randomise delays to avoid thundering herd
)

# Client-level: all requests retry automatically
with httpxr.Client(retry=retry) as client:
    r = client.get("https://api.example.com/data")
    print(r.json())
```

### Per-Request Override

```python
with httpxr.Client() as client:
    # Only this request gets retry logic
    r = client.get(
        "https://api.example.com/flaky",
        extensions={"retry": httpxr.RetryConfig(max_retries=5)},
    )
```

### Parameters

| Parameter | Type | Default | Description |
|:---|:---|:---|:---|
| `max_retries` | `int` | `3` | Maximum number of retry attempts |
| `backoff_factor` | `float` | `0.5` | Base delay multiplier (seconds) |
| `retry_on_status` | `list[int]` | `[429, 500, 502, 503, 504]` | Status codes that trigger a retry |
| `jitter` | `bool` | `True` | Add random noise to prevent thundering herd |

### Backoff Formula

```
delay = backoff_factor * (2 ** attempt)
```

With `backoff_factor=0.5`: 0.5s → 1.0s → 2.0s → 4.0s …

With `jitter=True`, a random fraction is added to each delay.

### Utility Methods

```python
retry = httpxr.RetryConfig(max_retries=3, backoff_factor=0.5)

# Check if a status code triggers a retry
retry.should_retry(429)  # True
retry.should_retry(200)  # False

# Calculate delay for a given attempt (0-indexed)
retry.delay_for_attempt(0)  # ~0.5s
retry.delay_for_attempt(1)  # ~1.0s
retry.delay_for_attempt(2)  # ~2.0s
```

---

## RateLimit — Request Throttling

A token-bucket rate limiter that automatically paces requests to stay within
a configured rate.

### Basic Usage

```python
import httpxr

rate_limit = httpxr.RateLimit(
    requests_per_second=10.0,  # max 10 req/s sustained
    burst=20,                  # allow bursts up to 20 req
)

with httpxr.Client(rate_limit=rate_limit) as client:
    for i in range(100):
        r = client.get(f"https://api.example.com/items/{i}")
        print(r.status_code)
    # Automatically throttled to ≤10 req/s
```

### Parameters

| Parameter | Type | Default | Description |
|:---|:---|:---|:---|
| `requests_per_second` | `float` | required | Sustained request rate |
| `burst` | `int` | `1` | Maximum burst size (token bucket capacity) |

### Utility Methods

```python
rl = httpxr.RateLimit(requests_per_second=5.0, burst=10)

# How long to wait before the next request is allowed
wait = rl.wait_time()  # seconds (0.0 if tokens available)
```

---

## Combined: Resilient Client

For production API clients, combine both:

```python
import httpxr

client = httpxr.Client(
    base_url="https://api.example.com",
    retry=httpxr.RetryConfig(
        max_retries=3,
        backoff_factor=0.5,
        retry_on_status=[429, 500, 502, 503, 504],
        jitter=True,
    ),
    rate_limit=httpxr.RateLimit(
        requests_per_second=10.0,
        burst=20,
    ),
    timeout=30.0,
)

with client:
    # Rate-limited, auto-retrying requests
    for item_id in range(1000):
        r = client.get(f"/items/{item_id}")
        r.raise_for_status()
        print(r.json())
```

---

## Dynamic Configuration

Both `retry` and `rate_limit` can be set or cleared after client creation:

```python
with httpxr.Client() as client:
    # Start without retry
    assert client.retry is None

    # Add retry dynamically
    client.retry = httpxr.RetryConfig(max_retries=3)

    # Add rate limit dynamically
    client.rate_limit = httpxr.RateLimit(requests_per_second=50.0)

    # Remove them
    client.retry = None
    client.rate_limit = None
```

---

## AsyncClient Support

Both work identically with `AsyncClient`:

```python
import asyncio
import httpxr


async def main() -> None:
    async with httpxr.AsyncClient(
        retry=httpxr.RetryConfig(max_retries=3),
        rate_limit=httpxr.RateLimit(requests_per_second=20.0),
    ) as client:
        r = await client.get("https://api.example.com/data")
        print(r.json())


asyncio.run(main())
```
