"""
Example: httpxr.AsyncClient.gather() and paginate() — async versions

The async client provides the same gather() and paginate() extensions.
paginate() returns an **async iterator** that you use with ``async for``.

This is an httpxr extension — not available in httpx.
"""

import asyncio
import time

import httpxr


async def async_gather_basic() -> None:
    """Send 10 requests concurrently with AsyncClient."""
    async with httpxr.AsyncClient() as client:
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
    async with httpxr.AsyncClient() as client:
        requests = [
            client.build_request("GET", "https://httpbin.org/delay/0.5")
            for _ in range(20)
        ]

        start = time.perf_counter()
        responses = await client.gather(requests, max_concurrency=5)
        elapsed = time.perf_counter() - start

        print(f"{len(responses)} requests with max_concurrency=5: {elapsed:.2f}s")


async def async_gather_error_handling() -> None:
    """Handle errors in async gather."""
    async with httpxr.AsyncClient() as client:
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


# ── Async paginate — uses async for ─────────────────────────────────────


async def async_paginate_iterate() -> None:
    """Auto-follow pagination with async for."""
    async with httpxr.AsyncClient() as client:
        total = 0
        async for page in client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/issues",
            next_header="link",
            max_pages=3,
        ):
            items = page.json()
            total += len(items)
            print(f"  Page: {len(items)} items")

        print(f"Total items: {total}")


async def async_paginate_collect() -> None:
    """Use collect() to gather all pages into a list."""
    async with httpxr.AsyncClient() as client:
        pages = await client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/issues",
            next_header="link",
            max_pages=2,
        ).collect()

        print(f"Collected {len(pages)} pages")


async def async_paginate_with_progress() -> None:
    """Track pagination progress with pages_fetched."""
    async with httpxr.AsyncClient() as client:
        paginator = client.paginate(
            "GET",
            "https://api.github.com/repos/python/cpython/commits",
            next_header="link",
            max_pages=3,
        )

        async for page in paginator:
            print(f"  Page {paginator.pages_fetched}: {page.status_code}")

        print(f"Total pages fetched: {paginator.pages_fetched}")


async def main() -> None:
    print("=== Async Gather (Basic) ===")
    await async_gather_basic()

    print("\n=== Async Gather (Concurrency Limit) ===")
    await async_gather_with_concurrency()

    print("\n=== Async Gather (Error Handling) ===")
    await async_gather_error_handling()

    print("\n=== Async Paginate (iterate) ===")
    await async_paginate_iterate()

    print("\n=== Async Paginate (collect) ===")
    await async_paginate_collect()

    print("\n=== Async Paginate (progress) ===")
    await async_paginate_with_progress()


if __name__ == "__main__":
    asyncio.run(main())
