"""httpxr extensions — paginate, stream, gather, and OAuth2."""

from __future__ import annotations

import json as _json
import threading
import time
from collections.abc import AsyncGenerator, Callable, Iterator
from typing import Any

import httpxr  # noqa: TCH002

_SSE_SKIP_PREFIXES = (b"event:", b"id:", b"retry:")
_SSE_DONE = (b"[DONE]", b'"[DONE]"')
_ParserT = Callable[[bytes], Any]


def _extract_records(body: Any, records_key: str | None) -> list[Any]:
    if records_key is None:
        return [body]
    records = body[records_key] if isinstance(body, dict) else body
    return records if isinstance(records, list) else [records]


def _process_sse_line(raw: bytes) -> bytes | None:
    line = raw.strip()
    if not line:
        return None
    if line.startswith(b"data:"):
        line = line[5:].lstrip()
    if line in _SSE_DONE or line.startswith(_SSE_SKIP_PREFIXES):
        return None
    return line


def paginate_to_records(
    client: httpxr.Client,
    method: str,
    url: str | httpxr.URL,
    *,
    records_key: str | None = "value",
    next_url: str | None = None,
    next_header: str | None = None,
    next_func: Callable[[httpxr.Response], str | None] | None = None,
    max_pages: int = 100,
    params: Any | None = None,
    headers: dict[str, str] | None = None,
    timeout: float | httpxr.Timeout | None = None,
    **kwargs: Any,
) -> Iterator[Any]:
    """Lazily yield individual records from a paginated JSON API."""
    has_strategy = next_url is not None or next_header is not None or next_func is not None

    if not has_strategy:
        resp = client.request(method, url, params=params, headers=headers, timeout=timeout, **kwargs)
        resp.raise_for_status()
        yield from _extract_records(resp.json(), records_key)
        return

    page_iter = client.paginate(
        method, url,
        next_url=next_url, next_header=next_header, next_func=next_func,
        max_pages=max_pages, params=params, headers=headers, timeout=timeout,
        **kwargs,
    )
    for response in page_iter:
        response.raise_for_status()
        yield from _extract_records(response.json(), records_key)


async def apaginate_to_records(
    client: httpxr.AsyncClient,
    method: str,
    url: str | httpxr.URL,
    *,
    records_key: str | None = "value",
    next_url: str | None = None,
    next_header: str | None = None,
    next_func: Callable[[httpxr.Response], str | None] | None = None,
    max_pages: int = 100,
    params: Any | None = None,
    headers: dict[str, str] | None = None,
    timeout: float | httpxr.Timeout | None = None,
    **kwargs: Any,
) -> AsyncGenerator[Any, None]:
    """Async variant of :func:`paginate_to_records`."""
    has_strategy = next_url is not None or next_header is not None or next_func is not None

    if not has_strategy:
        resp = await client.request(method, url, params=params, headers=headers, timeout=timeout, **kwargs)
        resp.raise_for_status()
        for record in _extract_records(resp.json(), records_key):
            yield record
        return

    page_iter = client.paginate(
        method, url,
        next_url=next_url, next_header=next_header, next_func=next_func,
        max_pages=max_pages, params=params, headers=headers, timeout=timeout,
        **kwargs,
    )
    async for response in page_iter:  # type: ignore[union-attr]
        response.raise_for_status()
        for record in _extract_records(response.json(), records_key):
            yield record


def iter_json_bytes(response: httpxr.Response) -> Iterator[bytes]:
    """Stream NDJSON / SSE response body as raw bytes lines."""
    buf = b""
    for chunk in response.iter_bytes():
        buf += chunk
        while b"\n" in buf:
            raw_line, buf = buf.split(b"\n", 1)
            line = _process_sse_line(raw_line)
            if line is not None:
                yield line
    if buf:
        line = _process_sse_line(buf)
        if line is not None:
            yield line


async def aiter_json_bytes(response: httpxr.Response) -> AsyncGenerator[bytes, None]:
    """Async variant of :func:`iter_json_bytes`."""
    buf = b""
    async for chunk in response.aiter_bytes():  # type: ignore[union-attr]
        buf += chunk
        while b"\n" in buf:
            raw_line, buf = buf.split(b"\n", 1)
            line = _process_sse_line(raw_line)
            if line is not None:
                yield line
    if buf:
        line = _process_sse_line(buf)
        if line is not None:
            yield line


def gather_raw_bytes(
    client: httpxr.Client,
    requests: list[httpxr.Request],
    *,
    max_concurrency: int = 10,
    return_exceptions: bool = False,
    parser: _ParserT | None = None,
) -> list[Any]:
    """Concurrent batch requests returning parsed bodies."""
    if not requests:
        return []

    responses = client.gather(requests, max_concurrency=max_concurrency, return_exceptions=return_exceptions)
    out: list[Any] = []
    for item in responses:
        if isinstance(item, Exception):
            out.append(item)
            continue
        body: bytes = item.content
        if parser is None:
            out.append(body)
            continue
        try:
            out.append(parser(body))
        except Exception as exc:
            if return_exceptions:
                out.append(exc)
            else:
                raise
    return out


class OAuth2Auth(httpxr.Auth):
    """OAuth 2.0 client-credentials auth with automatic token refresh. Thread-safe."""

    def __init__(
        self,
        token_url: str,
        client_id: str,
        client_secret: str,
        scope: str = "",
        extra_params: dict[str, str] | None = None,
        leeway_seconds: int = 60,
    ) -> None:
        self._token_url = token_url
        self._client_id = client_id
        self._client_secret = client_secret
        self._scope = scope
        self._extra_params: dict[str, str] = extra_params or {}
        self._leeway = leeway_seconds
        self._token: str | None = None
        self._expires_at: float = 0.0
        self._lock = threading.Lock()

    def _is_expired(self) -> bool:
        return time.monotonic() >= self._expires_at - self._leeway

    def _build_token_data(self) -> dict[str, str]:
        data: dict[str, str] = {
            "grant_type": "client_credentials",
            "client_id": self._client_id,
            "client_secret": self._client_secret,
        }
        if self._scope:
            data["scope"] = self._scope
        data.update(self._extra_params)
        return data

    def _parse_token_response(self, body: bytes) -> str:
        payload = _json.loads(body)
        if "access_token" not in payload:
            raise RuntimeError(f"OAuth2Auth: token response missing 'access_token': {payload}")
        self._token = str(payload["access_token"])
        self._expires_at = time.monotonic() + int(payload.get("expires_in", 3600))
        return self._token

    def _refresh_sync(self) -> str:
        with httpxr.Client() as c:
            resp = c.post(self._token_url, data=self._build_token_data())
            resp.raise_for_status()
            return self._parse_token_response(resp.content)

    def _get_token_sync(self) -> str:
        with self._lock:
            if self._token is None or self._is_expired():
                self._refresh_sync()
        assert self._token is not None
        return self._token

    def auth_flow(self, request: httpxr.Request) -> Iterator[httpxr.Request]:
        request.headers["Authorization"] = f"Bearer {self._get_token_sync()}"
        response = yield request
        if response is not None and response.status_code == 401:
            with self._lock:
                self._token = None
            request.headers["Authorization"] = f"Bearer {self._get_token_sync()}"
            yield request

    async def _refresh_async(self) -> str:
        async with httpxr.AsyncClient() as c:
            resp = await c.post(self._token_url, data=self._build_token_data())
            resp.raise_for_status()
            return self._parse_token_response(resp.content)

    async def _get_token_async(self) -> str:
        if self._token is None or self._is_expired():
            await self._refresh_async()
        assert self._token is not None
        return self._token

    async def async_auth_flow(self, request: httpxr.Request) -> AsyncGenerator[httpxr.Request, None]:
        request.headers["Authorization"] = f"Bearer {await self._get_token_async()}"
        response = yield request
        if response is not None and response.status_code == 401:
            self._token = None
            request.headers["Authorization"] = f"Bearer {await self._get_token_async()}"
            yield request
