from __future__ import annotations

from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager, contextmanager
from typing import Any, AsyncIterator, Iterator

import httpxr

from ._decoders import SSEDecoder, SSELineDecoder
from ._exceptions import SSEError
from ._models import ServerSentEvent


class EventSource:
    def __init__(self, response: httpxr.Response) -> None:
        self._response = response

    def _check_content_type(self) -> None:
        content_type = (self._response.headers.get("content-type") or "").partition(";")[0]
        if "text/event-stream" not in content_type:
            raise SSEError(
                "Expected response header Content-Type to contain 'text/event-stream', "
                f"got {content_type!r}"
            )

    @property
    def response(self) -> httpxr.Response:
        return self._response

    def iter_sse(self) -> Iterator[ServerSentEvent]:
        self._check_content_type()
        decoder = SSEDecoder()
        for line in _iter_sse_lines(self._response):
            line = line.rstrip("\n")
            sse = decoder.decode(line)
            if sse is not None:
                yield sse

    async def aiter_sse(self) -> AsyncGenerator[ServerSentEvent, None]:
        self._check_content_type()
        decoder = SSEDecoder()
        lines = _aiter_sse_lines(self._response)
        try:
            async for line in lines:
                line = line.rstrip("\n")
                sse = decoder.decode(line)
                if sse is not None:
                    yield sse
        finally:
            await lines.aclose()


@contextmanager
def connect_sse(
    client: httpxr.Client,
    method: str,
    url: str,
    **kwargs: Any,
) -> Iterator[EventSource]:
    headers = kwargs.pop("headers", {})
    headers["Accept"] = "text/event-stream"
    headers["Cache-Control"] = "no-store"

    with client.stream(method, url, headers=headers, **kwargs) as response:
        yield EventSource(response)


@asynccontextmanager
async def aconnect_sse(
    client: httpxr.AsyncClient,
    method: str,
    url: str,
    **kwargs: Any,
) -> AsyncIterator[EventSource]:
    headers = kwargs.pop("headers", {})
    headers["Accept"] = "text/event-stream"
    headers["Cache-Control"] = "no-store"

    async with client.stream(method, url, headers=headers, **kwargs) as response:  # type: ignore[attr-defined]
        yield EventSource(response)


async def _aiter_sse_lines(response: httpxr.Response) -> AsyncGenerator[str, None]:
    decoder = SSELineDecoder()
    async for text in response.aiter_text():  # type: ignore[union-attr]
        for line in decoder.decode(text):
            yield line
    for line in decoder.flush():
        yield line


def _iter_sse_lines(response: httpxr.Response) -> Iterator[str]:
    decoder = SSELineDecoder()
    for text in response.iter_text():
        for line in decoder.decode(text):
            yield line
    for line in decoder.flush():
        yield line
