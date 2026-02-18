# Cookbook

Real-world patterns and recipes for httpxr.

---

## OAuth2 Token Refresh

Use event hooks to automatically refresh expired OAuth2 tokens:

```python
import httpxr

TOKEN = {"access_token": "initial_token", "expires_at": 0}


def refresh_token() -> str:
    """Call your OAuth2 token endpoint."""
    with httpxr.Client() as token_client:
        r = token_client.post(
            "https://auth.example.com/oauth/token",
            data={
                "grant_type": "client_credentials",
                "client_id": "my-client-id",
                "client_secret": "my-secret",
            },
        )
        r.raise_for_status()
        data = r.json()
        TOKEN["access_token"] = data["access_token"]
        return data["access_token"]


def add_auth(request: httpxr.Request) -> None:
    request.headers["authorization"] = (
        f"Bearer {TOKEN['access_token']}"
    )


def handle_401(response: httpxr.Response) -> None:
    if response.status_code == 401:
        # Refresh and retry
        token = refresh_token()
        request = response.request
        request.headers["authorization"] = f"Bearer {token}"
        # Note: automatic retry requires custom logic
        # This hook updates the token for the NEXT request


with httpxr.Client(
    event_hooks={
        "request": [add_auth],
        "response": [handle_401],
    }
) as client:
    r = client.get("https://api.example.com/data")
    print(r.json())
```

---

## Microsoft Graph Pagination

Use `paginate()` with `@odata.nextLink` to fetch all pages from Microsoft Graph:

```python
import httpxr

with httpxr.Client() as client:
    all_users = []

    for page in client.paginate(
        "GET",
        "https://graph.microsoft.com/v1.0/users",
        next_url="@odata.nextLink",
        max_pages=50,
        headers={"Authorization": "Bearer YOUR_TOKEN"},
    ):
        page.raise_for_status()
        users = page.json().get("value", [])
        all_users.extend(users)
        print(f"Fetched {len(users)} users (total: {len(all_users)})")

    print(f"Total users: {len(all_users)}")
```

---

## GitHub Issues with Link Header Pagination

```python
import httpxr

with httpxr.Client(
    headers={"Accept": "application/vnd.github.v3+json"}
) as client:
    all_issues = []

    for page in client.paginate(
        "GET",
        "https://api.github.com/repos/python/cpython/issues",
        next_header="link",
        max_pages=5,
    ):
        issues = page.json()
        all_issues.extend(issues)
        print(f"Page: {len(issues)} issues")

    print(f"Total: {len(all_issues)} issues")
```

---

## Concurrent API Calls with Error Handling

Use `gather()` to fetch many resources in parallel with graceful error handling:

```python
import httpxr

with httpxr.Client() as client:
    # Build requests
    requests = [
        client.build_request(
            "GET", f"https://api.example.com/items/{i}"
        )
        for i in range(100)
    ]

    # Send all at once, max 20 in-flight
    responses = client.gather(
        requests,
        max_concurrency=20,
        return_exceptions=True,
    )

    # Process results
    succeeded = 0
    failed = 0
    for i, resp in enumerate(responses):
        if isinstance(resp, Exception):
            print(f"Item {i}: FAILED — {resp}")
            failed += 1
        else:
            print(f"Item {i}: {resp.status_code}")
            succeeded += 1

    print(f"\n✓ {succeeded} succeeded, ✗ {failed} failed")
```

---

## Async Concurrent Requests

```python
import asyncio
import httpxr


async def main() -> None:
    async with httpxr.AsyncClient() as client:
        requests = [
            client.build_request(
                "GET", f"https://httpbin.org/delay/{i % 3}"
            )
            for i in range(10)
        ]
        responses = await client.gather(
            requests, max_concurrency=5
        )

        for r in responses:
            print(f"{r.url} → {r.status_code}")


asyncio.run(main())
```

---

## Download Large Files with Progress

Stream large files to disk without loading everything into memory:

```python
import httpxr

url = "https://example.com/large-dataset.csv"

with httpxr.Client() as client:
    with client.stream("GET", url) as response:
        response.raise_for_status()
        total = int(response.headers.get("content-length", 0))
        downloaded = 0

        with open("dataset.csv", "wb") as f:
            for chunk in response.iter_bytes(chunk_size=8192):
                f.write(chunk)
                downloaded += len(chunk)
                if total:
                    pct = downloaded / total * 100
                    print(f"\r{pct:.1f}%", end="", flush=True)

        print(f"\n✓ Downloaded {downloaded:,} bytes")
```

---

## Retry with Backoff

Use `RetryConfig` for automatic retries with exponential backoff:

```python
import httpxr

retry = httpxr.RetryConfig(
    max_retries=3,
    backoff_factor=0.5,       # 0.5s, 1.0s, 2.0s
    retry_on_status=[429, 500, 502, 503, 504],
    jitter=True,              # add randomness to prevent thundering herd
)

with httpxr.Client() as client:
    response = client.get(
        "https://api.example.com/data",
        extensions={"retry": retry},
    )
    print(response.json())
```

---

## Custom SSL Certificates

Use custom CA certificates or client certificates:

```python
import httpxr

# Custom CA bundle
with httpxr.Client(verify="/path/to/ca-bundle.crt") as client:
    r = client.get("https://internal-api.corp.example.com/data")

# Disable SSL verification (development only!)
with httpxr.Client(verify=False) as client:
    r = client.get("https://localhost:8443/api")
```

---

## Session Cookies

Cookies persist automatically across requests within a client session:

```python
import httpxr

with httpxr.Client() as client:
    # Login — cookies are stored automatically
    client.post(
        "https://example.com/login",
        data={"username": "alice", "password": "secret"},
    )

    # Subsequent requests include the session cookie
    profile = client.get("https://example.com/profile")
    print(profile.json())

    # Inspect cookies
    for name in client.cookies:
        print(f"  {name} = {client.cookies[name]}")
```

---

## Logging All Requests

Use event hooks to log every request and response:

```python
import logging
import httpxr

logging.basicConfig(level=logging.INFO)
log = logging.getLogger("http")


def log_request(request: httpxr.Request) -> None:
    log.info(f"→ {request.method} {request.url}")


def log_response(response: httpxr.Response) -> None:
    elapsed = response.elapsed.total_seconds() * 1000
    log.info(
        f"← {response.status_code} "
        f"({elapsed:.0f}ms) {response.url}"
    )


with httpxr.Client(
    event_hooks={
        "request": [log_request],
        "response": [log_response],
    }
) as client:
    client.get("https://httpbin.org/get")
    client.get("https://httpbin.org/status/404")
```

Output:

```
INFO:http:→ GET https://httpbin.org/get
INFO:http:← 200 (142ms) https://httpbin.org/get
INFO:http:→ GET https://httpbin.org/status/404
INFO:http:← 404 (98ms) https://httpbin.org/status/404
```

---

## Raw API for Maximum Speed

When you need the absolute lowest latency and don't need the full `Response`
object:

```python
import httpxr
import json

with httpxr.Client() as client:
    # Returns (status, headers_dict, body_bytes)
    status, headers, body = client.get_raw(
        "https://api.example.com/data"
    )

    if status == 200:
        data = json.loads(body)
        print(data)
    else:
        print(f"Error: {status}")
```

---

## Base URL for API Clients

```python
import httpxr

with httpxr.Client(
    base_url="https://api.example.com/v2",
    headers={"Authorization": "Bearer token123"},
) as api:
    users = api.get("/users").json()
    posts = api.get("/posts").json()
    api.post("/users", json={"name": "Alice"})
```

---

## Testing with MockTransport

```python
import httpxr


def mock_handler(request: httpxr.Request) -> httpxr.Response:
    if request.url.path == "/users":
        return httpxr.Response(
            200,
            json=[{"id": 1, "name": "Alice"}],
        )
    return httpxr.Response(404)


transport = httpxr.MockTransport(mock_handler)

with httpxr.Client(transport=transport) as client:
    r = client.get("https://api.example.com/users")
    assert r.status_code == 200
    assert r.json()[0]["name"] == "Alice"

    r = client.get("https://api.example.com/nope")
    assert r.status_code == 404
```
