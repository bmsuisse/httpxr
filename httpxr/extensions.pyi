"""Type stubs for httpxr.extensions."""

from __future__ import annotations

from collections.abc import AsyncGenerator, Callable, Iterator
from threading import Lock
from typing import Any

import httpxr

# ---------------------------------------------------------------------------
# paginate_to_records
# ---------------------------------------------------------------------------

def paginate_to_records(
    client: httpxr.Client,
    method: str,
    url: str | httpxr.URL,
    *,
    records_key: str | None = ...,
    next_url: str | None = ...,
    next_header: str | None = ...,
    next_func: Callable[[httpxr.Response], str | None] | None = ...,
    max_pages: int = ...,
    params: Any | None = ...,
    headers: dict[str, str] | None = ...,
    timeout: float | httpxr.Timeout | None = ...,
    **kwargs: Any,
) -> Iterator[Any]:
    """Lazily yield individual records from a paginated JSON API (sync)."""
    ...

async def apaginate_to_records(
    client: httpxr.AsyncClient,
    method: str,
    url: str | httpxr.URL,
    *,
    records_key: str | None = ...,
    next_url: str | None = ...,
    next_header: str | None = ...,
    next_func: Callable[[httpxr.Response], str | None] | None = ...,
    max_pages: int = ...,
    params: Any | None = ...,
    headers: dict[str, str] | None = ...,
    timeout: float | httpxr.Timeout | None = ...,
    **kwargs: Any,
) -> AsyncGenerator[Any, None]:
    """Lazily yield individual records from a paginated JSON API (async)."""
    ...

# ---------------------------------------------------------------------------
# iter_json_bytes
# ---------------------------------------------------------------------------

def iter_json_bytes(
    response: httpxr.Response,
) -> Iterator[bytes]:
    """Stream NDJSON / SSE response as raw bytes lines — zero UTF-8 decode.

    Suitable for feeding directly into ``orjson.loads``, ``msgspec``, or
    ``spark.read.json(rdd)``.
    """
    ...

async def aiter_json_bytes(
    response: httpxr.Response,
) -> AsyncGenerator[bytes, None]:
    """Async streaming NDJSON / SSE response as raw bytes lines."""
    ...

# ---------------------------------------------------------------------------
# gather_raw_bytes
# ---------------------------------------------------------------------------

def gather_raw_bytes(
    client: httpxr.Client,
    requests: list[httpxr.Request],
    *,
    max_concurrency: int = ...,
    return_exceptions: bool = ...,
    parser: Callable[[bytes], Any] | None = ...,
) -> list[Any]:
    """Concurrent batch requests returning parsed bodies with minimum overhead.

    ``parser`` is applied to each response body bytes.  When ``None``,
    raw ``bytes`` are returned.
    """
    ...

# ---------------------------------------------------------------------------
# OAuth2Auth
# ---------------------------------------------------------------------------

class OAuth2Auth(httpxr.Auth):
    """OAuth 2.0 client-credentials authentication with automatic token refresh.

    Thread-safe.  Works with both :class:`~httpxr.Client` (sync) and
    :class:`~httpxr.AsyncClient` (async).
    """

    _token_url: str
    _client_id: str
    _client_secret: str
    _scope: str
    _extra_params: dict[str, str]
    _leeway: int
    _token: str | None
    _expires_at: float
    _lock: Lock

    def __init__(
        self,
        token_url: str,
        client_id: str,
        client_secret: str,
        scope: str = ...,
        extra_params: dict[str, str] | None = ...,
        leeway_seconds: int = ...,
    ) -> None: ...
    def _is_expired(self) -> bool: ...
    def _build_token_data(self) -> dict[str, str]: ...
    def _parse_token_response(self, body: bytes) -> str: ...
    def _refresh_sync(self) -> str: ...
    def _get_token_sync(self) -> str: ...
    def auth_flow(self, request: httpxr.Request) -> Iterator[httpxr.Request]: ...
    async def _refresh_async(self) -> str: ...
    async def _get_token_async(self) -> str: ...
    async def async_auth_flow(
        self, request: httpxr.Request
    ) -> AsyncGenerator[httpxr.Request, None]: ...
