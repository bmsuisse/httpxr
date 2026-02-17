"""
Timeouts and Connection Limits
==============================

Fine-tune connection behavior with Timeout and Limits objects.
"""

import httpxr


def main() -> None:
    # ── Default timeout (5s for everything) ──────────────────────────────
    print("── Default timeout ────────────────────────────────────────────")
    with httpxr.Client(timeout=5.0) as client:
        response = client.get("https://httpbin.org/get")
        print(f"  Status: {response.status_code}, elapsed: {response.elapsed}")
    print()

    # ── Granular timeout configuration ───────────────────────────────────
    print("── Granular timeouts ──────────────────────────────────────────")
    timeout = httpxr.Timeout(
        connect=5.0,  # Time to establish a connection
        read=10.0,  # Time to receive a response
        write=5.0,  # Time to send the request
        pool=5.0,  # Time waiting for a connection from the pool
    )
    with httpxr.Client(timeout=timeout) as client:
        response = client.get("https://httpbin.org/get")
        print(f"  Status: {response.status_code}")
    print()

    # ── Disable timeouts ─────────────────────────────────────────────────
    print("── No timeout ─────────────────────────────────────────────────")
    timeout_none = httpxr.Timeout(None)
    with httpxr.Client(timeout=timeout_none) as client:
        response = client.get("https://httpbin.org/get")
        print(f"  Status: {response.status_code} (no timeout)")
    print()

    # ── Per-request timeout override ─────────────────────────────────────
    print("── Per-request timeout ────────────────────────────────────────")
    with httpxr.Client(timeout=2.0) as client:
        # Override timeout for a single slow endpoint
        response = client.get(
            "https://httpbin.org/delay/1",
            timeout=httpxr.Timeout(5.0),
        )
        print(f"  Status: {response.status_code} (per-request 5s timeout)")
    print()

    # ── Connection pool limits ───────────────────────────────────────────
    print("── Connection limits ──────────────────────────────────────────")
    limits = httpxr.Limits(
        max_connections=100,  # Max total connections in the pool
        max_keepalive_connections=20,  # Max idle keepalive connections
    )
    with httpxr.Client(limits=limits) as client:
        response = client.get("https://httpbin.org/get")
        print(f"  Status: {response.status_code} (pooled)")
    print()

    # ── Timeout that triggers ────────────────────────────────────────────
    print("── Triggering a timeout ───────────────────────────────────────")
    try:
        httpxr.get(
            "https://httpbin.org/delay/5",
            timeout=httpxr.Timeout(1.0),
        )
    except httpxr.TimeoutException as exc:
        print(f"  Caught: {type(exc).__name__}")


if __name__ == "__main__":
    main()
