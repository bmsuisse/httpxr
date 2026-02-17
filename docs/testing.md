# Testing

httpxr provides the same testing utilities as httpx, making it easy to write
tests without hitting the network.

## MockTransport

Create a mock transport to return predefined responses:

```python
import httpxr

def mock_handler(request: httpxr.Request) -> httpxr.Response:
    """Return different responses based on the request."""
    if request.url.path == "/users":
        return httpxr.Response(
            200,
            json={"users": [{"id": 1, "name": "Alice"}]},
        )
    elif request.url.path == "/error":
        return httpxr.Response(500, text="Internal Server Error")
    return httpxr.Response(404, text="Not Found")

# Sync
transport = httpxr.MockTransport(mock_handler)
with httpxr.Client(transport=transport) as client:
    response = client.get("https://api.example.com/users")
    assert response.status_code == 200
    assert response.json()["users"][0]["name"] == "Alice"
```

### Async Mock Transport

```python
async def async_handler(request: httpxr.Request) -> httpxr.Response:
    return httpxr.Response(200, json={"ok": True})

transport = httpxr.AsyncMockTransport(async_handler)
async with httpxr.AsyncClient(transport=transport) as client:
    response = await client.get("https://api.example.com/health")
    assert response.json()["ok"] is True
```

## Constructing Test Responses

Build `Response` objects directly for unit tests:

```python
# Simple text response
response = httpxr.Response(200, text="Hello!")

# JSON response
response = httpxr.Response(200, json={"key": "value"})

# With custom headers
response = httpxr.Response(
    201,
    content=b"Created",
    headers={"X-Request-Id": "abc-123"},
)

# Status code helpers
assert httpxr.Response(200).is_success
assert httpxr.Response(404).is_client_error
assert httpxr.Response(500).is_server_error
```

## ASGITransport

Test ASGI applications (FastAPI, Starlette, etc.) directly:

```python
from myapp import app  # Your FastAPI/Starlette app

transport = httpxr.ASGITransport(app=app)

async with httpxr.AsyncClient(
    transport=transport,
    base_url="http://testserver",
) as client:
    response = await client.get("/api/users")
    assert response.status_code == 200
```

## WSGITransport

Test WSGI applications (Flask, Django, etc.) directly:

```python
from myapp import app  # Your Flask/Django app

transport = httpxr.WSGITransport(app=app)

with httpxr.Client(
    transport=transport,
    base_url="http://testserver",
) as client:
    response = client.get("/api/users")
    assert response.status_code == 200
```

## pytest Example

```python
import pytest
import httpxr

@pytest.fixture
def client():
    def handler(request):
        return httpxr.Response(200, json={"status": "ok"})

    transport = httpxr.MockTransport(handler)
    with httpxr.Client(transport=transport, base_url="https://api.test") as c:
        yield c

def test_health_check(client):
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json()["status"] == "ok"
```
