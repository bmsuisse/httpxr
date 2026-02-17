"""
Client Usage
============

Using httpxr.Client for connection pooling, base URLs, and session-level config.
Always use ``with`` to ensure connections are properly closed.
"""

from datetime import timedelta

import httpxr


def main() -> None:
    # ── Basic client with context manager ────────────────────────────────
    with httpxr.Client() as client:
        response = client.get("https://httpbin.org/get")
        print(f"Status: {response.status_code}")
        print(f"Elapsed: {response.elapsed}")
        assert response.elapsed > timedelta(0)
        print()

    # ── Base URL — all requests are relative to this ─────────────────────
    with httpxr.Client(base_url="https://httpbin.org") as client:
        r1 = client.get("/get")
        r2 = client.post("/post", content=b"hello")
        print(f"Base URL GET:  {r1.url}")
        print(f"Base URL POST: {r2.url}")
        print()

    # ── Session-level headers ────────────────────────────────────────────
    headers = {"X-Custom-Header": "my-value", "Accept": "application/json"}
    with httpxr.Client(headers=headers) as client:
        response = client.get("https://httpbin.org/headers")
        echoed = response.json()["headers"]
        print(f"Custom header echoed: {echoed.get('X-Custom-Header')}")
        print()

    # ── Build and send a request manually ────────────────────────────────
    with httpxr.Client() as client:
        request = client.build_request("GET", "https://httpbin.org/get")
        request.headers["X-Injected"] = "after-build"
        response = client.send(request)
        print(f"Injected header: {response.json()['headers'].get('X-Injected')}")
        print()

    # ── Client lifecycle: is_closed ──────────────────────────────────────
    client = httpxr.Client()
    print(f"is_closed before close: {client.is_closed}")
    client.close()
    print(f"is_closed after close:  {client.is_closed}")


if __name__ == "__main__":
    main()
