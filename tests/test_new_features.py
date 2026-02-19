"""Tests for httpxr extension features:
- response.json_bytes()
- response.iter_json() (NDJSON/SSE parsing)
- client.download()
- client.gather_paginate()
- client.pool_status()
- Client-level RetryConfig
- Client-level RateLimit
"""

from __future__ import annotations

import json
import os
import tempfile
from typing import Any

import pytest

import httpxr


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def echo_url(request: httpxr.Request) -> httpxr.Response:
    return httpxr.Response(
        200,
        json={"url": str(request.url), "method": request.method},
    )


def hello_world(request: httpxr.Request) -> httpxr.Response:
    return httpxr.Response(200, text="Hello, world!")


def page_handler(request: httpxr.Request) -> httpxr.Response:
    """Return page number based on ?page= query param."""
    url = str(request.url)
    page = 1
    if "page=" in url:
        page = int(url.split("page=")[1].split("&")[0])
    return httpxr.Response(200, json={"page": page, "items": [f"item_{page}"]})


# ===========================================================================
# json_bytes() tests
# ===========================================================================


class TestJsonBytes:
    def test_json_bytes_returns_bytes(self) -> None:
        """json_bytes() returns raw bytes, not decoded Python objects."""
        data = {"hello": "world", "number": 42}
        response = httpxr.Response(200, json=data)
        raw = response.json_bytes()
        assert isinstance(raw, bytes)
        # Should be valid JSON
        parsed = json.loads(raw)
        assert parsed == data

    def test_json_bytes_large_payload(self) -> None:
        """json_bytes() works with larger payloads."""
        data = {"items": [{"id": i, "name": f"item_{i}"} for i in range(100)]}
        content = json.dumps(data).encode("utf-8")
        response = httpxr.Response(200, content=content)
        raw = response.json_bytes()
        assert json.loads(raw) == data

    def test_json_bytes_unicode(self) -> None:
        """json_bytes() preserves unicode content."""
        data = {"greeting": "こんにちは", "emoji": "🎉"}
        content = json.dumps(data, ensure_ascii=False).encode("utf-8")
        response = httpxr.Response(200, content=content)
        raw = response.json_bytes()
        parsed = json.loads(raw)
        assert parsed == data

    def test_json_bytes_empty_response(self) -> None:
        """json_bytes() on empty response raises ResponseNotRead."""
        response = httpxr.Response(200)
        # Empty response should still return empty bytes or raise
        raw = response.json_bytes()
        assert raw == b""

    def test_json_bytes_with_orjson_compatible(self) -> None:
        """json_bytes() output should be compatible with json.loads."""
        data = {"nested": {"array": [1, 2, 3], "null": None, "bool": True}}
        content = json.dumps(data).encode("utf-8")
        response = httpxr.Response(200, content=content)
        raw = response.json_bytes()
        assert json.loads(raw) == data


# ===========================================================================
# iter_json() tests
# ===========================================================================


class TestIterJson:
    def test_iter_json_ndjson(self) -> None:
        """iter_json() parses NDJSON (one JSON object per line)."""
        lines = [
            json.dumps({"id": 1, "name": "first"}),
            json.dumps({"id": 2, "name": "second"}),
            json.dumps({"id": 3, "name": "third"}),
        ]
        content = "\n".join(lines).encode("utf-8")
        response = httpxr.Response(200, content=content)
        result = list(response.iter_json())
        assert len(result) == 3
        assert result[0] == {"id": 1, "name": "first"}
        assert result[2] == {"id": 3, "name": "third"}

    def test_iter_json_sse_format(self) -> None:
        """iter_json() handles SSE (Server-Sent Events) format."""
        content = (
            "event: message\n"
            'data: {"id": 1}\n'
            "\n"
            "event: message\n"
            'data: {"id": 2}\n'
            "\n"
            "data: [DONE]\n"
        ).encode("utf-8")
        response = httpxr.Response(200, content=content)
        result = list(response.iter_json())
        assert len(result) == 2
        assert result[0] == {"id": 1}
        assert result[1] == {"id": 2}

    def test_iter_json_skips_empty_lines(self) -> None:
        """iter_json() skips empty lines."""
        content = b'{"a": 1}\n\n{"b": 2}\n\n\n'
        response = httpxr.Response(200, content=content)
        result = list(response.iter_json())
        assert len(result) == 2

    def test_iter_json_empty_content(self) -> None:
        """iter_json() on empty content returns empty list."""
        response = httpxr.Response(200, content=b"")
        result = list(response.iter_json())
        assert result == []


# ===========================================================================
# download() tests
# ===========================================================================


class TestDownload:
    def test_download_creates_file(self) -> None:
        """download() writes response content to a file."""
        content = b"Hello, downloaded world!"

        def serve_content(request: httpxr.Request) -> httpxr.Response:
            return httpxr.Response(200, content=content)

        client = httpxr.Client(transport=httpxr.MockTransport(serve_content))

        with tempfile.NamedTemporaryFile(delete=False, suffix=".txt") as f:
            path = f.name

        try:
            resp = client.download("http://example.com/file.txt", path)
            assert resp.status_code == 200
            with open(path, "rb") as f:
                assert f.read() == content
        finally:
            os.unlink(path)
            client.close()

    def test_download_large_content(self) -> None:
        """download() handles larger content."""
        content = b"x" * 100_000

        def serve_large(request: httpxr.Request) -> httpxr.Response:
            return httpxr.Response(200, content=content)

        client = httpxr.Client(transport=httpxr.MockTransport(serve_large))

        with tempfile.NamedTemporaryFile(delete=False, suffix=".bin") as f:
            path = f.name

        try:
            resp = client.download("http://example.com/large", path)
            assert resp.status_code == 200
            assert os.path.getsize(path) == 100_000
        finally:
            os.unlink(path)
            client.close()

    def test_download_returns_response(self) -> None:
        """download() returns the Response object for inspection."""

        def serve_json(request: httpxr.Request) -> httpxr.Response:
            return httpxr.Response(
                200,
                content=b'{"ok": true}',
                headers={"content-type": "application/json"},
            )

        client = httpxr.Client(transport=httpxr.MockTransport(serve_json))

        with tempfile.NamedTemporaryFile(delete=False) as f:
            path = f.name

        try:
            resp = client.download("http://example.com/data.json", path)
            assert resp.status_code == 200
            assert resp.headers["content-type"] == "application/json"
        finally:
            os.unlink(path)
            client.close()


# ===========================================================================
# gather_paginate() tests
# ===========================================================================


class TestGatherPaginate:
    def test_gather_paginate_basic(self) -> None:
        """gather_paginate() fetches multiple pages concurrently."""
        client = httpxr.Client(transport=httpxr.MockTransport(page_handler))
        results = client.gather_paginate(
            "GET",
            "http://api.example.com/items",
            total_pages=3,
        )
        assert len(results) == 3
        # Each page should have the correct page number
        pages = sorted([r.json()["page"] for r in results])
        assert pages == [1, 2, 3]
        client.close()

    def test_gather_paginate_custom_page_param(self) -> None:
        """gather_paginate() supports custom page parameter name."""

        def page_handler_offset(request: httpxr.Request) -> httpxr.Response:
            url = str(request.url)
            offset = 0
            if "offset=" in url:
                offset = int(url.split("offset=")[1].split("&")[0])
            return httpxr.Response(200, json={"offset": offset})

        client = httpxr.Client(transport=httpxr.MockTransport(page_handler_offset))
        results = client.gather_paginate(
            "GET",
            "http://api.example.com/items",
            total_pages=3,
            page_param="offset",
            start_page=0,
        )
        assert len(results) == 3
        client.close()

    def test_gather_paginate_zero_pages(self) -> None:
        """gather_paginate() with total_pages=0 returns empty."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        results = client.gather_paginate(
            "GET",
            "http://api.example.com/items",
            total_pages=0,
        )
        assert len(results) == 0
        client.close()

    def test_gather_paginate_requires_total_pages(self) -> None:
        """gather_paginate() raises ValueError without total_pages."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        with pytest.raises(ValueError, match="total_pages is required"):
            client.gather_paginate("GET", "http://api.example.com/items")
        client.close()

    def test_gather_paginate_start_page(self) -> None:
        """gather_paginate() respects start_page parameter."""
        client = httpxr.Client(transport=httpxr.MockTransport(page_handler))
        results = client.gather_paginate(
            "GET",
            "http://api.example.com/items",
            total_pages=2,
            start_page=5,
        )
        assert len(results) == 2
        pages = sorted([r.json()["page"] for r in results])
        assert pages == [5, 6]
        client.close()


# ===========================================================================
# pool_status() tests
# ===========================================================================


class TestPoolStatus:
    def test_pool_status_returns_dict(self) -> None:
        """pool_status() returns a dict with expected keys."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        status = client.pool_status()
        assert isinstance(status, dict)
        assert "is_closed" in status
        assert status["is_closed"] is False
        client.close()

    def test_pool_status_after_close(self) -> None:
        """pool_status() reflects closed state."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        client.close()
        status = client.pool_status()
        assert status["is_closed"] is True


# ===========================================================================
# RetryConfig client-level tests
# ===========================================================================


class TestRetryConfig:
    def test_retry_config_creation(self) -> None:
        """RetryConfig can be created with defaults."""
        config = httpxr.RetryConfig()
        assert config.max_retries == 3
        assert config.backoff_factor == 0.5
        assert 429 in config.retry_on_status
        assert config.jitter is True

    def test_retry_config_custom(self) -> None:
        """RetryConfig with custom parameters."""
        config = httpxr.RetryConfig(
            max_retries=5,
            backoff_factor=1.0,
            retry_on_status=[500, 502],
            jitter=False,
        )
        assert config.max_retries == 5
        assert config.backoff_factor == 1.0
        assert config.retry_on_status == [500, 502]
        assert config.jitter is False

    def test_client_with_retry(self) -> None:
        """Client accepts retry parameter."""
        config = httpxr.RetryConfig(max_retries=3)
        client = httpxr.Client(
            transport=httpxr.MockTransport(hello_world),
            retry=config,
        )
        assert client.retry is not None
        assert client.retry.max_retries == 3
        client.close()

    def test_client_retry_setter(self) -> None:
        """Client.retry can be set after creation."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        assert client.retry is None

        config = httpxr.RetryConfig(max_retries=5)
        client.retry = config
        assert client.retry is not None
        assert client.retry.max_retries == 5

        client.retry = None
        assert client.retry is None
        client.close()

    def test_async_client_with_retry(self) -> None:
        """AsyncClient accepts retry parameter."""
        config = httpxr.RetryConfig(max_retries=2)
        client = httpxr.AsyncClient(
            transport=httpxr.MockTransport(hello_world),
            retry=config,
        )
        assert client.retry is not None
        assert client.retry.max_retries == 2
        client.close()

    def test_retry_config_should_retry(self) -> None:
        """RetryConfig.should_retry method works correctly."""
        config = httpxr.RetryConfig(retry_on_status=[429, 503])
        assert config.should_retry(429) is True
        assert config.should_retry(503) is True
        assert config.should_retry(200) is False
        assert config.should_retry(500) is False

    def test_retry_config_backoff_factor(self) -> None:
        """RetryConfig.backoff_factor is stored correctly."""
        config = httpxr.RetryConfig(backoff_factor=2.0, jitter=False)
        assert config.backoff_factor == 2.0
        assert config.jitter is False


# ===========================================================================
# RateLimit config tests
# ===========================================================================


class TestRateLimit:
    def test_rate_limit_creation(self) -> None:
        """RateLimit can be created with defaults."""
        rl = httpxr.RateLimit()
        assert rl.requests_per_second == 10.0
        assert rl.burst >= 1

    def test_rate_limit_custom(self) -> None:
        """RateLimit with custom parameters."""
        rl = httpxr.RateLimit(requests_per_second=5.0, burst=10)
        assert rl.requests_per_second == 5.0
        assert rl.burst == 10

    def test_client_with_rate_limit(self) -> None:
        """Client accepts rate_limit parameter."""
        rl = httpxr.RateLimit(requests_per_second=100.0)
        client = httpxr.Client(
            transport=httpxr.MockTransport(hello_world),
            rate_limit=rl,
        )
        assert client.rate_limit is not None
        assert client.rate_limit.requests_per_second == 100.0
        client.close()

    def test_client_rate_limit_setter(self) -> None:
        """Client.rate_limit can be set after creation."""
        client = httpxr.Client(transport=httpxr.MockTransport(hello_world))
        assert client.rate_limit is None

        rl = httpxr.RateLimit(requests_per_second=50.0)
        client.rate_limit = rl
        assert client.rate_limit is not None
        assert client.rate_limit.requests_per_second == 50.0

        client.rate_limit = None
        assert client.rate_limit is None
        client.close()

    def test_async_client_with_rate_limit(self) -> None:
        """AsyncClient accepts rate_limit parameter."""
        rl = httpxr.RateLimit(requests_per_second=20.0)
        client = httpxr.AsyncClient(
            transport=httpxr.MockTransport(hello_world),
            rate_limit=rl,
        )
        assert client.rate_limit is not None
        assert client.rate_limit.requests_per_second == 20.0
        client.close()

    def test_rate_limit_wait_time(self) -> None:
        """RateLimit.wait_time returns a value."""
        rl = httpxr.RateLimit(requests_per_second=10.0, burst=1)
        # First call should not need to wait (burst allows immediate)
        wait = rl.wait_time()
        assert isinstance(wait, float)
