"""Tests for httpxr.sse — ported from httpx-sse test suite.

See: https://github.com/florimondmanca/httpx-sse/tree/master/tests
"""

from __future__ import annotations

import json

import pytest

import httpxr
from httpxr.sse import EventSource, SSEError, ServerSentEvent, connect_sse
from httpxr.sse._api import _iter_sse_lines
from httpxr.sse._decoders import SSELineDecoder, _splitlines_sse


# ---------------------------------------------------------------------------
# test_models.py
# ---------------------------------------------------------------------------


class TestServerSentEvent:
    def test_sse_default(self) -> None:
        sse = ServerSentEvent()

        assert sse.event == "message"
        assert sse.data == ""
        assert sse.id == ""
        assert sse.retry is None

    def test_sse_json(self) -> None:
        sse = ServerSentEvent()

        with pytest.raises(json.JSONDecodeError):
            sse.json()

        sse = ServerSentEvent(data='{"key": "value"}')
        assert sse.json() == {"key": "value"}

        sse = ServerSentEvent(data='["item1", "item2"]')
        assert sse.json() == ["item1", "item2"]

    def test_sse_repr(self) -> None:
        sse = ServerSentEvent()
        assert repr(sse) == "ServerSentEvent(event='message')"

        sse = ServerSentEvent(data="data", retry=3, id="id", event="event")
        expected = "ServerSentEvent(event='event', data='data', id='id', retry=3)"
        assert repr(sse) == expected


# ---------------------------------------------------------------------------
# test_exceptions.py
# ---------------------------------------------------------------------------


class TestSSEError:
    def test_sse_error(self) -> None:
        assert issubclass(SSEError, httpxr.TransportError)


# ---------------------------------------------------------------------------
# test_decoders.py — _splitlines_sse
# ---------------------------------------------------------------------------


class TestSplitlinesSSE:
    def test_crlf_splitting(self) -> None:
        text = "line1\r\nline2\r\nline3"
        lines = _splitlines_sse(text)
        assert lines == ["line1", "line2", "line3"]

    def test_cr_splitting(self) -> None:
        text = "line1\rline2\rline3"
        lines = _splitlines_sse(text)
        assert lines == ["line1", "line2", "line3"]

    def test_lf_splitting(self) -> None:
        text = "line1\nline2\nline3"
        lines = _splitlines_sse(text)
        assert lines == ["line1", "line2", "line3"]

    def test_mixed_line_endings(self) -> None:
        text = "line1\r\nline2\nline3\rline4"
        lines = _splitlines_sse(text)
        assert lines == ["line1", "line2", "line3", "line4"]

    def test_empty_lines(self) -> None:
        text = "line1\n\nline3"
        lines = _splitlines_sse(text)
        assert lines == ["line1", "", "line3"]

    def test_unicode_line_separator_not_split(self) -> None:
        # U+2028 (LINE SEPARATOR) should NOT be treated as a newline
        text = "line1\u2028line2"
        lines = _splitlines_sse(text)
        assert lines == ["line1\u2028line2"]

    def test_unicode_paragraph_separator_not_split(self) -> None:
        # U+2029 (PARAGRAPH SEPARATOR) should NOT be treated as a newline
        text = "line1\u2029line2"
        lines = _splitlines_sse(text)
        assert lines == ["line1\u2029line2"]

    def test_unicode_next_line_not_split(self) -> None:
        # U+0085 (NEXT LINE) should NOT be treated as a newline
        text = "line1\u0085line2"
        lines = _splitlines_sse(text)
        assert lines == ["line1\u0085line2"]

    def test_empty_string(self) -> None:
        lines = _splitlines_sse("")
        assert lines == []

    def test_only_newlines(self) -> None:
        lines = _splitlines_sse("\n\r\n\r")
        assert lines == ["", "", ""]

    def test_trailing_newlines(self) -> None:
        lines = _splitlines_sse("line1\n")
        assert lines == ["line1"]


# ---------------------------------------------------------------------------
# test_decoders.py — SSELineDecoder
# ---------------------------------------------------------------------------


class TestSSELineDecoder:
    def _decode_chunks(self, chunks: list[str]) -> list[str]:
        """Helper to decode a list of chunks and return all lines."""
        decoder = SSELineDecoder()
        lines = []
        for chunk in chunks:
            lines.extend(decoder.decode(chunk))
        lines.extend(decoder.flush())
        return lines

    def test_basic_lines(self) -> None:
        chunks = ["line1\nline2\n"]
        assert self._decode_chunks(chunks) == ["line1", "line2"]

    def test_incremental_decoding(self) -> None:
        chunks = ["partial", " line\n", "another\n"]
        assert self._decode_chunks(chunks) == ["partial line", "another"]

    def test_trailing_cr_with_immediate_n(self) -> None:
        # \r at end of first chunk, \n at start of second chunk
        chunks = ["line1\r", "\nline2", "\n"]
        assert self._decode_chunks(chunks) == ["line1", "line2"]

    def test_crlf_across_chunks(self) -> None:
        # \r\n split across two chunks
        chunks = ["line1\r", "\nline2\n"]
        assert self._decode_chunks(chunks) == ["line1", "line2"]

    def test_buffer_without_newline(self) -> None:
        # Text without newline should be buffered then flushed
        chunks = ["buffered"]
        assert self._decode_chunks(chunks) == ["buffered"]

    def test_buffer_with_newline(self) -> None:
        # Text without newline followed by newline
        chunks = ["buffered", "\n"]
        assert self._decode_chunks(chunks) == ["buffered"]

    def test_no_flush_needed(self) -> None:
        # All lines terminated, flush returns nothing
        chunks = ["line1\n", "line2\n"]
        assert self._decode_chunks(chunks) == ["line1", "line2"]

    def test_flush_with_trailing_cr(self) -> None:
        # Text ending with \r should not leave buffered content after flush
        chunks = ["text\r"]
        assert self._decode_chunks(chunks) == ["text"]

    def test_empty_chunks(self) -> None:
        # Empty chunks should be handled gracefully
        chunks = ["", "line1\n", "", "line2\n", ""]
        assert self._decode_chunks(chunks) == ["line1", "line2"]

    def test_multiple_empty_lines(self) -> None:
        chunks = ["\n\n\n"]
        assert self._decode_chunks(chunks) == ["", "", ""]

    def test_mixed_line_endings_incremental(self) -> None:
        chunks = ["line1\r\n", "line2\r", "line3\n"]
        assert self._decode_chunks(chunks) == ["line1", "line2", "line3"]

    def test_partial_line_then_complete(self) -> None:
        chunks = ["par", "tial", " line\ncomp", "lete\n"]
        assert self._decode_chunks(chunks) == ["partial line", "complete"]

    def test_unicode_line_separators_preserved(self) -> None:
        # Unicode line separators should be preserved in the output
        chunks = ["data\u2028field\nline2\u2029end\n"]
        assert self._decode_chunks(chunks) == ["data\u2028field", "line2\u2029end"]

    def test_alternating_cr_lf(self) -> None:
        chunks = ["\r\n\r\n"]
        assert self._decode_chunks(chunks) == ["", ""]

    def test_flush_after_partial(self) -> None:
        chunks = ["line1\npartial"]
        assert self._decode_chunks(chunks) == ["line1", "partial"]

    def test_consecutive_cr_handling(self) -> None:
        chunks = ["line1\r\rline2\n"]
        assert self._decode_chunks(chunks) == ["line1", "", "line2"]

    def test_text_after_trailing_newline(self) -> None:
        chunks = ["line1\n", "line2", "\n"]
        assert self._decode_chunks(chunks) == ["line1", "line2"]

    def test_only_cr(self) -> None:
        chunks = ["\r", "\r"]
        assert self._decode_chunks(chunks) == ["", ""]

    def test_only_lf(self) -> None:
        chunks = ["\n"]
        assert self._decode_chunks(chunks) == [""]

    def test_empty_input(self) -> None:
        assert self._decode_chunks([]) == []

    def test_single_char_chunks(self) -> None:
        # Test with single character chunks to ensure buffering works
        chunks = ["h", "e", "l", "l", "o", "\n", "w", "o", "r", "l", "d"]
        assert self._decode_chunks(chunks) == ["hello", "world"]

    def test_cr_lf_as_separate_chunks(self) -> None:
        # Each character as separate chunk
        chunks = ["l", "i", "n", "e", "1", "\r", "\n", "l", "i", "n", "e", "2"]
        assert self._decode_chunks(chunks) == ["line1", "line2"]

    def test_mixed_endings_with_content(self) -> None:
        chunks = ["a\rb\nc\r\nd"]
        assert self._decode_chunks(chunks) == ["a", "b", "c", "d"]

    def test_trailing_cr_no_followup(self) -> None:
        # Trailing \r with no following text
        chunks = ["line\r"]
        assert self._decode_chunks(chunks) == ["line"]

    def test_complex_mixed_scenario(self) -> None:
        # Complex scenario with various line endings and partial chunks
        chunks = [
            "first",
            " line\r",
            "\nsecond",
            " line\r\n",
            "third\rfo",
            "urth\n",
            "fifth",
        ]
        assert self._decode_chunks(chunks) == [
            "first line",
            "second line",
            "third",
            "fourth",
            "fifth",
        ]


# ---------------------------------------------------------------------------
# test_event_source.py — WHATWG spec examples
# See: https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation
# ---------------------------------------------------------------------------


def _make_sse_response(text: str) -> httpxr.Response:
    """Create an httpxr Response with SSE content-type and text body."""
    return httpxr.Response(
        200,
        headers={"content-type": "text/event-stream"},
        text=text,
    )


class TestEventSource:
    def test_iter_sse_whatwg_example1(self) -> None:
        response = _make_sse_response("data: YH00\ndata: +2\ndata: 10\n\n")

        events = list(EventSource(response).iter_sse())
        assert len(events) == 1

        assert events[0].event == "message"
        assert events[0].data == "YH00\n+2\n10"
        assert events[0].id == ""
        assert events[0].retry is None

    def test_iter_sse_whatwg_example2(self) -> None:
        response = _make_sse_response(
            ": test stream\n"
            "\n"
            "data: first event\n"
            "id: 1\n"
            "\n"
            "data: second event\n"
            "id\n"
            "\n"
            "data:  third event\n"
            "\n"
        )

        events = list(EventSource(response).iter_sse())
        assert len(events) == 3

        assert events[0].event == "message"
        assert events[0].data == "first event"
        assert events[0].id == "1"
        assert events[0].retry is None

        assert events[1].event == "message"
        assert events[1].data == "second event"
        assert events[1].id == ""
        assert events[1].retry is None

        assert events[2].event == "message"
        assert events[2].data == " third event"
        assert events[2].id == ""
        assert events[2].retry is None

    def test_iter_sse_whatwg_example3(self) -> None:
        response = _make_sse_response("data\n\ndata\ndata\n\ndata:\n")

        events = list(EventSource(response).iter_sse())
        assert len(events) == 2

        assert events[0].event == "message"
        assert events[0].data == ""
        assert events[0].id == ""
        assert events[0].retry is None

        assert events[1].event == "message"
        assert events[1].data == "\n"
        assert events[1].id == ""
        assert events[1].retry is None

    def test_iter_sse_whatwg_example4(self) -> None:
        response = _make_sse_response("data:test\n\ndata: test\n\n")

        events = list(EventSource(response).iter_sse())
        assert len(events) == 2

        assert events[0].event == "message"
        assert events[0].data == "test"
        assert events[0].id == ""
        assert events[0].retry is None

        assert events[1].event == "message"
        assert events[1].data == "test"
        assert events[1].id == ""
        assert events[1].retry is None

    def test_iter_sse_event(self) -> None:
        response = _make_sse_response("event: logline\ndata: New user connected\n\n")

        events = list(EventSource(response).iter_sse())
        assert len(events) == 1

        assert events[0].event == "logline"
        assert events[0].data == "New user connected"
        assert events[0].id == ""
        assert events[0].retry is None

    def test_iter_sse_id_null(self) -> None:
        response = _make_sse_response("data: test\nid: 123\0\n\n")

        events = list(EventSource(response).iter_sse())
        assert len(events) == 1

        assert events[0].event == "message"
        assert events[0].data == "test"
        assert events[0].id == ""
        assert events[0].retry is None

    def test_iter_sse_id_retry(self) -> None:
        response = _make_sse_response("retry: 10000\n\n")

        events = list(EventSource(response).iter_sse())
        assert len(events) == 1

        assert events[0].event == "message"
        assert events[0].data == ""
        assert events[0].id == ""
        assert events[0].retry == 10000

    def test_iter_sse_id_retry_invalid(self) -> None:
        response = _make_sse_response("retry: 1667a\n\n")

        events = list(EventSource(response).iter_sse())
        assert len(events) == 0

    def test_iter_sse_unknown_field(self) -> None:
        response = _make_sse_response("something: ignore\n\n")

        events = list(EventSource(response).iter_sse())
        assert len(events) == 0

    def test_unicode_line_separator_not_parsed_as_newline(self) -> None:
        """Test that Unicode line separator (U+2028) is not parsed as a newline.

        The SSE spec only allows CR, LF, or CRLF as line separators.
        Other Unicode newline characters should be preserved in the data.

        See: https://github.com/florimondmanca/httpx-sse/issues/34
        """
        # U+2028 is LINE SEPARATOR
        text = 'event: message\ndata: {"text":"Hello\u2028World"}\n\n'
        response = _make_sse_response(text)

        events = list(EventSource(response).iter_sse())
        assert len(events) == 1

        assert events[0].event == "message"
        assert events[0].data == '{"text":"Hello\u2028World"}'
        assert events[0].id == ""
        assert events[0].retry is None


# ---------------------------------------------------------------------------
# test_api.py — connect_sse, content-type check, line iterators
# ---------------------------------------------------------------------------


class TestConnectSSE:
    @pytest.mark.parametrize(
        "content_type",
        [
            pytest.param("text/event-stream", id="exact"),
            pytest.param(
                "application/json, text/event-stream; charset=utf-8", id="contains"
            ),
        ],
    )
    def test_connect_sse(self, content_type: str) -> None:
        def handler(request: httpxr.Request) -> httpxr.Response:
            if request.url.path == "/":
                return httpxr.Response(200, text="Hello, world!")
            else:
                assert request.url.path == "/sse"
                text = "data: test\n\n"
                return httpxr.Response(
                    200, headers={"content-type": content_type}, text=text
                )

        with httpxr.Client(transport=httpxr.MockTransport(handler)) as client:
            response = client.request("GET", "http://testserver")
            assert response.status_code == 200
            assert response.headers["content-type"] == "text/plain; charset=utf-8"

            with connect_sse(client, "GET", "http://testserver/sse") as event_source:
                assert (
                    event_source.response.request.headers["cache-control"] == "no-store"
                )

    def test_connect_sse_non_event_stream_received(self) -> None:
        def handler(request: httpxr.Request) -> httpxr.Response:
            assert request.url.path == "/"
            return httpxr.Response(200, text="Hello, world!")

        with httpxr.Client(transport=httpxr.MockTransport(handler)) as client:
            with pytest.raises(SSEError, match="text/event-stream"):
                with connect_sse(client, "GET", "http://testserver") as event_source:
                    for _ in event_source.iter_sse():
                        pass  # pragma: no cover

    def test_iter_sse_lines_basic(self) -> None:
        text = "line1\nline2\n"
        response = httpxr.Response(200, text=text)
        lines = list(_iter_sse_lines(response))
        assert lines == ["line1", "line2"]

    def test_iter_sse_lines_with_flush(self) -> None:
        text = "line1\npartial"
        response = httpxr.Response(200, text=text)
        lines = list(_iter_sse_lines(response))
        assert lines == ["line1", "partial"]  # flush gets the partial line


# ---------------------------------------------------------------------------
# test_api.py — async variants
# ---------------------------------------------------------------------------


class TestAsyncConnectSSE:
    @pytest.mark.anyio
    async def test_aconnect_sse(self) -> None:
        from httpxr.sse import aconnect_sse

        async def handler(request: httpxr.Request) -> httpxr.Response:
            if request.url.path == "/":
                return httpxr.Response(200, text="Hello, world!")
            else:
                assert request.url.path == "/sse"
                text = "data: test\n\n"
                return httpxr.Response(
                    200, headers={"content-type": "text/event-stream"}, text=text
                )

        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(handler)
        ) as client:
            response = await client.request("GET", "http://testserver")
            assert response.status_code == 200
            assert response.headers["content-type"] == "text/plain; charset=utf-8"

            async with aconnect_sse(
                client, "GET", "http://testserver/sse"
            ) as event_source:
                assert (
                    event_source.response.request.headers["cache-control"] == "no-store"
                )

    @pytest.mark.anyio
    async def test_aiter_sse_events(self) -> None:
        """Test that aiter_sse correctly yields events using async client."""
        from httpxr.sse import aconnect_sse

        async def handler(request: httpxr.Request) -> httpxr.Response:
            text = "data: YH00\ndata: +2\ndata: 10\n\n"
            return httpxr.Response(
                200, headers={"content-type": "text/event-stream"}, text=text
            )

        async with httpxr.AsyncClient(
            transport=httpxr.MockTransport(handler)
        ) as client:
            async with aconnect_sse(
                client, "GET", "http://testserver/sse"
            ) as event_source:
                events = [sse async for sse in event_source.aiter_sse()]

        assert len(events) == 1
        assert events[0].event == "message"
        assert events[0].data == "YH00\n+2\n10"
