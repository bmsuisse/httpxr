"""
Example: httpxr.Client.download() — direct file downloads

download() sends a GET request and writes the response body directly to
a file on disk. Returns the Response object for status/header inspection.

This is an httpxr extension — not available in httpx.
"""

import os
import tempfile

import httpxr


def basic_download() -> None:
    """Download a file to disk."""
    with httpxr.Client() as client:
        with tempfile.NamedTemporaryFile(delete=False, suffix=".json") as f:
            path = f.name

        resp = client.download("https://httpbin.org/json", path)
        size = os.path.getsize(path)
        print(f"✓ {resp.status_code} — {size:,} bytes → {path}")
        os.unlink(path)


def download_with_inspection() -> None:
    """Download and inspect response headers."""
    with httpxr.Client() as client:
        with tempfile.NamedTemporaryFile(delete=False, suffix=".html") as f:
            path = f.name

        resp = client.download("https://httpbin.org/html", path)
        print(f"Status:       {resp.status_code}")
        print(f"Content-Type: {resp.headers.get('content-type')}")
        print(f"File size:    {os.path.getsize(path):,} bytes")
        os.unlink(path)


def download_vs_stream() -> None:
    """Compare download() vs manual streaming."""
    import time

    url = "https://httpbin.org/bytes/100000"

    with httpxr.Client() as client:
        # Method 1: download() — one line
        with tempfile.NamedTemporaryFile(delete=False) as f:
            path = f.name
        start = time.perf_counter()
        client.download(url, path)
        t1 = time.perf_counter() - start
        print(f"download():  {t1:.3f}s ({os.path.getsize(path):,} bytes)")
        os.unlink(path)

        # Method 2: manual streaming
        with tempfile.NamedTemporaryFile(delete=False) as f:
            path = f.name
        start = time.perf_counter()
        with client.stream("GET", url) as response:
            with open(path, "wb") as fout:
                for chunk in response.iter_bytes():
                    fout.write(chunk)
        t2 = time.perf_counter() - start
        print(f"stream():    {t2:.3f}s ({os.path.getsize(path):,} bytes)")
        os.unlink(path)


if __name__ == "__main__":
    print("=== Basic Download ===")
    basic_download()

    print("\n=== Download with Inspection ===")
    download_with_inspection()

    print("\n=== download() vs stream() ===")
    download_vs_stream()
