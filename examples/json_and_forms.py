"""
JSON, Form Data, and Multipart File Uploads
============================================

Sending structured data in different encodings.
"""

import io
import tempfile
from pathlib import Path

import httpxr


def main() -> None:
    # ── JSON body ────────────────────────────────────────────────────────
    response = httpxr.post(
        "https://httpbin.org/post",
        json={
            "name": "httpxr",
            "language": "Rust + Python",
            "version": httpxr.__version__,
        },
    )
    print("JSON POST:")
    print(f"  Content-Type sent: {response.request.headers['content-type']}")
    print(f"  Echoed JSON: {response.json()['json']}")
    print()

    # ── URL-encoded form data ────────────────────────────────────────────
    response = httpxr.post(
        "https://httpbin.org/post",
        data={"username": "admin", "password": "s3cret"},
    )
    print("Form POST:")
    print(f"  Content-Type sent: {response.request.headers['content-type']}")
    print(f"  Echoed form: {response.json()['form']}")
    print()

    # ── Multipart file upload (in-memory) ────────────────────────────────
    response = httpxr.post(
        "https://httpbin.org/post",
        files={
            "upload": ("hello.txt", io.BytesIO(b"Hello from httpxr!"), "text/plain")
        },
    )
    print("Multipart upload (in-memory):")
    print(f"  Files echoed: {response.json()['files']}")
    print()

    # ── Multipart with data + file ───────────────────────────────────────
    with tempfile.NamedTemporaryFile(suffix=".txt", delete=False) as tmp:
        tmp.write(b"Temporary file content")
        tmp_path = Path(tmp.name)

    with open(tmp_path, "rb") as f:
        response = httpxr.post(
            "https://httpbin.org/post",
            data={"description": "A test upload"},
            files={"document": (tmp_path.name, f, "text/plain")},
        )
    print("Multipart upload (data + file):")
    print(f"  Form: {response.json()['form']}")
    print(f"  Files: {list(response.json()['files'].keys())}")

    # Cleanup
    tmp_path.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
