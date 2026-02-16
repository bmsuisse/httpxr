"""
Timeouts and Connection Limits
==============================

Fine-tune connection behavior with Timeout and Limits objects.
"""

import httpr


def main() -> None:
    # ── Default timeout (5s for everything) ──────────────────────────────
    print("── Default timeout ────────────────────────────────────────────")
    with httpr.Client(timeout=5.0) as client:
        response = client.get("https://httpbin.org/get")
        print(f"  Status: {response.status_code}, elapsed: {response.elapsed}")
    print()

    # ── Granular timeout configuration ───────────────────────────────────
    print("── Granular timeouts ──────────────────────────────────────────")
    timeout = httpr.Timeout(
        connect=5.0,       # Time to establish a connection
        read=10.0,         # Time to receive a response
        write=5.0,         # Time to send the request
        pool=5.0,          # Time waiting for a connection from the pool
    )
    with httpr.Client(timeout=timeout) as client:
        response = client.get("https://httpbin.org/get")
        print(f"  Status: {response.status_code}")
    print()

    # ── Disable timeouts ─────────────────────────────────────────────────
    print("── No timeout ─────────────────────────────────────────────────")
    timeout_none = httpr.Timeout(None)
    with httpr.Client(timeout=timeout_none) as client:
        response = client.get("https://httpbin.org/get")
        print(f"  Status: {response.status_code} (no timeout)")
    print()

    # ── Per-request timeout override ─────────────────────────────────────
    print("── Per-request timeout ────────────────────────────────────────")
    with httpr.Client(timeout=2.0) as client:
        # Override timeout for a single slow endpoint
        response = client.get(
            "https://httpbin.org/delay/1",
            timeout=httpr.Timeout(5.0),
        )
        print(f"  Status: {response.status_code} (per-request 5s timeout)")
    print()

    # ── Connection pool limits ───────────────────────────────────────────
    print("── Connection limits ──────────────────────────────────────────")
    limits = httpr.Limits(
        max_connections=100,        # Max total connections in the pool
        max_keepalive_connections=20,  # Max idle keepalive connections
    )
    with httpr.Client(limits=limits) as client:
        response = client.get("https://httpbin.org/get")
        print(f"  Status: {response.status_code} (pooled)")
    print()

    # ── Timeout that triggers ────────────────────────────────────────────
    print("── Triggering a timeout ───────────────────────────────────────")
    try:
        httpr.get(
            "https://httpbin.org/delay/5",
            timeout=httpr.Timeout(1.0),
        )
    except httpr.TimeoutException as exc:
        print(f"  Caught: {type(exc).__name__}")


if __name__ == "__main__":
    main()
