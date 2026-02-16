"""
Example: httpr.AsyncClient.gather() and paginate() — async versions

The async client provides the same gather() and paginate() extensions,
returning coroutines that can be awaited in an async context.

This is an httpr extension — not available in httpx.
"""

import asyncio
import time

import httpr


async def async_gather_basic() -> None:
    """Send 10 requests concurrently with AsyncClient."""
    async with httpr.AsyncClient() as client:
        requests = [
            client.build_request("GET", f"https://httpbin.org/get?id={i}")
            for i in range(10)
        ]

        start = time.perf_counter()
        responses = await client.gather(requests)
        elapsed = time.perf_counter() - start

        print(f"Sent {len(responses)} requests in {elapsed:.2f}s")
        for i, resp in enumerate(responses):
            print(f"  [{i}] {resp.status_code}")


async def async_gather_with_concurrency() -> None:
    """Limit concurrent requests with max_concurrency."""
    async with httpr.AsyncClient() as client:
        requests = [
            client.build_request("GET", "https://httpbin.org/delay/0.5")
            for _ in range(20)
        ]

        start = time.perf_counter()
        responses = await client.gather(requests, max_concurrency=5)  # noqa: F841
        elapsed = time.perf_counter() - start

        print(f"20 requests with max_concurrency=5: {elapsed:.2f}s")


async def async_gather_error_handling() -> None:
    """Handle errors in async gather."""
    async with httpr.AsyncClient() as client:
        requests = [
            client.build_request("GET", "https://httpbin.org/status/200"),
            client.build_request("GET", "https://httpbin.org/status/500"),
            client.build_request("GET", "https://httpbin.org/get"),
        ]

        # return_exceptions=True returns errors inline instead of raising
        responses = await client.gather(requests, return_exceptions=True)
        for i, resp in enumerate(responses):
            if isinstance(resp, Exception):
                print(f"  [{i}] ERROR: {resp}")
            else:
                print(f"  [{i}] {resp.status_code}")


async def async_paginate() -> None:
    """Auto-follow pagination with async client."""
    async with httpr.AsyncClient() as client:
        pages = await client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/issues",
            next_header="link",
            max_pages=3,
        )

        total = 0
        for i, page in enumerate(pages):
            items = page.json()
            total += len(items)
            print(f"  Page {i + 1}: {len(items)} items")

        print(f"Total items: {total}")


async def main() -> None:
    print("=== Async Gather (Basic) ===")
    await async_gather_basic()

    print("\n=== Async Gather (Concurrency Limit) ===")
    await async_gather_with_concurrency()

    print("\n=== Async Gather (Error Handling) ===")
    await async_gather_error_handling()

    print("\n=== Async Paginate ===")
    await async_paginate()


if __name__ == "__main__":
    asyncio.run(main())
