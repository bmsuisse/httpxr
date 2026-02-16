"""
Response Objects & Testing Patterns
====================================

Demonstrates how to construct Response and Request objects directly —
useful for unit tests, mocking, and building test fixtures without
hitting the network.
"""

import httpr


def main() -> None:
    # ── Construct a Response manually ────────────────────────────────────
    print("── Manual Response construction ────────────────────────────────")
    response = httpr.Response(200, text="Hello, world!")
    print(f"  Status: {response.status_code}")
    print(f"  Text: {response.text}")
    print(f"  repr: {repr(response)}")
    print()

    # ── Response with bytes content ──────────────────────────────────────
    print("── Response with bytes ─────────────────────────────────────────")
    response = httpr.Response(200, content=b'{"key": "value"}')
    print(f"  Content: {response.content}")
    print(f"  Text: {response.text}")
    print()

    # ── Response with JSON ───────────────────────────────────────────────
    print("── Response with JSON ──────────────────────────────────────────")
    response = httpr.Response(
        200,
        json={"users": [{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]},
    )
    print(f"  JSON: {response.json()}")
    print()

    # ── Response with custom headers ─────────────────────────────────────
    print("── Response with headers ───────────────────────────────────────")
    response = httpr.Response(
        201,
        content=b"Created",
        headers={
            "X-Request-Id": "abc-123",
            "Content-Type": "text/plain",
        },
    )
    print(f"  Status: {response.status_code}")
    print(f"  X-Request-Id: {response.headers['X-Request-Id']}")
    print(f"  Content-Type: {response.headers['Content-Type']}")
    print()

    # ── Status code checks ───────────────────────────────────────────────
    print("── Status code properties ─────────────────────────────────────")
    for code in (200, 301, 404, 500):
        r = httpr.Response(code)
        print(
            f"  {code}: is_success={r.is_success}, "
            f"is_redirect={r.is_redirect}, "
            f"is_client_error={r.is_client_error}, "
            f"is_server_error={r.is_server_error}"
        )
    print()

    # ── raise_for_status ─────────────────────────────────────────────────
    print("── raise_for_status ───────────────────────────────────────────")
    ok = httpr.Response(200, text="OK")
    result = ok.raise_for_status()
    print(f"  200: returns the response → {repr(result)}")

    err = httpr.Response(404, text="Not Found")
    try:
        err.raise_for_status()
    except httpr.HTTPStatusError as exc:
        print(f"  404: Caught HTTPStatusError → {exc}")
    print()

    # ── Construct a Request manually ─────────────────────────────────────
    print("── Manual Request construction ─────────────────────────────────")
    request = httpr.Request("GET", "https://example.com/api/users")
    print(f"  Method: {request.method}")
    print(f"  URL: {request.url}")
    print(f"  Headers: {dict(request.headers)}")
    print()

    # ── Request with body and custom headers ─────────────────────────────
    request = httpr.Request(
        "POST",
        "https://example.com/api/data",
        content=b'{"hello": "world"}',
        headers={"Content-Type": "application/json"},
    )
    print(f"  POST body: {request.content}")
    print(f"  Content-Type: {request.headers['Content-Type']}")
    print()

    # ── Working with Headers ─────────────────────────────────────────────
    print("── Headers object ─────────────────────────────────────────────")
    headers = httpr.Headers(
        {
            "Content-Type": "text/html",
            "X-Custom": "value",
            "Accept": "application/json",
        }
    )
    print(f"  Content-Type: {headers['Content-Type']}")
    print(f"  Keys: {list(headers.keys())}")
    print()

    # ── Working with URL ─────────────────────────────────────────────────
    print("── URL object ─────────────────────────────────────────────────")
    url = httpr.URL("https://www.example.com:8080/path/to/resource?q=test#section")
    print(f"  Full: {url}")
    print(f"  Scheme: {url.scheme}")
    print(f"  Host: {url.host}")
    print(f"  Port: {url.port}")
    print(f"  Path: {url.path}")
    print(f"  Query: {url.query}")
    print(f"  Fragment: {url.fragment}")
    print()

    # ── QueryParams ──────────────────────────────────────────────────────
    print("── QueryParams object ─────────────────────────────────────────")
    params = httpr.QueryParams({"search": "httpr", "page": "1"})
    print(f"  Params: {params}")


if __name__ == "__main__":
    main()
