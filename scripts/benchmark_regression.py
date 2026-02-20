import asyncio
import time
import statistics
import sys
import httpxr
from starlette.applications import Starlette
from starlette.responses import Response as StarletteResponse
from starlette.routing import Route
import uvicorn
import threading


async def homepage(request):
    return StarletteResponse(b"Hello, world")


app = Starlette(routes=[Route("/", homepage)])


def run_server(port):
    uvicorn.run(app, host="127.0.0.1", port=port, log_level="error")


def benchmark_sync(url, rounds=500):
    client = httpxr.Client()
    # Warmup
    for _ in range(10):
        client.get(url)

    durations = []
    for _ in range(rounds):
        start = time.perf_counter()
        client.get(url)
        durations.append((time.perf_counter() - start) * 1000)

    client.close()
    return statistics.median(durations)


if __name__ == "__main__":
    port = 65432
    server_thread = threading.Thread(target=run_server, args=(port,), daemon=True)
    server_thread.start()
    time.sleep(1)  # Wait for server

    url = f"http://127.0.0.1:{port}/"
    median_ms = benchmark_sync(url)
    print(f"Median: {median_ms:.4f} ms")

    # Check if we should fail (e.g. if > 0.30)
    if median_ms > 0.30:
        print("REGRESSION DETECTED!")
        # sys.exit(1) # Don't exit yet, we are just testing
