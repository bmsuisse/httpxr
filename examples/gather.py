"""
Example: httpr.Client.gather() — concurrent batch requests

gather() dispatches multiple HTTP requests concurrently using Rust's tokio
runtime. Requests are built in Python, then sent in parallel with a single
GIL release. This is significantly faster than sending requests sequentially.

This is an httpr extension — not available in httpx.
"""

import time

import httpr


def basic_gather() -> None:
    """Send 10 requests concurrently."""
    with httpr.Client() as client:
        # Build a batch of requests
        requests = [
            client.build_request("GET", "https://httpbin.org/delay/1")
            for _ in range(10)
        ]

        start = time.perf_counter()
        responses = client.gather(requests)
        elapsed = time.perf_counter() - start

        print(f"Sent {len(responses)} requests in {elapsed:.2f}s")
        for i, resp in enumerate(responses):
            print(f"  [{i}] {resp.status_code}")


def gather_with_concurrency_limit() -> None:
    """Limit concurrency to 3 simultaneous requests."""
    with httpr.Client() as client:
        requests = [
            client.build_request("GET", f"https://httpbin.org/get?id={i}")
            for i in range(20)
        ]

        # max_concurrency controls how many run at once (default: 10)
        responses = client.gather(requests, max_concurrency=3)
        print(f"All {len(responses)} completed with max 3 concurrent")


def gather_with_error_handling() -> None:
    """Handle partial failures gracefully."""
    with httpr.Client() as client:
        requests = [
            client.build_request("GET", "https://httpbin.org/status/200"),
            client.build_request("GET", "https://httpbin.org/status/404"),
            client.build_request("GET", "https://httpbin.org/status/500"),
            client.build_request("GET", "https://httpbin.org/get"),
        ]

        # With return_exceptions=False (default), first error raises immediately
        # With return_exceptions=True, errors are returned in the result list
        responses = client.gather(requests, return_exceptions=True)
        for i, resp in enumerate(responses):
            if isinstance(resp, Exception):
                print(f"  [{i}] ERROR: {resp}")
            else:
                print(f"  [{i}] {resp.status_code}")


def gather_vs_sequential() -> None:
    """Benchmark: gather() vs sequential requests."""
    url = "https://httpbin.org/delay/0.5"
    n = 6

    with httpr.Client() as client:
        requests = [client.build_request("GET", url) for _ in range(n)]

        # Sequential
        start = time.perf_counter()
        for req in requests:
            client.send(req)
        seq_time = time.perf_counter() - start

        # Concurrent with gather()
        requests = [client.build_request("GET", url) for _ in range(n)]
        start = time.perf_counter()
        client.gather(requests)
        gather_time = time.perf_counter() - start

        print(f"Sequential: {seq_time:.2f}s")
        print(f"Gather:     {gather_time:.2f}s")
        print(f"Speedup:    {seq_time / gather_time:.1f}x")


def gather_mixed_methods() -> None:
    """Mix different HTTP methods in a single gather call."""
    with httpr.Client() as client:
        requests = [
            client.build_request("GET", "https://httpbin.org/get"),
            client.build_request(
                "POST", "https://httpbin.org/post", json={"key": "value"},
            ),
            client.build_request("PUT", "https://httpbin.org/put", content=b"hello"),
            client.build_request("DELETE", "https://httpbin.org/delete"),
        ]

        responses = client.gather(requests)
        for resp in responses:
            print(f"  {resp.request.method} → {resp.status_code}")


if __name__ == "__main__":
    print("=== Basic Gather ===")
    basic_gather()

    print("\n=== Concurrency Limit ===")
    gather_with_concurrency_limit()

    print("\n=== Error Handling ===")
    gather_with_error_handling()

    print("\n=== Gather vs Sequential ===")
    gather_vs_sequential()

    print("\n=== Mixed Methods ===")
    gather_mixed_methods()
