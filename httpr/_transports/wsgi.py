from __future__ import annotations

import io
import itertools
import sys
import typing

from httpr._httpr import BaseTransport, Request, Response

if typing.TYPE_CHECKING:
    from _typeshed import OptExcInfo  # pragma: no cover
    from _typeshed.wsgi import WSGIApplication  # pragma: no cover

_T = typing.TypeVar("_T")


__all__ = ["WSGITransport"]


def _skip_leading_empty_chunks(body: typing.Iterable[_T]) -> typing.Iterable[_T]:
    body = iter(body)
    for chunk in body:
        if chunk:
            return itertools.chain([chunk], body)
    return typing.cast(typing.Iterable[_T], [])



class WSGITransport(BaseTransport):

    def __init__(
        self,
        app: WSGIApplication,
        raise_app_exceptions: bool = True,
        script_name: str = "",
        remote_addr: str = "127.0.0.1",
        wsgi_errors: typing.TextIO | None = None,
    ) -> None:
        self.app = app
        self.raise_app_exceptions = raise_app_exceptions
        self.script_name = script_name
        self.remote_addr = remote_addr
        self.wsgi_errors = wsgi_errors

    def handle_request(self, request: Request) -> Response:
        request.read()
        wsgi_input = io.BytesIO(request.content)

        port = request.url.port or {"http": 80, "https": 443}[request.url.scheme]
        environ = {
            "wsgi.version": (1, 0),
            "wsgi.url_scheme": request.url.scheme,
            "wsgi.input": wsgi_input,
            "wsgi.errors": self.wsgi_errors or sys.stderr,
            "wsgi.multithread": True,
            "wsgi.multiprocess": False,
            "wsgi.run_once": False,
            "REQUEST_METHOD": request.method,
            "SCRIPT_NAME": self.script_name,
            "PATH_INFO": request.url.path,
            "QUERY_STRING": request.url.query.decode("ascii"),
            "SERVER_NAME": request.url.host,
            "SERVER_PORT": str(port),
            "SERVER_PROTOCOL": "HTTP/1.1",
            "REMOTE_ADDR": self.remote_addr,
        }
        for header_key, header_value in request.headers.raw:
            key = header_key.decode("ascii").upper().replace("-", "_")
            if key not in ("CONTENT_TYPE", "CONTENT_LENGTH"):
                key = "HTTP_" + key
            environ[key] = header_value.decode("ascii")

        seen_status: str | None = None
        seen_response_headers: list[tuple[str, str]] | None = None
        seen_exc_info: OptExcInfo | None = None

        def start_response(
            status: str,
            response_headers: list[tuple[str, str]],
            exc_info: OptExcInfo | None = None,
        ) -> typing.Callable[[bytes], typing.Any]:
            nonlocal seen_status, seen_response_headers, seen_exc_info
            seen_status = status
            seen_response_headers = response_headers
            seen_exc_info = exc_info
            return lambda _: None

        result: typing.Iterable[typing.Any] = self.app(environ, start_response)

        body_parts: list[bytes] = []
        close_func = getattr(result, "close", None)
        try:
            for chunk in _skip_leading_empty_chunks(result):
                body_parts.append(chunk)
        finally:
            if close_func is not None:
                close_func()

        assert seen_status is not None
        assert seen_response_headers is not None

        if seen_exc_info and seen_exc_info[0] and self.raise_app_exceptions:
            exc = seen_exc_info[1]
            if exc is not None:
                raise exc

        status_code = int(seen_status.split()[0])
        headers = [
            (key.encode("ascii"), value.encode("ascii"))
            for key, value in seen_response_headers
        ]

        return Response(
            status_code, headers=headers, content=b"".join(body_parts)
        )
