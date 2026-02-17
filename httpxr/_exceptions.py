from __future__ import annotations

import typing

from ._httpxr import HTTPError as _HTTPError
from ._httpxr import RequestError as _RequestError

if typing.TYPE_CHECKING:
    from ._httpxr import Request

_original_httperror_init = _HTTPError.__init__
_original_requesterror_init = _RequestError.__init__


def _httperror_init(self: _HTTPError, message: str, **kwargs: typing.Any) -> None:
    _original_httperror_init(self, message)
    self._request = kwargs.get("request", None)  # type: ignore[attr-defined]


def _httperror_request_get(self: _HTTPError) -> Request:
    req = getattr(self, "_request", None)
    if req is None:
        raise RuntimeError("The .request property has not been set.")
    return req


def _httperror_request_set(self: _HTTPError, request: Request) -> None:
    self._request = request


_HTTPError.__init__ = _httperror_init  # type: ignore[assignment]
_HTTPError.request = property(_httperror_request_get, _httperror_request_set)  # type: ignore[attr-defined]


def _requesterror_init(
    self: _RequestError, message: str, *, request: typing.Any = None
) -> None:
    _original_httperror_init(self, message)
    self._request = request


_RequestError.__init__ = _requesterror_init  # type: ignore[assignment]
