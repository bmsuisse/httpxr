"""Tests for httpxr.extensions — big-data ingestion helpers."""

from __future__ import annotations

import json
import time
from typing import Any

import pytest

import httpxr
from httpxr.extensions import (
    OAuth2Auth,
    aiter_json_bytes,
    apaginate_to_records,
    gather_raw_bytes,
    iter_json_bytes,
    paginate_to_records,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def make_paginated_handler(
    pages: list[dict[str, Any]],
) -> Any:
    """MockTransport handler cycling through ``pages``."""
    call_count = [0]

    def handler(request: httpxr.Request) -> httpxr.Response:
        i = call_count[0]
        call_count[0] += 1
        if i >= len(pages):
            return httpxr.Response(200, json={"value": []})
        return httpxr.Response(200, json=pages[i])

    return handler


def ndjson_response(*records: dict[str, Any]) -> httpxr.Response:
    body = b"\n".join(json.dumps(r).encode() for r in records)
    return httpxr.Response(200, content=body)


# ===========================================================================
# paginate_to_records
# ===========================================================================


class TestPaginateToRecords:
    def test_single_page_odata(self) -> None:
        pages = [{"value": [{"id": 1}, {"id": 2}]}]
        with httpxr.Client(
            transport=httpxr.MockTransport(make_paginated_handler(pages))
        ) as c:
            records = list(
                paginate_to_records(c, "GET", "http://api/items", records_key="value")
            )
        assert records == [{"id": 1}, {"id": 2}]

    def test_multi_page_with_next_url(self) -> None:
        page1 = {"value": [{"id": 1}], "@odata.nextLink": "http://api/items?page=2"}
        page2 = {"value": [{"id": 2}]}

        def handler(request: httpxr.Request) -> httpxr.Response:
            if "page=2" in str(request.url):
                return httpxr.Response(200, json=page2)
            return httpxr.Response(200, json=page1)

        with httpxr.Client(transport=httpxr.MockTransport(handler)) as c:
            records = list(
                paginate_to_records(
                    c,
                    "GET",
                    "http://api/items",
                    records_key="value",
                    next_url="@odata.nextLink",
                )
            )
        assert [r["id"] for r in records] == [1, 2]

    def test_records_key_none_yields_whole_page(self) -> None:
        pages = [{"total": 10, "items": [1, 2, 3]}]
        with httpxr.Client(
            transport=httpxr.MockTransport(make_paginated_handler(pages))
        ) as c:
            records = list(
                paginate_to_records(c, "GET", "http://api/items", records_key=None)
            )
        assert len(records) == 1
        assert records[0]["total"] == 10

    def test_empty_value_array(self) -> None:
        pages = [{"value": []}]
        with httpxr.Client(
            transport=httpxr.MockTransport(make_paginated_handler(pages))
        ) as c:
            records = list(
                paginate_to_records(c, "GET", "http://api/items", records_key="value")
            )
        assert records == []

    def test_custom_records_key(self) -> None:
        pages = [{"items": [{"x": 1}, {"x": 2}]}]
        with httpxr.Client(
            transport=httpxr.MockTransport(make_paginated_handler(pages))
        ) as c:
            records = list(
                paginate_to_records(c, "GET", "http://api/items", records_key="items")
            )
        assert [r["x"] for r in records] == [1, 2]

    def test_max_pages_respected(self) -> None:
        def infinite(request: httpxr.Request) -> httpxr.Response:
            return httpxr.Response(
                200,
                json={
                    "value": [{"n": 1}],
                    "@odata.nextLink": "http://api/items?next=1",
                },
            )

        with httpxr.Client(transport=httpxr.MockTransport(infinite)) as c:
            records = list(
                paginate_to_records(
                    c,
                    "GET",
                    "http://api/items",
                    records_key="value",
                    next_url="@odata.nextLink",
                    max_pages=3,
                )
            )
        assert len(records) == 3

    def test_salesforce_records_key(self) -> None:
        pages = [{"records": [{"Id": "001"}, {"Id": "002"}], "done": True}]
        with httpxr.Client(
            transport=httpxr.MockTransport(make_paginated_handler(pages))
        ) as c:
            records = list(
                paginate_to_records(c, "GET", "http://sf/api", records_key="records")
            )
        assert len(records) == 2
        assert records[0]["Id"] == "001"

    def test_custom_next_func(self) -> None:
        page1 = {"results": [{"id": 1}], "cursor": "abc"}
        page2 = {"results": [{"id": 2}], "cursor": None}

        def handler(req: httpxr.Request) -> httpxr.Response:
            if "cursor=abc" in str(req.url):
                return httpxr.Response(200, json=page2)
            return httpxr.Response(200, json=page1)

        def next_fn(resp: httpxr.Response) -> str | None:
            cursor = resp.json().get("cursor")
            if cursor:
                return f"http://api/items?cursor={cursor}"
            return None

        with httpxr.Client(transport=httpxr.MockTransport(handler)) as c:
            records = list(
                paginate_to_records(
                    c,
                    "GET",
                    "http://api/items",
                    records_key="results",
                    next_func=next_fn,
                )
            )
        assert [r["id"] for r in records] == [1, 2]


# ===========================================================================
# iter_json_bytes
# ===========================================================================


class TestIterJsonBytes:
    def test_plain_ndjson(self) -> None:
        r = ndjson_response({"id": 1}, {"id": 2}, {"id": 3})
        lines = list(iter_json_bytes(r))
        assert len(lines) == 3
        assert all(isinstance(ln, bytes) for ln in lines)
        assert json.loads(lines[0]) == {"id": 1}
        assert json.loads(lines[2]) == {"id": 3}

    def test_strips_data_prefix(self) -> None:
        body = b'data: {"msg": "hello"}\ndata: {"msg": "world"}\n'
        r = httpxr.Response(200, content=body)
        lines = list(iter_json_bytes(r))
        assert len(lines) == 2
        assert json.loads(lines[0])["msg"] == "hello"

    def test_skips_done_sentinel(self) -> None:
        body = b'data: {"id": 1}\ndata: [DONE]\n'
        r = httpxr.Response(200, content=body)
        lines = list(iter_json_bytes(r))
        assert len(lines) == 1

    def test_skips_empty_lines(self) -> None:
        body = b'{"a": 1}\n\n\n{"b": 2}\n'
        r = httpxr.Response(200, content=body)
        lines = list(iter_json_bytes(r))
        assert len(lines) == 2

    def test_skips_sse_event_id_retry_lines(self) -> None:
        body = b'event: message\ndata: {"x": 1}\nid: 1\nretry: 1000\n\n'
        r = httpxr.Response(200, content=body)
        lines = list(iter_json_bytes(r))
        assert len(lines) == 1
        assert json.loads(lines[0]) == {"x": 1}

    def test_empty_body(self) -> None:
        r = httpxr.Response(200, content=b"")
        assert list(iter_json_bytes(r)) == []

    def test_unicode_bytes_preserved(self) -> None:
        record = {"greeting": "こんにちは", "emoji": "🎉"}
        body = (json.dumps(record, ensure_ascii=False) + "\n").encode("utf-8")
        r = httpxr.Response(200, content=body)
        lines = list(iter_json_bytes(r))
        assert len(lines) == 1
        assert json.loads(lines[0]) == record

    def test_large_payload_1000_records(self) -> None:
        records = [{"id": i, "data": "x" * 100} for i in range(1000)]
        body = b"\n".join(json.dumps(r).encode() for r in records)
        resp = httpxr.Response(200, content=body)
        count = sum(1 for _ in iter_json_bytes(resp))
        assert count == 1000

    def test_no_trailing_newline(self) -> None:
        """Last line without newline is still yielded."""
        body = b'{"a": 1}\n{"b": 2}'
        r = httpxr.Response(200, content=body)
        lines = list(iter_json_bytes(r))
        assert len(lines) == 2


# ===========================================================================
# gather_raw_bytes
# ===========================================================================


class TestGatherRawBytes:
    def _handler(self, request: httpxr.Request) -> httpxr.Response:
        item_id = request.url.path.split("/")[-1]
        return httpxr.Response(200, json={"id": item_id})

    def test_returns_raw_bytes_by_default(self) -> None:
        with httpxr.Client(transport=httpxr.MockTransport(self._handler)) as c:
            reqs = [c.build_request("GET", f"http://api/item/{i}") for i in range(5)]
            results = gather_raw_bytes(c, reqs)
        assert len(results) == 5
        assert all(isinstance(r, bytes) for r in results)

    def test_with_json_parser(self) -> None:
        with httpxr.Client(transport=httpxr.MockTransport(self._handler)) as c:
            reqs = [c.build_request("GET", f"http://api/item/{i}") for i in range(5)]
            results = gather_raw_bytes(c, reqs, parser=json.loads)
        assert len(results) == 5
        assert all(isinstance(r, dict) for r in results)
        assert {r["id"] for r in results} == {"0", "1", "2", "3", "4"}

    def test_empty_requests(self) -> None:
        with httpxr.Client(transport=httpxr.MockTransport(self._handler)) as c:
            assert gather_raw_bytes(c, []) == []

    def test_max_concurrency_large_batch(self) -> None:
        with httpxr.Client(transport=httpxr.MockTransport(self._handler)) as c:
            reqs = [c.build_request("GET", f"http://api/item/{i}") for i in range(20)]
            results = gather_raw_bytes(c, reqs, max_concurrency=3, parser=json.loads)
        assert len(results) == 20

    def test_parser_exception_raises_by_default(self) -> None:
        def bad(b: bytes) -> Any:
            raise ValueError("bad!")

        with httpxr.Client(transport=httpxr.MockTransport(self._handler)) as c:
            reqs = [c.build_request("GET", "http://api/item/1")]
            with pytest.raises(ValueError, match="bad!"):
                gather_raw_bytes(c, reqs, parser=bad)

    def test_parser_exception_inlined_when_return_exceptions(self) -> None:
        def bad(b: bytes) -> Any:
            raise ValueError("parse error")

        with httpxr.Client(transport=httpxr.MockTransport(self._handler)) as c:
            reqs = [c.build_request("GET", f"http://api/item/{i}") for i in range(3)]
            results = gather_raw_bytes(c, reqs, parser=bad, return_exceptions=True)
        assert all(isinstance(r, Exception) for r in results)

    def test_preserves_order(self) -> None:
        """Results are in the same order as requests."""
        with httpxr.Client(transport=httpxr.MockTransport(self._handler)) as c:
            reqs = [c.build_request("GET", f"http://api/item/{i}") for i in range(10)]
            results = gather_raw_bytes(c, reqs, parser=json.loads, max_concurrency=10)
        ids = [r["id"] for r in results]
        assert ids == [str(i) for i in range(10)]


# ===========================================================================
# OAuth2Auth
# ===========================================================================


class TestOAuth2Auth:
    def test_injects_bearer_token(self) -> None:
        received: list[str] = []

        def api(request: httpxr.Request) -> httpxr.Response:
            received.append(request.headers.get("authorization", ""))
            return httpxr.Response(200, json={"ok": True})

        auth = OAuth2Auth("http://token/", "cid", "csec")
        auth._token = "test_token"
        auth._expires_at = time.monotonic() + 3600

        with httpxr.Client(transport=httpxr.MockTransport(api), auth=auth) as c:
            c.get("http://api/data")

        assert received[0] == "Bearer test_token"

    def test_token_fetched_when_empty(self) -> None:
        auth = OAuth2Auth("http://auth/token", "cid", "csec")
        refreshed = [False]

        def mock_refresh() -> str:
            refreshed[0] = True
            auth._token = "fresh"
            auth._expires_at = time.monotonic() + 3600
            return "fresh"

        auth._refresh_sync = mock_refresh  # type: ignore[method-assign]

        def api(request: httpxr.Request) -> httpxr.Response:
            return httpxr.Response(200, json={"ok": True})

        with httpxr.Client(transport=httpxr.MockTransport(api), auth=auth) as c:
            c.get("http://api/data")

        assert refreshed[0] is True

    def test_token_cached_across_requests(self) -> None:
        auth = OAuth2Auth("http://auth/token", "cid", "csec")
        auth._token = "cached"
        auth._expires_at = time.monotonic() + 7200
        refresh_count = [0]

        def mock_refresh() -> str:
            refresh_count[0] += 1
            return "new"

        auth._refresh_sync = mock_refresh  # type: ignore[method-assign]

        def api(request: httpxr.Request) -> httpxr.Response:
            return httpxr.Response(200, json={"ok": True})

        with httpxr.Client(transport=httpxr.MockTransport(api), auth=auth) as c:
            for _ in range(5):
                c.get("http://api/data")

        assert refresh_count[0] == 0

    def test_expired_token_triggers_refresh(self) -> None:
        auth = OAuth2Auth("http://auth/token", "cid", "csec")
        auth._token = "old"
        auth._expires_at = time.monotonic() - 1
        refreshed = [False]

        def mock_refresh() -> str:
            refreshed[0] = True
            auth._token = "new"
            auth._expires_at = time.monotonic() + 3600
            return "new"

        auth._refresh_sync = mock_refresh  # type: ignore[method-assign]

        def api(request: httpxr.Request) -> httpxr.Response:
            return httpxr.Response(200, json={"ok": True})

        with httpxr.Client(transport=httpxr.MockTransport(api), auth=auth) as c:
            c.get("http://api/data")

        assert refreshed[0] is True
        assert auth._token == "new"

    def test_leeway_triggers_early_refresh(self) -> None:
        auth = OAuth2Auth("http://auth/token", "cid", "csec", leeway_seconds=120)
        auth._token = "soon"
        auth._expires_at = time.monotonic() + 60  # < leeway 120 → expired

        assert auth._is_expired() is True

    def test_scope_in_token_data(self) -> None:
        auth = OAuth2Auth(
            "http://auth/token",
            "cid",
            "csec",
            scope="https://graph.microsoft.com/.default",
        )
        data = auth._build_token_data()
        assert data["scope"] == "https://graph.microsoft.com/.default"
        assert data["grant_type"] == "client_credentials"

    def test_extra_params_merged(self) -> None:
        auth = OAuth2Auth(
            "http://auth/token",
            "cid",
            "csec",
            extra_params={"resource": "https://management.azure.com/"},
        )
        data = auth._build_token_data()
        assert data["resource"] == "https://management.azure.com/"

    def test_missing_access_token_raises(self) -> None:
        auth = OAuth2Auth("http://xt", "c", "s")
        with pytest.raises(RuntimeError, match="missing 'access_token'"):
            auth._parse_token_response(b'{"error": "invalid_client"}')

    def test_token_parsed_correctly(self) -> None:
        auth = OAuth2Auth("http://xt", "c", "s")
        token = auth._parse_token_response(
            b'{"access_token": "abc123", "expires_in": 7200}'
        )
        assert token == "abc123"
        assert auth._token == "abc123"
        assert auth._expires_at > time.monotonic() + 7100


# ===========================================================================
# Package-level exports
# ===========================================================================


class TestPackageExports:
    def test_all_callable_from_root(self) -> None:
        assert callable(httpxr.paginate_to_records)
        assert callable(httpxr.apaginate_to_records)
        assert callable(httpxr.iter_json_bytes)
        assert callable(httpxr.aiter_json_bytes)
        assert callable(httpxr.gather_raw_bytes)
        assert httpxr.OAuth2Auth is not None

    def test_extensions_namespace_importable(self) -> None:
        import httpxr.extensions as ext

        assert hasattr(ext, "paginate_to_records")
        assert hasattr(ext, "iter_json_bytes")
        assert hasattr(ext, "gather_raw_bytes")
        assert hasattr(ext, "OAuth2Auth")
