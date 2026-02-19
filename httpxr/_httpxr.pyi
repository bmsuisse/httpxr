"""Type stubs for the httpxr._httpxr Rust extension module."""

from __future__ import annotations

import datetime
from typing import (
    Any,
    AsyncIterator,
    Callable,
    Iterator,
    Mapping,
    Sequence,
)

# ---------------------------------------------------------------------------
# Version
# ---------------------------------------------------------------------------
__version__: str

# ---------------------------------------------------------------------------
# URL
# ---------------------------------------------------------------------------

class URL:
    scheme: str
    host: str
    port: int | None
    path: str
    query: bytes
    fragment: str | None
    raw_path: bytes
    authority: str
    netloc: str
    userinfo: str
    is_absolute_url: bool
    is_relative_url: bool

    def __init__(self, url: str | URL = ...) -> None: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def copy_with(
        self,
        *,
        scheme: str | None = ...,
        authority: str | None = ...,
        path: str | None = ...,
        query: str | bytes | None = ...,
        fragment: str | None = ...,
    ) -> URL: ...
    def copy_set_param(self, key: str, value: str) -> URL: ...
    def copy_add_param(self, key: str, value: str) -> URL: ...
    def copy_remove_param(self, key: str) -> URL: ...
    def copy_merge_params(self, params: Mapping[str, str] | URL) -> URL: ...
    def join(self, url: str | URL) -> URL: ...
    @property
    def params(self) -> QueryParams: ...
    @property
    def raw(self) -> dict[str, Any]: ...

class QueryParams:
    def __init__(self, params: Any = ...) -> None: ...
    def keys(self) -> list[str]: ...
    def values(self) -> list[str]: ...
    def items(self) -> list[tuple[str, str]]: ...
    def multi_items(self) -> list[tuple[str, str]]: ...
    def get(self, key: str, default: str | None = ...) -> str | None: ...
    def get_list(self, key: str) -> list[str]: ...
    def set(self, key: str, value: str) -> QueryParams: ...
    def add(self, key: str, value: str) -> QueryParams: ...
    def remove(self, key: str) -> QueryParams: ...
    def merge(self, params: Mapping[str, str] | QueryParams) -> QueryParams: ...
    def __contains__(self, key: str) -> bool: ...
    def __getitem__(self, key: str) -> str: ...
    def __len__(self) -> int: ...
    def __bool__(self) -> bool: ...
    def __iter__(self) -> Iterator[str]: ...
    def __eq__(self, other: object) -> bool: ...
    def __str__(self) -> str: ...
    def __repr__(self) -> str: ...
    def __hash__(self) -> int: ...
    def encode(self) -> str: ...

# ---------------------------------------------------------------------------
# Headers
# ---------------------------------------------------------------------------

class Headers:
    encoding: str

    def __init__(
        self,
        headers: (Mapping[str, str] | Sequence[tuple[str, str]] | Headers | None) = ...,
        encoding: str = ...,
    ) -> None: ...
    @property
    def raw(self) -> list[tuple[bytes, bytes]]: ...
    def keys(self) -> list[str]: ...
    def values(self) -> list[str]: ...
    def items(self) -> list[tuple[str, str]]: ...
    def multi_items(self) -> list[tuple[str, str]]: ...
    def get(self, key: str, default: str | None = ...) -> str | None: ...
    def get_list(self, key: str, split_commas: bool = ...) -> list[str]: ...
    def update(
        self,
        headers: (Mapping[str, str] | Sequence[tuple[str, str]] | Headers | None) = ...,
    ) -> None: ...
    def copy(self) -> Headers: ...
    def setdefault(self, key: str, default: str) -> str: ...
    def __getitem__(self, key: str) -> str: ...
    def __setitem__(self, key: str, value: str) -> None: ...
    def __delitem__(self, key: str) -> None: ...
    def __contains__(self, key: str) -> bool: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...
    def __bool__(self) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# Cookies
# ---------------------------------------------------------------------------

class Cookies:
    def __init__(self, cookies: Mapping[str, str] | Cookies | None = ...) -> None: ...
    def set(
        self, name: str, value: str, domain: str | None = ..., path: str | None = ...
    ) -> None: ...
    def get(
        self,
        name: str,
        default: str | None = ...,
        domain: str | None = ...,
        path: str | None = ...,
    ) -> str | None: ...
    def delete(
        self, name: str, domain: str | None = ..., path: str | None = ...
    ) -> None: ...
    def clear(self, domain: str | None = ..., path: str | None = ...) -> None: ...
    def update(self, other: Mapping[str, str] | Cookies | None = ...) -> None: ...
    def keys(self) -> list[str]: ...
    def values(self) -> list[str]: ...
    def items(self) -> list[tuple[str, str]]: ...
    def extract_cookies(self, response: Response) -> None: ...
    @property
    def jar(self) -> Any: ...
    def __getitem__(self, key: str) -> str: ...
    def __setitem__(self, key: str, value: str) -> None: ...
    def __delitem__(self, key: str) -> None: ...
    def __contains__(self, key: str) -> bool: ...
    def __iter__(self) -> Iterator[str]: ...
    def __len__(self) -> int: ...
    def __bool__(self) -> bool: ...
    def __repr__(self) -> str: ...

# ---------------------------------------------------------------------------
# Request
# ---------------------------------------------------------------------------

class Request:
    method: str
    url: URL
    headers: Headers

    def __init__(
        self,
        method: str,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | Sequence[tuple[str, str]] | Headers | None = ...,
        stream: Any | None = ...,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        extensions: dict[str, Any] | None = ...,
    ) -> None: ...
    @property
    def content(self) -> bytes: ...
    @property
    def stream(self) -> Any: ...
    def read(self) -> bytes: ...
    async def aread(self) -> bytes: ...
    def __repr__(self) -> str: ...
    def __eq__(self, other: object) -> bool: ...

# ---------------------------------------------------------------------------
# Response
# ---------------------------------------------------------------------------

class Response:
    status_code: int
    headers: Headers
    history: list[Response]

    def __init__(
        self,
        status_code: int,
        headers: Mapping[str, str] | Sequence[tuple[str, str]] | Headers | None = ...,
        stream: Any | None = ...,
        content: bytes | str | Any | None = ...,
        text: str | None = ...,
        html: str | None = ...,
        json: Any | None = ...,
        request: Request | None = ...,
        extensions: dict[str, Any] | None = ...,
        default_encoding: str | Callable[..., str] | None = ...,
        history: list[Response] | None = ...,
    ) -> None: ...
    @property
    def content(self) -> bytes: ...
    @property
    def text(self) -> str: ...
    @property
    def stream(self) -> Any: ...
    @property
    def url(self) -> URL: ...
    @property
    def encoding(self) -> str | None: ...
    @encoding.setter
    def encoding(self, value: str) -> None: ...
    @property
    def charset_encoding(self) -> str | None: ...
    @property
    def request(self) -> Request: ...
    @request.setter
    def request(self, value: Request) -> None: ...
    @property
    def extensions(self) -> dict[str, Any]: ...
    @extensions.setter
    def extensions(self, value: dict[str, Any]) -> None: ...
    @property
    def elapsed(self) -> datetime.timedelta: ...
    @property
    def reason_phrase(self) -> str: ...
    @property
    def http_version(self) -> str: ...
    @property
    def is_informational(self) -> bool: ...
    @property
    def is_success(self) -> bool: ...
    @property
    def is_redirect(self) -> bool: ...
    @property
    def is_client_error(self) -> bool: ...
    @property
    def is_server_error(self) -> bool: ...
    @property
    def is_error(self) -> bool: ...
    @property
    def has_redirect_location(self) -> bool: ...
    @property
    def is_closed(self) -> bool: ...
    @property
    def is_stream_consumed(self) -> bool: ...
    @property
    def num_bytes_downloaded(self) -> int: ...
    @property
    def cookies(self) -> Cookies: ...
    @property
    def links(self) -> dict[str, dict[str, str]]: ...
    @property
    def next_request(self) -> Request | None: ...
    @property
    def default_encoding(self) -> str: ...
    async def aiter_bytes(
        self, chunk_size: int | None = ...
    ) -> AsyncIterator[bytes]: ...
    def iter_bytes(self, chunk_size: int | None = ...) -> Iterator[bytes]: ...
    def iter_text(self, chunk_size: int | None = ...) -> Iterator[str]: ...
    async def aiter_text(self, chunk_size: int | None = ...) -> AsyncIterator[str]: ...
    def iter_lines(self) -> Iterator[str]: ...
    async def aiter_lines(self) -> AsyncIterator[str]: ...
    def json(self, **kwargs: Any) -> Any: ...
    def read(self) -> bytes: ...
    async def aread(self) -> bytes: ...
    def close(self) -> None: ...
    async def aclose(self) -> None: ...
    def raise_for_status(self) -> Response: ...
    def __eq__(self, other: object) -> bool: ...
    def __enter__(self) -> Response: ...
    def __exit__(
        self,
        exc_type: type | None = ...,
        exc_val: BaseException | None = ...,
        exc_tb: Any | None = ...,
    ) -> None: ...
    async def __aenter__(self) -> Response: ...
    async def __aexit__(
        self,
        exc_type: type | None = ...,
        exc_val: BaseException | None = ...,
        exc_tb: Any | None = ...,
    ) -> None: ...

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

class Timeout:
    connect: float | None
    read: float | None
    write: float | None
    pool: float | None

    def __init__(
        self,
        timeout: float | Timeout | tuple[float | None, ...] | None = ...,
        *,
        connect: float | None = ...,
        read: float | None = ...,
        write: float | None = ...,
        pool: float | None = ...,
    ) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...

class Limits:
    max_connections: int | None
    max_keepalive_connections: int | None
    keepalive_expiry: float | None

    def __init__(
        self,
        *,
        max_connections: int | None = ...,
        max_keepalive_connections: int | None = ...,
        keepalive_expiry: float | None = ...,
    ) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...

class Proxy:
    url: str
    auth: tuple[str, str] | None
    headers: Headers | None

    def __init__(
        self,
        url: str | URL,
        *,
        auth: tuple[str, str] | None = ...,
        headers: Mapping[str, str] | None = ...,
        ssl_context: Any | None = ...,
    ) -> None: ...
    def __repr__(self) -> str: ...

class RetryConfig:
    max_retries: int
    backoff_factor: float
    retry_on_status: list[int]
    jitter: bool

    def __init__(
        self,
        max_retries: int = ...,
        backoff_factor: float = ...,
        retry_on_status: list[int] | None = ...,
        jitter: bool = ...,
    ) -> None: ...
    def delay_for_attempt(self, attempt: int) -> float: ...
    def should_retry(self, status_code: int) -> bool: ...
    def __eq__(self, other: object) -> bool: ...
    def __repr__(self) -> str: ...

# Default config instances
DEFAULT_TIMEOUT_CONFIG: Timeout
DEFAULT_LIMITS: Limits
DEFAULT_MAX_REDIRECTS: int

def create_ssl_context(
    verify: bool | str | None = ...,
    cert: str | tuple[str, str] | None = ...,
    trust_env: bool | None = ...,
) -> Any: ...

# ---------------------------------------------------------------------------
# Auth
# ---------------------------------------------------------------------------

class Auth:
    """Base class for authentication."""

    ...

class BasicAuth:
    def __init__(self, username: str | bytes, password: str | bytes = ...) -> None: ...

class DigestAuth:
    def __init__(self, username: str | bytes, password: str | bytes = ...) -> None: ...

# ---------------------------------------------------------------------------
# Client
# ---------------------------------------------------------------------------

class Client:
    def __init__(
        self,
        *,
        auth: tuple[str, str] | Auth | Callable[..., Any] | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | Sequence[tuple[str, str]] | Headers | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        timeout: float | Timeout | None = ...,
        follow_redirects: bool = ...,
        max_redirects: int | None = ...,
        verify: bool | str | Any | None = ...,
        cert: str | None = ...,
        http2: bool = ...,
        proxy: str | Proxy | None = ...,
        limits: Limits | None = ...,
        mounts: Mapping[str, Any] | None = ...,
        transport: Any | None = ...,
        base_url: str | URL | None = ...,
        trust_env: bool = ...,
        default_encoding: str | Callable[..., str] | None = ...,
        event_hooks: dict[str, list[Callable[..., Any]]] | None = ...,
    ) -> None: ...

    # Properties
    @property
    def headers(self) -> Headers: ...
    @headers.setter
    def headers(
        self, value: Mapping[str, str] | Sequence[tuple[str, str]] | Headers
    ) -> None: ...
    @property
    def base_url(self) -> URL: ...
    @base_url.setter
    def base_url(self, value: str | URL) -> None: ...
    @property
    def auth(self) -> Any | None: ...
    @auth.setter
    def auth(
        self, value: tuple[str, str] | Auth | Callable[..., Any] | None
    ) -> None: ...
    @property
    def cookies(self) -> Cookies: ...
    @cookies.setter
    def cookies(self, value: Mapping[str, str] | Cookies) -> None: ...
    @property
    def timeout(self) -> Timeout | None: ...
    @timeout.setter
    def timeout(self, value: float | Timeout) -> None: ...
    @property
    def event_hooks(self) -> dict[str, list[Callable[..., Any]]]: ...
    @event_hooks.setter
    def event_hooks(self, value: dict[str, list[Callable[..., Any]]]) -> None: ...
    @property
    def trust_env(self) -> bool: ...
    @property
    def is_closed(self) -> bool: ...
    @property
    def params(self) -> QueryParams: ...
    @params.setter
    def params(self, value: Any) -> None: ...

    # Request methods
    def request(
        self,
        method: str,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def get(
        self,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def head(
        self,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def options(
        self,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def delete(
        self,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def post(
        self,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def put(
        self,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def patch(
        self,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def stream(
        self,
        method: str,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def build_request(
        self,
        method: str,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        extensions: dict[str, Any] | None = ...,
    ) -> Request: ...
    def send(
        self,
        request: Request,
        *,
        auth: Any | None = ...,
        follow_redirects: bool | None = ...,
    ) -> Response: ...
    def gather(
        self,
        requests: list[Request],
        *,
        max_concurrency: int = ...,
        return_exceptions: bool = ...,
    ) -> list[Response | Exception]: ...
    def paginate(
        self,
        method: str,
        url: str | URL,
        *,
        next_url: str | None = ...,
        next_header: str | None = ...,
        next_func: Callable[[Response], str | None] | None = ...,
        max_pages: int = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Iterator[Response]: ...

    # Raw methods (httpxr extension)
    def get_raw(
        self,
        url: str,
        *,
        headers: dict[str, str] | None = ...,
        timeout: float | None = ...,
    ) -> Any: ...
    def post_raw(
        self,
        url: str,
        *,
        headers: dict[str, str] | None = ...,
        body: bytes | None = ...,
        timeout: float | None = ...,
    ) -> Any: ...
    def put_raw(
        self,
        url: str,
        *,
        headers: dict[str, str] | None = ...,
        body: bytes | None = ...,
        timeout: float | None = ...,
    ) -> Any: ...
    def patch_raw(
        self,
        url: str,
        *,
        headers: dict[str, str] | None = ...,
        body: bytes | None = ...,
        timeout: float | None = ...,
    ) -> Any: ...
    def delete_raw(
        self,
        url: str,
        *,
        headers: dict[str, str] | None = ...,
        timeout: float | None = ...,
    ) -> Any: ...
    def head_raw(
        self,
        url: str,
        *,
        headers: dict[str, str] | None = ...,
        timeout: float | None = ...,
    ) -> Any: ...
    def close(self) -> None: ...
    def __enter__(self) -> Client: ...
    def __exit__(
        self,
        exc_type: type | None = ...,
        exc_val: BaseException | None = ...,
        exc_tb: Any | None = ...,
    ) -> None: ...

# ---------------------------------------------------------------------------
# AsyncClient
# ---------------------------------------------------------------------------

class AsyncClient:
    def __init__(
        self,
        *,
        auth: tuple[str, str] | Auth | Callable[..., Any] | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | Sequence[tuple[str, str]] | Headers | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        timeout: float | Timeout | None = ...,
        follow_redirects: bool = ...,
        max_redirects: int | None = ...,
        verify: bool | str | Any | None = ...,
        cert: str | None = ...,
        http2: bool = ...,
        proxy: str | Proxy | None = ...,
        limits: Limits | None = ...,
        mounts: Mapping[str, Any] | None = ...,
        transport: Any | None = ...,
        base_url: str | URL | None = ...,
        trust_env: bool = ...,
        default_encoding: str | Callable[..., str] | None = ...,
        event_hooks: dict[str, list[Callable[..., Any]]] | None = ...,
    ) -> None: ...

    # Properties
    @property
    def headers(self) -> Headers: ...
    @headers.setter
    def headers(
        self, value: Mapping[str, str] | Sequence[tuple[str, str]] | Headers
    ) -> None: ...
    @property
    def base_url(self) -> URL: ...
    @base_url.setter
    def base_url(self, value: str | URL) -> None: ...
    @property
    def auth(self) -> Any | None: ...
    @auth.setter
    def auth(
        self, value: tuple[str, str] | Auth | Callable[..., Any] | None
    ) -> None: ...
    @property
    def cookies(self) -> Cookies: ...
    @cookies.setter
    def cookies(self, value: Mapping[str, str] | Cookies) -> None: ...
    @property
    def timeout(self) -> Timeout | None: ...
    @property
    def event_hooks(self) -> dict[str, list[Callable[..., Any]]]: ...
    @event_hooks.setter
    def event_hooks(self, value: dict[str, list[Callable[..., Any]]]) -> None: ...
    @property
    def trust_env(self) -> bool: ...
    @property
    def is_closed(self) -> bool: ...

    # Async request methods
    async def request(
        self,
        method: str,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        stream: bool = ...,
        **kwargs: Any,
    ) -> Response: ...
    async def get(
        self,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    async def head(
        self,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    async def options(
        self,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    async def delete(
        self,
        url: str | URL,
        *,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    async def post(
        self,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    async def put(
        self,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    async def patch(
        self,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        follow_redirects: bool | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> Response: ...
    def build_request(
        self,
        method: str,
        url: str | URL,
        *,
        content: bytes | str | Any | None = ...,
        data: Mapping[str, Any] | None = ...,
        files: Any | None = ...,
        json: Any | None = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        extensions: dict[str, Any] | None = ...,
    ) -> Request: ...
    async def send(
        self,
        request: Request,
        *,
        stream: bool = ...,
        auth: Any | None = ...,
        follow_redirects: bool | None = ...,
    ) -> Response: ...
    async def gather(
        self,
        requests: list[Request],
        *,
        max_concurrency: int = ...,
        return_exceptions: bool = ...,
    ) -> list[Response | Exception]: ...
    async def paginate(
        self,
        method: str,
        url: str | URL,
        *,
        next_url: str | None = ...,
        next_header: str | None = ...,
        next_func: Callable[[Response], str | None] | None = ...,
        max_pages: int = ...,
        params: Any | None = ...,
        headers: Mapping[str, str] | None = ...,
        cookies: Mapping[str, str] | Cookies | None = ...,
        timeout: float | Timeout | None = ...,
        extensions: dict[str, Any] | None = ...,
        **kwargs: Any,
    ) -> AsyncIterator[Response]: ...
    async def aclose(self) -> None: ...
    def close(self) -> None: ...
    async def __aenter__(self) -> AsyncClient: ...
    async def __aexit__(
        self,
        exc_type: type | None = ...,
        exc_val: BaseException | None = ...,
        exc_tb: Any | None = ...,
    ) -> None: ...

# ---------------------------------------------------------------------------
# Transports
# ---------------------------------------------------------------------------

class BaseTransport:
    def __init__(self, *args: Any, **kwargs: Any) -> None: ...
    def handle_request(self, request: Request) -> Response: ...
    def close(self) -> None: ...
    def __enter__(self) -> BaseTransport: ...
    def __exit__(
        self,
        exc_type: type | None = ...,
        exc_val: BaseException | None = ...,
        exc_tb: Any | None = ...,
    ) -> None: ...

class AsyncBaseTransport:
    def __init__(self, *args: Any, **kwargs: Any) -> None: ...
    async def handle_async_request(self, request: Request) -> Response: ...
    async def aclose(self) -> None: ...
    async def __aenter__(self) -> AsyncBaseTransport: ...
    async def __aexit__(
        self,
        exc_type: type | None = ...,
        exc_val: BaseException | None = ...,
        exc_tb: Any | None = ...,
    ) -> None: ...

class MockTransport(BaseTransport):
    handler: Any
    def __init__(self, handler: Callable[..., Any]) -> None: ...
    def handle_request(self, request: Request) -> Response: ...
    async def handle_async_request(self, request: Request) -> Response: ...
    def close(self) -> None: ...
    def aclose(self) -> None: ...

class AsyncMockTransport(AsyncBaseTransport):
    handler: Any
    def __init__(self, handler: Callable[..., Any]) -> None: ...
    async def handle_async_request(self, request: Request) -> Response: ...

class PageIterator:
    """Lazy iterator that fetches one page per next() call. Created by Client.paginate()."""
    def __iter__(self) -> Iterator[Response]: ...
    def __next__(self) -> Response: ...

# ---------------------------------------------------------------------------
# Top-level API functions
# ---------------------------------------------------------------------------

def request(
    method: str,
    url: str | URL,
    *,
    params: Any | None = ...,
    content: bytes | str | Any | None = ...,
    data: Mapping[str, Any] | None = ...,
    files: Any | None = ...,
    json: Any | None = ...,
    headers: Mapping[str, str] | None = ...,
    cookies: Mapping[str, str] | Cookies | None = ...,
    auth: tuple[str, str] | Auth | None = ...,
    proxy: str | Proxy | None = ...,
    follow_redirects: bool = ...,
    verify: bool | str | Any | None = ...,
    timeout: float | Timeout | None = ...,
) -> Response: ...
def get(url: str | URL, **kwargs: Any) -> Response: ...
def post(url: str | URL, **kwargs: Any) -> Response: ...
def put(url: str | URL, **kwargs: Any) -> Response: ...
def patch(url: str | URL, **kwargs: Any) -> Response: ...
def delete(url: str | URL, **kwargs: Any) -> Response: ...
def head(url: str | URL, **kwargs: Any) -> Response: ...
def options(url: str | URL, **kwargs: Any) -> Response: ...
def stream(
    method: str,
    url: str | URL,
    **kwargs: Any,
) -> Response: ...
def fetch_all(
    requests: list[dict[str, Any]],
    *,
    max_concurrency: int = ...,
    return_exceptions: bool = ...,
    headers: Mapping[str, str] | None = ...,
    timeout: float | Timeout | None = ...,
    verify: bool | str | None = ...,
) -> list[Response | Exception]: ...

# ---------------------------------------------------------------------------
# Stream types
# ---------------------------------------------------------------------------

class AsyncByteStream:
    def __aiter__(self) -> AsyncIterator[bytes]: ...
    async def __anext__(self) -> bytes: ...

class SyncByteStream(AsyncByteStream):
    def __iter__(self) -> Iterator[bytes]: ...
    def __next__(self) -> bytes: ...

class ByteStream(SyncByteStream):
    def __init__(self, data: bytes = ...) -> None: ...

# ---------------------------------------------------------------------------
# Decoder types
# ---------------------------------------------------------------------------

class PyDecoder:
    def __init__(self, encoding: str = ...) -> None: ...
    def decode(self, data: bytes) -> bytes: ...
    def flush(self) -> bytes: ...

class ByteChunker:
    def __init__(self, chunk_size: int = ...) -> None: ...
    def decode(self, data: bytes) -> list[bytes]: ...
    def flush(self) -> list[bytes]: ...

class LineDecoder:
    def __init__(self) -> None: ...
    def decode(self, data: str) -> list[str]: ...
    def flush(self) -> list[str]: ...

# ---------------------------------------------------------------------------
# Multipart types
# ---------------------------------------------------------------------------

class DataField:
    def __init__(self, name: str, value: str) -> None: ...

class FileField:
    def __init__(self, name: str, value: Any) -> None: ...

class MultipartStream:
    def __init__(
        self,
        data: Any | None = ...,
        files: Any | None = ...,
        boundary: str | None = ...,
    ) -> None: ...
    def content_type(self) -> str: ...
    def get_content(self) -> bytes: ...
    def get_content_length(self) -> int: ...

# ---------------------------------------------------------------------------
# Utility functions
# ---------------------------------------------------------------------------

def primitive_value_to_str(value: Any) -> str: ...
def to_bytes(data: str | bytes) -> bytes: ...
def to_str(data: str | bytes) -> str: ...
def to_bytes_or_str(data: Any) -> str | bytes: ...
def unquote(data: str) -> str: ...
def peek_filelike_length(file: Any) -> int | None: ...
def is_ipv4_hostname(hostname: str) -> bool: ...
def is_ipv6_hostname(hostname: str) -> bool: ...
def get_environment_proxies() -> dict[str, str]: ...
def encode_request(
    content: Any | None = ...,
    data: Any | None = ...,
    files: Any | None = ...,
    json: Any | None = ...,
) -> tuple[Any, Any]: ...

# ---------------------------------------------------------------------------
# Status codes (IntEnum)
# ---------------------------------------------------------------------------

class codes:
    """HTTP status codes as IntEnum-like values."""

    # 1xx Informational
    CONTINUE: int
    SWITCHING_PROTOCOLS: int
    PROCESSING: int
    EARLY_HINTS: int

    # 2xx Success
    OK: int
    CREATED: int
    ACCEPTED: int
    NON_AUTHORITATIVE_INFORMATION: int
    NO_CONTENT: int
    RESET_CONTENT: int
    PARTIAL_CONTENT: int
    MULTI_STATUS: int
    ALREADY_REPORTED: int
    IM_USED: int

    # 3xx Redirection
    MULTIPLE_CHOICES: int
    MOVED_PERMANENTLY: int
    FOUND: int
    SEE_OTHER: int
    NOT_MODIFIED: int
    USE_PROXY: int
    TEMPORARY_REDIRECT: int
    PERMANENT_REDIRECT: int

    # 4xx Client Error
    BAD_REQUEST: int
    UNAUTHORIZED: int
    PAYMENT_REQUIRED: int
    FORBIDDEN: int
    NOT_FOUND: int
    METHOD_NOT_ALLOWED: int
    NOT_ACCEPTABLE: int
    PROXY_AUTHENTICATION_REQUIRED: int
    REQUEST_TIMEOUT: int
    CONFLICT: int
    GONE: int
    LENGTH_REQUIRED: int
    PRECONDITION_FAILED: int
    REQUEST_ENTITY_TOO_LARGE: int
    REQUEST_URI_TOO_LONG: int
    UNSUPPORTED_MEDIA_TYPE: int
    REQUESTED_RANGE_NOT_SATISFIABLE: int
    EXPECTATION_FAILED: int
    IM_A_TEAPOT: int
    MISDIRECTED_REQUEST: int
    UNPROCESSABLE_ENTITY: int
    LOCKED: int
    FAILED_DEPENDENCY: int
    TOO_EARLY: int
    UPGRADE_REQUIRED: int
    PRECONDITION_REQUIRED: int
    TOO_MANY_REQUESTS: int
    REQUEST_HEADER_FIELDS_TOO_LARGE: int
    UNAVAILABLE_FOR_LEGAL_REASONS: int

    # 5xx Server Error
    INTERNAL_SERVER_ERROR: int
    NOT_IMPLEMENTED: int
    BAD_GATEWAY: int
    SERVICE_UNAVAILABLE: int
    GATEWAY_TIMEOUT: int
    HTTP_VERSION_NOT_SUPPORTED: int
    VARIANT_ALSO_NEGOTIATES: int
    INSUFFICIENT_STORAGE: int
    LOOP_DETECTED: int
    NOT_EXTENDED: int
    NETWORK_AUTHENTICATION_REQUIRED: int

    def is_informational(self) -> bool: ...
    def is_success(self) -> bool: ...
    def is_redirect(self) -> bool: ...
    def is_client_error(self) -> bool: ...
    def is_server_error(self) -> bool: ...
    def is_error(self) -> bool: ...

# ---------------------------------------------------------------------------
# Exceptions
# ---------------------------------------------------------------------------

class HTTPError(Exception):
    _request: Request | None
    request: Request
    def __init__(self, message: str, **kwargs: Any) -> None: ...

class RequestError(HTTPError):
    def __init__(self, message: str, *, request: Request | None = ...) -> None: ...

class TransportError(RequestError): ...
class TimeoutException(TransportError): ...
class ConnectTimeout(TimeoutException): ...
class ReadTimeout(TimeoutException): ...
class WriteTimeout(TimeoutException): ...
class PoolTimeout(TimeoutException): ...
class NetworkError(TransportError): ...
class ReadError(NetworkError): ...
class WriteError(NetworkError): ...
class ConnectError(NetworkError): ...
class CloseError(NetworkError): ...
class ProxyError(TransportError): ...
class UnsupportedProtocol(TransportError): ...
class ProtocolError(TransportError): ...
class LocalProtocolError(ProtocolError): ...
class RemoteProtocolError(ProtocolError): ...
class DecodingError(RequestError): ...
class TooManyRedirects(RequestError): ...

class HTTPStatusError(HTTPError):
    response: Response
    request: Request

class StreamError(RequestError): ...
class StreamConsumed(StreamError): ...
class StreamClosed(StreamError): ...
class ResponseNotRead(StreamError): ...
class RequestNotRead(StreamError): ...
class InvalidURL(Exception): ...
class CookieConflict(Exception): ...
