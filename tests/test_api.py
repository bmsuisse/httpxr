import typing

import pytest

import httpxr


def test_get(server):
    response = httpxr.get(server.url)
    assert response.status_code == 200
    assert response.reason_phrase == "OK"
    assert response.text == "Hello, world!"
    assert response.http_version == "HTTP/1.1"


def test_post(server):
    response = httpxr.post(server.url, content=b"Hello, world!")
    assert response.status_code == 200
    assert response.reason_phrase == "OK"


def test_post_byte_iterator(server):
    def data() -> typing.Iterator[bytes]:
        yield b"Hello"
        yield b", "
        yield b"world!"

    response = httpxr.post(server.url, content=data())
    assert response.status_code == 200
    assert response.reason_phrase == "OK"


def test_post_byte_stream(server):
    class Data(httpxr.SyncByteStream):
        def __iter__(self):
            yield b"Hello"
            yield b", "
            yield b"world!"

    response = httpxr.post(server.url, content=Data())
    assert response.status_code == 200
    assert response.reason_phrase == "OK"


def test_options(server):
    response = httpxr.options(server.url)
    assert response.status_code == 200
    assert response.reason_phrase == "OK"


def test_head(server):
    response = httpxr.head(server.url)
    assert response.status_code == 200
    assert response.reason_phrase == "OK"


def test_put(server):
    response = httpxr.put(server.url, content=b"Hello, world!")
    assert response.status_code == 200
    assert response.reason_phrase == "OK"


def test_patch(server):
    response = httpxr.patch(server.url, content=b"Hello, world!")
    assert response.status_code == 200
    assert response.reason_phrase == "OK"


def test_delete(server):
    response = httpxr.delete(server.url)
    assert response.status_code == 200
    assert response.reason_phrase == "OK"


def test_stream(server):
    with httpxr.stream("GET", server.url) as response:
        response.read()

    assert response.status_code == 200
    assert response.reason_phrase == "OK"
    assert response.text == "Hello, world!"
    assert response.http_version == "HTTP/1.1"


def test_get_invalid_url():
    with pytest.raises(httpxr.UnsupportedProtocol):
        httpxr.get("invalid://example.org")


# check that httpcore isn't imported until we do a request
@pytest.mark.skip(reason="httpxr uses a Rust backend (ureq), not httpcore.")
def test_httpcore_lazy_loading(server):
    import sys

    # unload our module if it is already loaded
    if "httpxr" in sys.modules:
        del sys.modules["httpxr"]
    if "httpcore" in sys.modules:
        del sys.modules["httpcore"]
    import httpxr

    assert "httpcore" not in sys.modules
    _response = httpxr.get(server.url)
    assert "httpcore" in sys.modules
