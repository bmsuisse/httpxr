# Authentication

httpxr supports the same authentication classes as httpx.

## Basic Auth

```python
import httpxr

# Using BasicAuth class
response = httpxr.get(
    "https://httpbin.org/basic-auth/user/pass",
    auth=httpxr.BasicAuth("user", "pass"),
)

# Tuple shorthand
response = httpxr.get(
    "https://httpbin.org/basic-auth/user/pass",
    auth=("user", "pass"),
)
```

## Client-Level Auth

Apply auth to every request from a client:

```python
with httpxr.Client(auth=httpxr.BasicAuth("user", "pass")) as client:
    # Auth applied automatically
    r = client.get("https://httpbin.org/basic-auth/user/pass")
    print(r.status_code)  # 200

    # Disable for a specific request
    r = client.get("https://httpbin.org/get", auth=None)
```

## Digest Auth

```python
response = httpxr.get(
    "https://httpbin.org/digest-auth/auth/user/pass",
    auth=httpxr.DigestAuth("user", "pass"),
)
```

## Custom Auth with FunctionAuth

Inject headers, tokens, or any modification into every request:

```python
def add_api_key(request: httpxr.Request) -> httpxr.Request:
    """Add an API key header to every request."""
    request.headers["X-API-Key"] = "my-secret-key"
    return request

auth = httpxr.FunctionAuth(add_api_key)

with httpxr.Client(auth=auth) as client:
    r = client.get("https://httpbin.org/headers")
    print(r.json()["headers"]["X-Api-Key"])  # "my-secret-key"
```

### Bearer Token Example

```python
def bearer_auth(request: httpxr.Request) -> httpxr.Request:
    request.headers["Authorization"] = f"Bearer {get_token()}"
    return request

with httpxr.Client(auth=httpxr.FunctionAuth(bearer_auth)) as client:
    r = client.get("https://api.example.com/protected")
```

## NetRC Auth

Automatically read credentials from `~/.netrc`:

```python
with httpxr.Client(auth=httpxr.NetRCAuth()) as client:
    r = client.get("https://example.com/api")
```
