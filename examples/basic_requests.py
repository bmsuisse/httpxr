"""
Basic Requests
==============

Demonstrates the top-level convenience functions: get, post, put, patch, delete.
These create a one-off Client per call — great for scripts and quick tasks.
"""

import httpr


def main() -> None:
    # ── GET ──────────────────────────────────────────────────────────────
    response = httpr.get("https://httpbin.org/get")
    print(f"GET  → {response.status_code} {response.reason_phrase}")
    print(f"  URL:          {response.url}")
    print(f"  HTTP version: {response.http_version}")
    print(f"  Encoding:     {response.encoding}")
    print()

    # ── POST with raw bytes ──────────────────────────────────────────────
    response = httpr.post(
        "https://httpbin.org/post",
        content=b"Hello, world!",
    )
    print(f"POST → {response.status_code}")
    print(f"  Body echoed: {response.json()['data']}")
    print()

    # ── PUT ──────────────────────────────────────────────────────────────
    response = httpr.put(
        "https://httpbin.org/put",
        content=b"updated payload",
    )
    print(f"PUT  → {response.status_code}")
    print()

    # ── PATCH ────────────────────────────────────────────────────────────
    response = httpr.patch(
        "https://httpbin.org/patch",
        content=b"partial update",
    )
    print(f"PATCH → {response.status_code}")
    print()

    # ── DELETE ───────────────────────────────────────────────────────────
    response = httpr.delete("https://httpbin.org/delete")
    print(f"DELETE → {response.status_code}")


if __name__ == "__main__":
    main()
