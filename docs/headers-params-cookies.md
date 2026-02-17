# Headers, Params & Cookies

## Headers

The `Headers` object provides case-insensitive header access:

```python
import httpxr

response = httpxr.get("https://httpbin.org/get")

# Case-insensitive access
response.headers["content-type"]   # "application/json"
response.headers["Content-Type"]   # "application/json"

# Iterate
for name, value in response.headers.items():
    print(f"{name}: {value}")
```

### Sending Custom Headers

```python
# Per-request
response = client.get(
    "https://httpbin.org/headers",
    headers={"X-Custom": "value", "Accept": "application/json"},
)

# Client-level (applied to every request)
with httpxr.Client(headers={"User-Agent": "my-app/1.0"}) as client:
    response = client.get("https://httpbin.org/headers")
```

### Constructing Headers

```python
headers = httpxr.Headers({
    "Content-Type": "text/html",
    "X-Custom": "value",
    "Accept": "application/json",
})

print(headers["content-type"])  # "text/html" (case-insensitive)
print(list(headers.keys()))     # ["content-type", "x-custom", "accept"]
```

---

## Query Parameters

### Sending Parameters

```python
# As a dict
response = client.get(
    "https://httpbin.org/get",
    params={"search": "httpxr", "page": "1"},
)
print(response.url)  # https://httpbin.org/get?search=httpxr&page=1

# Multi-value parameters
response = client.get(
    "https://httpbin.org/get",
    params={"tag": ["python", "rust"]},
)
```

### The QueryParams Object

```python
params = httpxr.QueryParams({"search": "httpxr", "page": "1"})
print(str(params))            # "search=httpxr&page=1"
print(params["search"])       # "httpxr"
```

---

## Cookies

### Reading Cookies

```python
response = client.get("https://httpbin.org/cookies/set?name=value")
print(response.cookies["name"])  # "value"
```

### Sending Cookies

```python
# Per-request
response = client.get(
    "https://httpbin.org/cookies",
    cookies={"session": "abc123"},
)

# Client-level (persisted across requests)
with httpxr.Client(cookies={"session": "abc123"}) as client:
    r1 = client.get("https://httpbin.org/cookies")
    r2 = client.get("https://httpbin.org/cookies")
    # Both requests send the session cookie
```

### The Cookies Object

```python
cookies = httpxr.Cookies({"theme": "dark", "lang": "en"})
cookies.set("token", "xyz", domain="example.com")
```
