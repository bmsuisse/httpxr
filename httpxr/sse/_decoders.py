from __future__ import annotations

from typing import Optional

from ._models import ServerSentEvent


def _splitlines_sse(text: str) -> list[str]:
    """Split text on \\r\\n, \\r, or \\n only.

    Unlike Python's str.splitlines(), this does NOT treat Unicode line
    separators (U+2028, U+2029, U+0085 etc.) as newlines, per the SSE spec.
    """
    if not text:
        return []

    if "\r" not in text:
        lines = text.split("\n")
    else:
        normalized = text.replace("\r\n", "\n").replace("\r", "\n")
        lines = normalized.split("\n")

    if text[-1] in "\r\n":
        lines.pop()

    return lines


class SSELineDecoder:
    """Handles incrementally reading lines from text.

    As per SSE spec, only \\r\\n, \\r, and \\n are treated as newlines.
    """

    def __init__(self) -> None:
        self.buffer: list[str] = []
        self.trailing_cr: bool = False

    def decode(self, text: str) -> list[str]:
        # We always push a trailing `\r` into the next decode iteration.
        if self.trailing_cr:
            text = "\r" + text
            self.trailing_cr = False
        if text.endswith("\r"):
            self.trailing_cr = True
            text = text[:-1]

        if not text:
            return []  # pragma: no cover

        trailing_newline = text[-1] in "\n\r"
        lines = _splitlines_sse(text)

        if len(lines) == 1 and not trailing_newline:
            # No new lines, buffer the input and continue.
            self.buffer.append(lines[0])
            return []

        if self.buffer:
            # Include any existing buffer in the first portion of the
            # splitlines result.
            lines = ["".join(self.buffer) + lines[0]] + lines[1:]
            self.buffer = []

        if not trailing_newline:
            # If the last segment of splitlines is not newline terminated,
            # then drop it from our output and start a new buffer.
            self.buffer = [lines.pop()]

        return lines

    def flush(self) -> list[str]:
        if not self.buffer and not self.trailing_cr:
            return []

        lines = ["".join(self.buffer)]
        self.buffer = []
        self.trailing_cr = False
        return lines


class SSEDecoder:
    """Parses SSE fields from lines per the WHATWG spec.

    See: https://html.spec.whatwg.org/multipage/server-sent-events.html#event-stream-interpretation
    """

    def __init__(self) -> None:
        self._event = ""
        self._data: list[str] = []
        self._last_event_id = ""
        self._retry: Optional[int] = None

    def decode(self, line: str) -> Optional[ServerSentEvent]:
        if not line:
            if (
                not self._event
                and not self._data
                and not self._last_event_id
                and self._retry is None
            ):
                return None

            sse = ServerSentEvent(
                event=self._event,
                data="\n".join(self._data),
                id=self._last_event_id,
                retry=self._retry,
            )

            # NOTE: as per the SSE spec, do not reset last_event_id.
            self._event = ""
            self._data = []
            self._retry = None

            return sse

        if line.startswith(":"):
            return None

        fieldname, _, value = line.partition(":")

        if value.startswith(" "):
            value = value[1:]

        if fieldname == "event":
            self._event = value
        elif fieldname == "data":
            self._data.append(value)
        elif fieldname == "id":
            if "\0" in value:
                pass
            else:
                self._last_event_id = value
        elif fieldname == "retry":
            try:
                self._retry = int(value)
            except (TypeError, ValueError):
                pass
        else:
            pass  # Field is ignored.

        return None
