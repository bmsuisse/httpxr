"""
httpxr Extensions — fast, low-memory helpers for data ingestion.

Designed for Databricks, PySpark, and any high-throughput pipeline that
pulls data from REST APIs.  Pure Python — no Rust rebuild required.

Public API (also exported from ``httpxr`` package root):

* :func:`paginate_to_records`   — lazy record iterator over paginated APIs
* :func:`apaginate_to_records`  — async variant
* :func:`iter_json_bytes`       — NDJSON streaming as raw bytes (zero decode)
* :func:`aiter_json_bytes`      — async variant
* :func:`gather_raw_bytes`      — concurrent batch requests → bytes/parsed
* :class:`OAuth2Auth`           — client-credentials token with auto-refresh

Examples
--------
>>> import httpxr
>>> import httpxr.extensions            # also importable as a namespace
>>> from httpxr import paginate_to_records, iter_json_bytes, gather_raw_bytes
"""

from __future__ import annotations

import json as _json
import threading
import time
from collections.abc import AsyncGenerator, Callable, Iterator
from typing import Any

import httpxr  # noqa: TCH002

# ---------------------------------------------------------------------------
# paginate_to_records
# ---------------------------------------------------------------------------


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
    """Lazily yield individual records from a paginated JSON API.

    Unlike :meth:`~httpxr.Client.paginate` which yields full
    :class:`~httpxr.Response` objects, this iterator unpacks the records
    array from each page so the caller never holds an entire page in memory.

    Parameters
    ----------
    client:
        An open :class:`~httpxr.Client` instance.
    method:
        HTTP method (``"GET"``, ``"POST"`` …).
    url:
        URL of the first page.
    records_key:
        JSON key whose value is the list of records (default ``"value"``
        for OData / Microsoft Graph).  Pass ``None`` to yield the entire
        parsed page body as a single record.
    next_url:
        JSON key in the response body that contains the next page URL,
        e.g. ``"@odata.nextLink"`` for Microsoft Graph.
    next_header:
        HTTP response header to parse for ``rel="next"`` links,
        e.g. ``"link"`` for GitHub-style pagination.
    next_func:
        Custom callable ``(Response) -> str | None`` that extracts the next
        page URL from an arbitrary response.
    max_pages:
        Safety cap — stops after this many pages.
    params:
        Query parameters for the *first* request only.
    headers:
        Extra HTTP headers.
    timeout:
        Request timeout.
    **kwargs:
        Forwarded to the underlying request.

    Yields
    ------
    Any
        Individual records (JSON-decoded Python objects).

    Examples
    --------
    Microsoft Graph (OData):

    >>> with httpxr.Client(auth=oauth_auth) as client:
    ...     for user in paginate_to_records(
    ...         client, "GET",
    ...         "https://graph.microsoft.com/v1.0/users",
    ...         records_key="value",
    ...         next_url="@odata.nextLink",
    ...     ):
    ...         process(user)

    Salesforce SOQL:

    >>> with httpxr.Client() as client:
    ...     for record in paginate_to_records(
    ...         client, "GET", sf_query_url,
    ...         records_key="records",
    ...         next_func=lambda r: r.json().get("nextRecordsUrl"),
    ...         headers={"Authorization": f"Bearer {token}"},
    ...     ):
    ...         upsert_to_delta(record)
    """
    _has_strategy = (
        next_url is not None or next_header is not None or next_func is not None
    )
    if not _has_strategy:
        # No pagination strategy — single request, extract records, done.
        resp = client.request(
            method, url, params=params, headers=headers, timeout=timeout, **kwargs
        )
        resp.raise_for_status()
        body = resp.json()
        if records_key is None:
            yield body
            return
        records = body[records_key] if isinstance(body, dict) else body
        yield from (records if isinstance(records, list) else [records])
        return

    page_iter = client.paginate(
        method,
        url,
        next_url=next_url,
        next_header=next_header,
        next_func=next_func,
        max_pages=max_pages,
        params=params,
        headers=headers,
        timeout=timeout,
        **kwargs,
    )
    for response in page_iter:
        response.raise_for_status()
        body = response.json()
        if records_key is None:
            yield body
            continue
        records = body[records_key] if isinstance(body, dict) else body
        if isinstance(records, list):
            yield from records
        else:
            yield records


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
    """Async version of :func:`paginate_to_records`."""
    _has_strategy = (
        next_url is not None or next_header is not None or next_func is not None
    )
    if not _has_strategy:
        resp = await client.request(
            method, url, params=params, headers=headers, timeout=timeout, **kwargs
        )
        resp.raise_for_status()
        body = resp.json()
        if records_key is None:
            yield body
            return
        records = body[records_key] if isinstance(body, dict) else body
        items = records if isinstance(records, list) else [records]
        for record in items:
            yield record
        return

    page_iter = client.paginate(
        method,
        url,
        next_url=next_url,
        next_header=next_header,
        next_func=next_func,
        max_pages=max_pages,
        params=params,
        headers=headers,
        timeout=timeout,
        **kwargs,
    )
    async for response in page_iter:  # type: ignore[union-attr]
        response.raise_for_status()
        body = response.json()
        if records_key is None:
            yield body
            continue
        records = body[records_key] if isinstance(body, dict) else body
        if isinstance(records, list):
            for record in records:
                yield record
        else:
            yield records


# ---------------------------------------------------------------------------
# iter_json_bytes — NDJSON / SSE streaming with zero Python decode overhead
# ---------------------------------------------------------------------------

_SSE_SKIP_PREFIXES = (b"event:", b"id:", b"retry:")


def iter_json_bytes(
    response: httpxr.Response,
) -> Iterator[bytes]:
    """Stream NDJSON response body as raw ``bytes`` lines — zero decode.

    Each yielded value is a single JSON record as ``bytes`` with no UTF-8
    decoding.  Feed directly into ``orjson.loads()``, ``msgspec.json.decode()``,
    or PySpark ``spark.read.json(rdd)`` for maximum throughput.

    Handles:

    * Plain NDJSON (one JSON object per line)
    * SSE ``data:`` prefix stripping
    * ``[DONE]`` sentinel skipping (OpenAI-style)
    * SSE ``event:`` / ``id:`` / ``retry:`` line skipping
    * Blank-line skipping

    Parameters
    ----------
    response:
        A :class:`~httpxr.Response` whose body is NDJSON or SSE.

    Yields
    ------
    bytes
        Raw bytes for each non-empty JSON line.

    Examples
    --------
    >>> import orjson
    >>> with httpxr.Client() as client:
    ...     with client.stream("GET", ndjson_url) as r:
    ...         for line in iter_json_bytes(r):
    ...             record = orjson.loads(line)
    ...             batch.append(record)
    ...             if len(batch) >= 1000:
    ...                 spark_df = spark.createDataFrame(batch)
    ...                 batch = []
    """  # noqa: E501
    buf = b""
    for chunk in response.iter_bytes():
        buf += chunk
        while b"\n" in buf:
            raw_line, buf = buf.split(b"\n", 1)
            line = raw_line.strip()
            if not line:
                continue
            if line.startswith(b"data:"):
                line = line[5:].lstrip()
            if line in (b"[DONE]", b'"[DONE]"'):
                continue
            if line.startswith(_SSE_SKIP_PREFIXES):
                continue
            yield line
    if buf:
        line = buf.strip()
        if not line:
            return
        if line.startswith(b"data:"):
            line = line[5:].lstrip()
        if line and line not in (b"[DONE]", b'"[DONE]"'):
            if not line.startswith(_SSE_SKIP_PREFIXES):
                yield line


async def aiter_json_bytes(
    response: httpxr.Response,
) -> AsyncGenerator[bytes, None]:
    """Async version of :func:`iter_json_bytes`.

    Streams NDJSON / SSE response bytes without UTF-8 decoding.
    """
    buf = b""
    async for chunk in response.aiter_bytes():  # type: ignore[union-attr]
        buf += chunk
        while b"\n" in buf:
            raw_line, buf = buf.split(b"\n", 1)
            line = raw_line.strip()
            if not line:
                continue
            if line.startswith(b"data:"):
                line = line[5:].lstrip()
            if line in (b"[DONE]", b'"[DONE]"'):
                continue
            if line.startswith(_SSE_SKIP_PREFIXES):
                continue
            yield line
    if buf:
        line = buf.strip()
        if not line:
            return
        if line.startswith(b"data:"):
            line = line[5:].lstrip()
        if line and line not in (b"[DONE]", b'"[DONE]"'):
            if not line.startswith(_SSE_SKIP_PREFIXES):
                yield line


# ---------------------------------------------------------------------------
# gather_raw_bytes — concurrent batch → bytes (with optional parser)
# ---------------------------------------------------------------------------

_ParserT = Callable[[bytes], Any]


def gather_raw_bytes(
    client: httpxr.Client,
    requests: list[httpxr.Request],
    *,
    max_concurrency: int = 10,
    return_exceptions: bool = False,
    parser: _ParserT | None = None,
) -> list[Any]:
    """Concurrent batch requests returning parsed bodies with minimum overhead.

    Wraps :meth:`~httpxr.Client.gather_raw` and optionally parses each
    response body bytes with ``parser`` (e.g. ``orjson.loads``).

    Parameters
    ----------
    client:
        An open :class:`~httpxr.Client` instance.
    requests:
        List of :class:`~httpxr.Request` objects (build with
        :meth:`~httpxr.Client.build_request`).
    max_concurrency:
        Maximum in-flight requests (default ``10``).
    return_exceptions:
        If ``True``, network or parse errors are returned inline rather than
        raised.
    parser:
        Optional callable applied to each response body ``bytes``.
        Defaults to returning raw ``bytes``.

    Returns
    -------
    list[Any]
        One result per request, in input order.  Each element is the parsed
        value, raw ``bytes`` (when ``parser=None``), or an ``Exception``
        (when ``return_exceptions=True``).

    Examples
    --------
    >>> import orjson
    >>> with httpxr.Client() as client:
    ...     reqs = [
    ...         client.build_request("GET", f"https://api.example.com/item/{i}")
    ...         for i in range(500)
    ...     ]
    ...     records = gather_raw_bytes(client, reqs, parser=orjson.loads,
    ...                                max_concurrency=50)
    ...     df = spark.createDataFrame(records)
    """
    if not requests:
        return []

    responses = client.gather(
        requests,
        max_concurrency=max_concurrency,
        return_exceptions=return_exceptions,
    )

    out: list[Any] = []
    for item in responses:
        if isinstance(item, Exception):
            out.append(item)
            continue
        body: bytes = item.content
        if parser is None:
            out.append(body)
        else:
            try:
                out.append(parser(body))
            except Exception as exc:
                if return_exceptions:
                    out.append(exc)
                else:
                    raise
    return out


# ---------------------------------------------------------------------------
# OAuth2Auth — client-credentials with automatic token refresh
# ---------------------------------------------------------------------------


class OAuth2Auth(httpxr.Auth):
    """OAuth 2.0 client-credentials authentication with automatic token refresh.

    Maintains a cached access token and transparently refreshes it when it
    expires.  Thread-safe.  Works with both :class:`~httpxr.Client` (sync)
    and :class:`~httpxr.AsyncClient` (async).

    Parameters
    ----------
    token_url:
        Token endpoint URL.
    client_id:
        OAuth 2.0 client ID.
    client_secret:
        OAuth 2.0 client secret.
    scope:
        Space-separated scope string.
    extra_params:
        Additional form fields included in the token request.
    leeway_seconds:
        Seconds before actual expiry at which the token is treated as
        expired (default ``60``).

    Examples
    --------
    **Microsoft Graph / Azure AD:**

    >>> auth = OAuth2Auth(
    ...     token_url="https://login.microsoftonline.com/<tenant>/oauth2/v2.0/token",
    ...     client_id="...", client_secret="...",
    ...     scope="https://graph.microsoft.com/.default",
    ... )
    >>> with httpxr.Client(auth=auth) as client:
    ...     for user in paginate_to_records(
    ...         client, "GET", "https://graph.microsoft.com/v1.0/users",
    ...         records_key="value", next_url="@odata.nextLink",
    ...     ):
    ...         upsert_to_delta(user)

    **Salesforce:**

    >>> auth = OAuth2Auth(
    ...     token_url="https://login.salesforce.com/services/oauth2/token",
    ...     client_id="...", client_secret="...",
    ... )
    >>> with httpxr.Client(auth=auth) as client:
    ...     for record in paginate_to_records(
    ...         client, "GET", soql_url, records_key="records",
    ...         next_func=lambda r: r.json().get("nextRecordsUrl"),
    ...     ):
    ...         process(record)
    """

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

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

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
            raise RuntimeError(
                f"OAuth2Auth: token response missing 'access_token': {payload}"
            )
        self._token = str(payload["access_token"])
        expires_in = int(payload.get("expires_in", 3600))
        self._expires_at = time.monotonic() + expires_in
        return self._token

    # ------------------------------------------------------------------
    # Sync
    # ------------------------------------------------------------------

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

    def auth_flow(
        self, request: httpxr.Request
    ) -> Iterator[httpxr.Request]:
        token = self._get_token_sync()
        request.headers["Authorization"] = f"Bearer {token}"
        response = yield request
        # 401 → force-refresh and retry once
        if response is not None and response.status_code == 401:
            with self._lock:
                self._token = None
            token = self._get_token_sync()
            request.headers["Authorization"] = f"Bearer {token}"
            yield request

    # ------------------------------------------------------------------
    # Async
    # ------------------------------------------------------------------

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

    async def async_auth_flow(
        self, request: httpxr.Request
    ) -> AsyncGenerator[httpxr.Request, None]:
        token = await self._get_token_async()
        request.headers["Authorization"] = f"Bearer {token}"
        response = yield request
        if response is not None and response.status_code == 401:
            self._token = None
            token = await self._get_token_async()
            request.headers["Authorization"] = f"Bearer {token}"
            yield request
