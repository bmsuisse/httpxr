"""
Benchmark fixtures: shared test server + client factories for Python vs Rust httpx.

Terminology:
  - "python" = original pure-Python httpx from _reference/
  - "rust"   = httpr — the Rust-backed rewrite (current project)
"""

from __future__ import annotations

import json
import random
import socket
import sys
import threading
import time
import typing
from pathlib import Path

import pytest
from uvicorn.config import Config
from uvicorn.server import Server


# ---------------------------------------------------------------------------
# ASGI test app
# ---------------------------------------------------------------------------

Message = typing.Dict[str, typing.Any]
Receive = typing.Callable[[], typing.Awaitable[Message]]
Send = typing.Callable[
    [typing.Dict[str, typing.Any]], typing.Coroutine[None, None, None]
]
Scope = typing.Dict[str, typing.Any]


async def app(scope: Scope, receive: Receive, send: Send) -> None:
    assert scope["type"] == "http"
    path = scope["path"]

    if path == "/json":
        await json_response(scope, receive, send)
    elif path == "/echo_body":
        await echo_body(scope, receive, send)
    elif path == "/large_json":
        await large_json_response(scope, receive, send)
    elif path.startswith("/tabular_json"):
        await tabular_json_response(scope, receive, send)
    elif path.startswith("/nested_json"):
        await nested_json_response(scope, receive, send)
    else:
        await hello_world(scope, receive, send)


async def hello_world(scope: Scope, receive: Receive, send: Send) -> None:
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [[b"content-type", b"text/plain"]],
        }
    )
    await send({"type": "http.response.body", "body": b"Hello, world!"})


async def json_response(scope: Scope, receive: Receive, send: Send) -> None:
    payload = json.dumps({"message": "Hello", "numbers": list(range(100))}).encode()
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [
                [b"content-type", b"application/json"],
                [b"content-length", str(len(payload)).encode()],
            ],
        }
    )
    await send({"type": "http.response.body", "body": payload})


async def large_json_response(scope: Scope, receive: Receive, send: Send) -> None:
    payload = json.dumps({"data": "x" * 1_000_000}).encode()
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [
                [b"content-type", b"application/json"],
                [b"content-length", str(len(payload)).encode()],
            ],
        }
    )
    await send({"type": "http.response.body", "body": payload})


# Pre-generate tabular JSON payloads (array-of-objects, like a typical API)
_TABULAR_SIZES = {"small": 1_000, "medium": 10_000, "large": 50_000}
_TABULAR_CACHE: dict[str, bytes] = {}


def _get_tabular_payload(size_name: str) -> bytes:
    if size_name not in _TABULAR_CACHE:
        n = _TABULAR_SIZES[size_name]
        rng = random.Random(42)  # deterministic
        rows = [
            {
                "id": i,
                "name": f"item_{i}",
                "value": round(rng.uniform(0.0, 1000.0), 2),
                "score": rng.randint(0, 100),
                "active": rng.choice([True, False]),
                "category": rng.choice(["A", "B", "C", "D"]),
            }
            for i in range(n)
        ]
        _TABULAR_CACHE[size_name] = json.dumps(rows).encode()
    return _TABULAR_CACHE[size_name]


async def tabular_json_response(scope: Scope, receive: Receive, send: Send) -> None:
    # /tabular_json/small, /tabular_json/medium, /tabular_json/large
    parts = scope["path"].rstrip("/").split("/")
    size_name = parts[-1] if len(parts) > 1 else "small"
    if size_name not in _TABULAR_SIZES:
        size_name = "small"
    payload = _get_tabular_payload(size_name)
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [
                [b"content-type", b"application/json"],
                [b"content-length", str(len(payload)).encode()],
            ],
        }
    )
    await send({"type": "http.response.body", "body": payload})


# Pre-generate nested JSON payloads
_NESTED_CACHE: dict[str, bytes] = {}


def _build_nested_payload(variant: str) -> bytes:
    if variant not in _NESTED_CACHE:
        rng = random.Random(99)
        if variant == "shallow":
            # 1000 records, each with 2 levels of nesting
            rows = [
                {
                    "id": i,
                    "user": {
                        "name": f"user_{i}",
                        "email": f"u{i}@test.com",
                        "age": rng.randint(18, 80),
                    },
                    "address": {
                        "city": rng.choice(["Zurich", "Bern", "Basel", "Geneva"]),
                        "zip": str(rng.randint(1000, 9999)),
                        "country": "CH",
                    },
                    "score": round(rng.uniform(0, 100), 2),
                }
                for i in range(1_000)
            ]
        elif variant == "deep":
            # 500 records, 5 levels deep
            rows = []
            for i in range(500):
                rows.append(
                    {
                        "id": i,
                        "l1": {
                            "a": rng.randint(0, 100),
                            "l2": {
                                "b": f"v_{i}",
                                "l3": {
                                    "c": rng.random(),
                                    "l4": {
                                        "d": rng.choice([True, False]),
                                        "l5": {
                                            "val": rng.gauss(0, 1),
                                            "tag": rng.choice(["x", "y", "z"]),
                                        },
                                    },
                                },
                            },
                        },
                    }
                )
        else:  # wide
            # 500 records with many keys + nested arrays
            rows = []
            for i in range(500):
                rows.append(
                    {
                        "id": i,
                        "metrics": {
                            f"m{j}": round(rng.uniform(-100, 100), 4) for j in range(20)
                        },
                        "tags": [
                            rng.choice(["alpha", "beta", "gamma", "delta"])
                            for _ in range(10)
                        ],
                        "history": [
                            {
                                "ts": 1700000000 + k * 3600,
                                "val": round(rng.gauss(50, 15), 2),
                            }
                            for k in range(5)
                        ],
                    }
                )
        _NESTED_CACHE[variant] = json.dumps(rows).encode()
    return _NESTED_CACHE[variant]


async def nested_json_response(scope: Scope, receive: Receive, send: Send) -> None:
    parts = scope["path"].rstrip("/").split("/")
    variant = parts[-1] if len(parts) > 1 else "shallow"
    if variant not in ("shallow", "deep", "wide"):
        variant = "shallow"
    payload = _build_nested_payload(variant)
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [
                [b"content-type", b"application/json"],
                [b"content-length", str(len(payload)).encode()],
            ],
        }
    )
    await send({"type": "http.response.body", "body": payload})


async def echo_body(scope: Scope, receive: Receive, send: Send) -> None:
    body = b""
    more_body = True
    while more_body:
        message = await receive()
        body += message.get("body", b"")
        more_body = message.get("more_body", False)

    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [
                [b"content-type", b"application/octet-stream"],
                [b"content-length", str(len(body)).encode()],
            ],
        }
    )
    await send({"type": "http.response.body", "body": body})


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _find_free_port() -> int:
    """Find a free TCP port on localhost."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


# ---------------------------------------------------------------------------
# Test server running in a background thread
# ---------------------------------------------------------------------------


class BenchmarkServer(Server):
    def install_signal_handlers(self) -> None:
        pass  # Cannot install signal handlers outside main thread


@pytest.fixture(scope="session")
def server() -> typing.Iterator[BenchmarkServer]:
    port = _find_free_port()
    config = Config(
        app=app, lifespan="off", loop="asyncio", host="127.0.0.1", port=port
    )
    srv = BenchmarkServer(config=config)
    thread = threading.Thread(target=srv.run)
    thread.start()
    try:
        while not srv.started:
            time.sleep(1e-3)
        yield srv
    finally:
        srv.should_exit = True
        thread.join()


@pytest.fixture(scope="session")
def base_url(server: BenchmarkServer) -> str:
    return f"http://127.0.0.1:{server.config.port}"


# ---------------------------------------------------------------------------
# Reference (pure-Python) httpx — imported from _reference/
# ---------------------------------------------------------------------------


@pytest.fixture(scope="session")
def python_httpx() -> typing.Any:
    """Import the original pure-Python httpx from _reference/ as a standalone module."""
    import importlib

    ref_path = str(Path(__file__).resolve().parent.parent / "_reference")

    # Save and remove the existing httpx module (if any) to avoid conflicts
    saved_modules: dict[str, typing.Any] = {}
    keys_to_remove = [k for k in sys.modules if k == "httpx" or k.startswith("httpx.")]
    for k in keys_to_remove:
        saved_modules[k] = sys.modules.pop(k)

    # Prepend _reference so `import httpx` resolves to the pure-Python version
    sys.path.insert(0, ref_path)
    try:
        mod = importlib.import_module("httpx")
    finally:
        sys.path.remove(ref_path)
        # Keep the Python httpx cached under a private key
        python_httpx_modules = {
            k: sys.modules.pop(k)
            for k in list(sys.modules)
            if k == "httpx" or k.startswith("httpx.")
        }
        # Restore any previously-cached httpx (shouldn't be any, but just in case)
        sys.modules.update(saved_modules)

    return mod


@pytest.fixture(scope="session")
def python_client(
    python_httpx: typing.Any, base_url: str
) -> typing.Iterator[typing.Any]:
    """Sync Client from the original pure-Python httpx."""
    client = python_httpx.Client(base_url=base_url)
    yield client
    client.close()


# ---------------------------------------------------------------------------
# Rust-backed httpr — the current project
# ---------------------------------------------------------------------------


@pytest.fixture(scope="session")
def rust_httpx() -> typing.Any:
    """Import the Rust-backed httpr (the installed package)."""
    import httpr

    return httpr


@pytest.fixture(scope="session")
def rust_client(rust_httpx: typing.Any, base_url: str) -> typing.Iterator[typing.Any]:
    """Sync Client from the Rust-backed httpr."""
    client = rust_httpx.Client(base_url=base_url)
    yield client
    client.close()
