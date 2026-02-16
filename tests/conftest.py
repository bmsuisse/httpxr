import asyncio
import json
import multiprocessing
import os
import threading
import time
import typing

import pytest
import trustme
from cryptography.hazmat.backends import default_backend
from cryptography.hazmat.primitives.serialization import (
    BestAvailableEncryption,
    Encoding,
    PrivateFormat,
    load_pem_private_key,
)
from uvicorn.config import Config
from uvicorn.server import Server

import httpr
from tests.concurrency import sleep


# httpr uses tokio runtime which is only compatible with asyncio, not trio
@pytest.fixture
def anyio_backend():
    return "asyncio"


def pytest_collection_modifyitems(items):
    """Skip trio-backend tests since httpr uses tokio runtime (asyncio-only)."""
    skip_trio = pytest.mark.skip(reason="httpr uses tokio runtime, incompatible with trio")
    for item in items:
        if "[trio]" in item.nodeid:
            item.add_marker(skip_trio)

ENVIRONMENT_VARIABLES = {
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "SSLKEYLOGFILE",
}


@pytest.fixture(scope="function", autouse=True)
def clean_environ():
    """Keeps os.environ clean for every test without having to mock os.environ"""
    original_environ = os.environ.copy()
    os.environ.clear()
    os.environ.update(
        {
            k: v
            for k, v in original_environ.items()
            if k not in ENVIRONMENT_VARIABLES and k.lower() not in ENVIRONMENT_VARIABLES
        }
    )
    yield
    os.environ.clear()
    os.environ.update(original_environ)


Message = typing.Dict[str, typing.Any]
Receive = typing.Callable[[], typing.Awaitable[Message]]
Send = typing.Callable[
    [typing.Dict[str, typing.Any]], typing.Coroutine[None, None, None]
]
Scope = typing.Dict[str, typing.Any]


async def app(scope: Scope, receive: Receive, send: Send) -> None:
    assert scope["type"] == "http"
    if scope["path"].startswith("/slow_response"):
        await slow_response(scope, receive, send)
    elif scope["path"].startswith("/status"):
        await status_code(scope, receive, send)
    elif scope["path"].startswith("/echo_body"):
        await echo_body(scope, receive, send)
    elif scope["path"].startswith("/echo_binary"):
        await echo_binary(scope, receive, send)
    elif scope["path"].startswith("/echo_headers"):
        await echo_headers(scope, receive, send)
    elif scope["path"].startswith("/redirect_301"):
        await redirect_301(scope, receive, send)
    elif scope["path"].startswith("/json"):
        await hello_world_json(scope, receive, send)
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


async def hello_world_json(scope: Scope, receive: Receive, send: Send) -> None:
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [[b"content-type", b"application/json"]],
        }
    )
    await send({"type": "http.response.body", "body": b'{"Hello": "world!"}'})


async def slow_response(scope: Scope, receive: Receive, send: Send) -> None:
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [[b"content-type", b"text/plain"]],
        }
    )
    await sleep(1.0)  # Allow triggering a read timeout.
    await send({"type": "http.response.body", "body": b"Hello, world!"})


async def status_code(scope: Scope, receive: Receive, send: Send) -> None:
    status_code = int(scope["path"].replace("/status/", ""))
    await send(
        {
            "type": "http.response.start",
            "status": status_code,
            "headers": [[b"content-type", b"text/plain"]],
        }
    )
    await send({"type": "http.response.body", "body": b"Hello, world!"})


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
            "headers": [[b"content-type", b"text/plain"]],
        }
    )
    await send({"type": "http.response.body", "body": body})


async def echo_binary(scope: Scope, receive: Receive, send: Send) -> None:
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
            "headers": [[b"content-type", b"application/octet-stream"]],
        }
    )
    await send({"type": "http.response.body", "body": body})


async def echo_headers(scope: Scope, receive: Receive, send: Send) -> None:
    body = {
        name.capitalize().decode(): value.decode()
        for name, value in scope.get("headers", [])
    }
    await send(
        {
            "type": "http.response.start",
            "status": 200,
            "headers": [[b"content-type", b"application/json"]],
        }
    )
    await send({"type": "http.response.body", "body": json.dumps(body).encode()})


async def redirect_301(scope: Scope, receive: Receive, send: Send) -> None:
    await send(
        {"type": "http.response.start", "status": 301, "headers": [[b"location", b"/"]]}
    )
    await send({"type": "http.response.body"})


@pytest.fixture(scope="session")
def cert_authority():
    return trustme.CA()


@pytest.fixture(scope="session")
def localhost_cert(cert_authority):
    return cert_authority.issue_cert("localhost")


@pytest.fixture(scope="session")
def cert_pem_file(localhost_cert):
    with localhost_cert.cert_chain_pems[0].tempfile() as tmp:
        yield tmp


@pytest.fixture(scope="session")
def cert_private_key_file(localhost_cert):
    with localhost_cert.private_key_pem.tempfile() as tmp:
        yield tmp


@pytest.fixture(scope="session")
def cert_encrypted_private_key_file(localhost_cert):
    # Deserialize the private key and then reserialize with a password
    private_key = load_pem_private_key(
        localhost_cert.private_key_pem.bytes(), password=None, backend=default_backend()
    )
    encrypted_private_key_pem = trustme.Blob(
        private_key.private_bytes(
            Encoding.PEM,
            PrivateFormat.TraditionalOpenSSL,
            BestAvailableEncryption(password=b"password"),
        )
    )
    with encrypted_private_key_pem.tempfile() as tmp:
        yield tmp


class TestServer(Server):
    async def serve(self, sockets=None):
        self.restart_requested = asyncio.Event()

        loop = asyncio.get_event_loop()
        tasks = {
            loop.create_task(super().serve(sockets=sockets)),
            loop.create_task(self.watch_restarts()),
        }
        await asyncio.wait(tasks)

    @property
    def url(self) -> httpr.URL:
        protocol = "https" if self.config.is_ssl else "http"
        port = self.servers[0].sockets[0].getsockname()[1]
        url = httpr.URL(f"{protocol}://{self.config.host}:{port}/")
        print(f"DEBUG: TestServer running at {url}")
        return url

    async def restart(self) -> None:  # pragma: no cover
        # This coroutine may be called from a different thread than the one the
        # server is running on, and from an async environment that's not asyncio.
        # For this reason, we use an event to coordinate with the server
        # instead of calling shutdown()/startup() directly, and should not make
        # any asyncio-specific operations.
        self.started = False
        self.restart_requested.set()
        while not self.started:
            await sleep(0.2)

    async def watch_restarts(self) -> None:  # pragma: no cover
        while True:
            if self.should_exit:
                return

            try:
                await asyncio.wait_for(self.restart_requested.wait(), timeout=0.1)
            except asyncio.TimeoutError:
                continue

            self.restart_requested.clear()
            await self.shutdown()
            await self.startup()


def _run_server(config, port_shared, ready_event, stop_event):
    try:
        # Recreate the server in the child process
        # We must use a fresh TestServer instance
        # import inside to avoid circular or pickling issues if any
        from tests.conftest import TestServer
        child_server = TestServer(config=config)
        
        # Patch startup to signal readiness and communicative port
        original_startup = child_server.startup
        async def patched_startup(sockets=None):
            await original_startup(sockets=sockets)
            # Notify parent of the actual port
            port_shared.value = child_server.servers[0].sockets[0].getsockname()[1]
            ready_event.set()
        
        child_server.startup = patched_startup
        
        # Start a thread to watch for the stop event
        def check_stop():
            while not stop_event.wait(timeout=0.1):
                pass
            child_server.should_exit = True
        
        import threading
        stop_watcher = threading.Thread(target=check_stop, daemon=True)
        stop_watcher.start()
        
        child_server.run()
    except Exception as e:
        print(f"ERROR: Server process failed: {e}")
        ready_event.set() # Don't hang the parent


def serve_in_process(server: TestServer) -> typing.Iterator[TestServer]:
    config = server.config
    port_shared = multiprocessing.Value("i", 0)
    ready_event = multiprocessing.Event()
    stop_event = multiprocessing.Event()

    process = multiprocessing.Process(
        target=_run_server, 
        args=(config, port_shared, ready_event, stop_event)
    )
    process.start()
    
    try:
        if not ready_event.wait(timeout=10):
            process.terminate()
            raise RuntimeError("Server failed to start within 10 seconds")
        
        if port_shared.value == 0:
             raise RuntimeError("Server reported port 0 - startup might have failed")

        # Mock the server object in the parent process so server.url works
        class DummySocket:
            def getsockname(self): return ("127.0.0.1", port_shared.value)
        class DummyServer:
            def __init__(self): self.sockets = [DummySocket()]
        server.servers = [DummyServer()]
        server.started = True
        
        yield server
    finally:
        stop_event.set()
        process.join(timeout=2)
        if process.is_alive():
            process.terminate()


@pytest.fixture(scope="session")
def server() -> typing.Iterator[TestServer]:
    config = Config(app=app, lifespan="off", loop="asyncio", host="127.0.0.1", port=0)
    server = TestServer(config=config)
    yield from serve_in_process(server)


@pytest.fixture(scope="session")
def https_server(
    cert_pem_file: str, cert_private_key_file: str
) -> typing.Iterator[TestServer]:
    config = Config(
        app=app,
        lifespan="off",
        loop="asyncio",
        ssl_certfile=cert_pem_file,
        ssl_keyfile=cert_private_key_file,
        host="localhost",
        port=0,
    )
    server = TestServer(config=config)
    yield from serve_in_process(server)
