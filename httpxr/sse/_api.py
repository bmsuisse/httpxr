from __future__ import annotations

from collections.abc import AsyncGenerator
from contextlib import asynccontextmanager, contextmanager
from typing import Any, AsyncIterator, Iterator

import httpxr

from ._decoders import SSEDecoder, SSELineDecoder
from ._exceptions import SSEError
from ._models import ServerSentEvent

_SSE_HEADERS = {"Accept": "text/event-stream", "Cache-Control": "no-store"}


class EventSource:
    def __init__(self, response: httpxr.Response) -> None:
        self._response = response

    @property
    def response(self) -> httpxr.Response:
        return self._response

    def _check_content_type(self) -> None:
        ct = (self._response.headers.get("content-type") or "").partition(";")[0]
        if "text/event-stream" not in ct:
            raise SSEError(
                "Expected response header Content-Type to contain 'text/event-stream', "
                f"got {ct!r}"
            )

    def iter_sse(self) -> Iterator[ServerSentEvent]:
        self._check_content_type()
        decoder = SSEDecoder()
        for line in _iter_sse_lines(self._response):
            sse = decoder.decode(line.rstrip("\n"))
            if sse is not None:
                yield sse

    async def aiter_sse(self) -> AsyncGenerator[ServerSentEvent, None]:
        self._check_content_type()
        decoder = SSEDecoder()
        async for line in _aiter_sse_lines(self._response):
            sse = decoder.decode(line.rstrip("\n"))
            if sse is not None:
                yield sse


@contextmanager
def connect_sse(client: httpxr.Client, method: str, url: str, **kwargs: Any) -> Iterator[EventSource]:
    headers = {**_SSE_HEADERS, **kwargs.pop("headers", {})}
    with client.stream(method, url, headers=headers, **kwargs) as response:
        yield EventSource(response)


@asynccontextmanager
async def aconnect_sse(client: httpxr.AsyncClient, method: str, url: str, **kwargs: Any) -> AsyncIterator[EventSource]:
    headers = {**_SSE_HEADERS, **kwargs.pop("headers", {})}
    async with client.stream(method, url, headers=headers, **kwargs) as response:  # type: ignore[attr-defined]
        yield EventSource(response)


def _iter_sse_lines(response: httpxr.Response) -> Iterator[str]:
    decoder = SSELineDecoder()
    for text in response.iter_text():
        yield from decoder.decode(text)
    yield from decoder.flush()


async def _aiter_sse_lines(response: httpxr.Response) -> AsyncGenerator[str, None]:
    decoder = SSELineDecoder()
    async for text in response.aiter_text():  # type: ignore[union-attr]
        for line in decoder.decode(text):
            yield line
    for line in decoder.flush():
        yield line
