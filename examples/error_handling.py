"""
Error Handling
==============

Demonstrates httpr's exception hierarchy and how to handle HTTP errors gracefully.

Exception hierarchy:
    HTTPError
    ├── HTTPStatusError        (4xx / 5xx responses)
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
"""

import httpr


def main() -> None:
    # ── raise_for_status() ───────────────────────────────────────────────
    print("── raise_for_status() ─────────────────────────────────────────")
    with httpr.Client() as client:
        # 200 — no error
        response = client.get("https://httpbin.org/status/200")
        result = response.raise_for_status()
        print(f"  200: raise_for_status() returns the response → {result}")

        # 404 — raises HTTPStatusError
        response = client.get("https://httpbin.org/status/404")
        try:
            response.raise_for_status()
        except httpr.HTTPStatusError as exc:
            print(f"  404: Caught HTTPStatusError → {exc}")
            print(f"       request URL: {exc.request.url}")
            print(f"       response status: {exc.response.status_code}")

        # 500 — server error
        response = client.get("https://httpbin.org/status/500")
        try:
            response.raise_for_status()
        except httpr.HTTPStatusError as exc:
            print(f"  500: Caught HTTPStatusError → {exc}")
    print()

    # ── Checking status without exceptions ───────────────────────────────
    print("── Status code checks ─────────────────────────────────────────")
    with httpr.Client() as client:
        response = client.get("https://httpbin.org/status/201")
        print(f"  is_success:      {response.is_success}")
        print(f"  is_redirect:     {response.is_redirect}")
        print(f"  is_client_error: {response.is_client_error}")
        print(f"  is_server_error: {response.is_server_error}")
    print()

    # ── UnsupportedProtocol ──────────────────────────────────────────────
    print("── UnsupportedProtocol ────────────────────────────────────────")
    try:
        httpr.get("ftp://example.org")
    except httpr.UnsupportedProtocol as exc:
        print(f"  Caught: {exc}")
    print()

    # ── Timeout handling ─────────────────────────────────────────────────
    print("── Timeout handling ───────────────────────────────────────────")
    try:
        httpr.get("https://httpbin.org/delay/10", timeout=httpr.Timeout(1.0))
    except httpr.TimeoutException as exc:
        print(f"  Caught TimeoutException: {type(exc).__name__}")
    print()

    # ── Catch-all with HTTPError ─────────────────────────────────────────
    print("── Catch-all pattern ──────────────────────────────────────────")
    try:
        response = httpr.get("https://httpbin.org/status/503")
        response.raise_for_status()
    except httpr.HTTPError as exc:
        print(f"  Caught HTTPError (base class): {type(exc).__name__}: {exc}")


if __name__ == "__main__":
    main()
