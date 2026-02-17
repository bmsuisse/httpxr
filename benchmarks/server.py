"""
Shared ASGI test server for benchmarks.

Provides a simple ASGI app with endpoints for benchmarking and
a helper to spin up a uvicorn server in a background thread.
"""

from __future__ import annotations

import json
import socket
import threading
import time
import typing

from uvicorn.config import Config
from uvicorn.server import Server


# ---------------------------------------------------------------------------
# Types
# ---------------------------------------------------------------------------

Message = typing.Dict[str, typing.Any]
Receive = typing.Callable[[], typing.Awaitable[Message]]
Send = typing.Callable[
    [typing.Dict[str, typing.Any]], typing.Coroutine[None, None, None]
]
Scope = typing.Dict[str, typing.Any]


# ---------------------------------------------------------------------------
# ASGI app
# ---------------------------------------------------------------------------


async def app(scope: Scope, receive: Receive, send: Send) -> None:
    assert scope["type"] == "http"
    path = scope["path"]

    if path == "/json":
        await json_response(scope, receive, send)
    elif path == "/echo_body":
        await echo_body(scope, receive, send)
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
# Server helpers
# ---------------------------------------------------------------------------


def _find_free_port() -> int:
    """Find a free TCP port on localhost."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class BenchmarkServer(Server):
    def install_signal_handlers(self) -> None:
        pass  # Cannot install signal handlers outside main thread


def start_server() -> tuple[BenchmarkServer, threading.Thread, str]:
    """
    Start a uvicorn benchmark server on a free port.

    Returns (server, thread, base_url).
    The caller should call `stop_server(server, thread)` when done.
    """
    port = _find_free_port()
    config = Config(
        app=app,
        lifespan="off",
        loop="asyncio",
        host="127.0.0.1",
        port=port,
        log_level="warning",
    )
    srv = BenchmarkServer(config=config)
    thread = threading.Thread(target=srv.run, daemon=True)
    thread.start()
    while not srv.started:
        time.sleep(1e-3)
    return srv, thread, f"http://127.0.0.1:{port}"


def stop_server(srv: BenchmarkServer, thread: threading.Thread) -> None:
    """Gracefully stop the benchmark server."""
    srv.should_exit = True
    thread.join(timeout=5)
