"""
Async Client
============

Demonstrates ``AsyncClient`` for non-blocking HTTP requests using asyncio.
Ideal for high-concurrency workloads or integration with async frameworks.
"""

import asyncio

import httpxr


async def main() -> None:
    # ── Basic async GET ──────────────────────────────────────────────────
    async with httpxr.AsyncClient() as client:
        response = await client.get("https://httpbin.org/get")
        print(f"GET → {response.status_code}")
        print(f"  text preview: {response.text[:80]}...")
        print()

    # ── Concurrent requests with asyncio.gather ──────────────────────────
    async with httpxr.AsyncClient(base_url="https://httpbin.org") as client:
        responses = await asyncio.gather(
            client.get("/get"),
            client.get("/ip"),
            client.get("/user-agent"),
        )
        for r in responses:
            print(f"  {r.url} → {r.status_code}")
        print()

    # ── POST JSON ────────────────────────────────────────────────────────
    async with httpxr.AsyncClient() as client:
        response = await client.post(
            "https://httpbin.org/post",
            json={"message": "Hello from async httpxr!"},
        )
        print(f"POST → {response.status_code}")
        print(f"  echoed json: {response.json()['json']}")
        print()

    # ── Async streaming ──────────────────────────────────────────────────
    async with httpxr.AsyncClient() as client:
        async with client.stream("GET", "https://httpbin.org/get") as response:
            body = await response.aread()
        print(f"Streamed {len(body)} bytes, status {response.status_code}")


if __name__ == "__main__":
    asyncio.run(main())
