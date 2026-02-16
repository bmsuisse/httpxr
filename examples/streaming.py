"""
Streaming Responses
===================

Download large responses efficiently by streaming chunks instead of loading
everything into memory at once.
"""

import asyncio

import httpr


def sync_streaming() -> None:
    """Stream with the synchronous Client."""
    print("── Sync streaming ─────────────────────────────────────────────")

    # stream() returns a context manager; the response body isn't fetched
    # until you explicitly iterate or call .read()
    with httpr.Client() as client:
        with client.stream("GET", "https://httpbin.org/stream-bytes/1024") as response:
            print(f"  Status: {response.status_code}")

            # Option 1: Read the whole body at once
            body = response.read()
            print(f"  Read {len(body)} bytes at once")

    # Option 2: Iterate over chunks
    with httpr.Client() as client:
        with client.stream("GET", "https://httpbin.org/stream-bytes/4096") as response:
            total = 0
            for chunk in response.iter_bytes():
                total += len(chunk)
            print(f"  Streamed {total} bytes in chunks")

    # Option 3: Raw bytes (no content decoding)
    with httpr.Client() as client:
        with client.stream("GET", "https://httpbin.org/get") as response:
            raw_total = 0
            for chunk in response.iter_raw():
                raw_total += len(chunk)
            print(f"  Raw streamed {raw_total} bytes")
    print()


async def async_streaming() -> None:
    """Stream with the async client."""
    print("── Async streaming ────────────────────────────────────────────")

    async with httpr.AsyncClient() as client:
        async with client.stream(
            "GET", "https://httpbin.org/stream-bytes/2048"
        ) as response:
            body = await response.aread()
            print(f"  Async read {len(body)} bytes")

    # Top-level stream() convenience function (sync only)
    with httpr.stream("GET", "https://httpbin.org/get") as response:
        response.read()
        print(
            f"  Top-level stream: {response.status_code},"
            f" {len(response.content)} bytes"
        )
    print()


def main() -> None:
    sync_streaming()
    asyncio.run(async_streaming())


if __name__ == "__main__":
    main()
