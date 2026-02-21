from __future__ import annotations

from typing import Optional

from ._models import ServerSentEvent


def _splitlines_sse(text: str) -> list[str]:
    """Split on \\r\\n, \\r, or \\n only — not Unicode line separators (SSE spec)."""
    if not text:
        return []
    if "\r" not in text:
        lines = text.split("\n")
    else:
        lines = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    if text[-1] in "\r\n":
        lines.pop()
    return lines


class SSELineDecoder:
    """Incrementally reads lines from text chunks per the SSE spec."""

    def __init__(self) -> None:
        self.buffer: list[str] = []
        self.trailing_cr: bool = False

    def decode(self, text: str) -> list[str]:
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
            self.buffer.append(lines[0])
            return []

        if self.buffer:
            lines = ["".join(self.buffer) + lines[0]] + lines[1:]
            self.buffer = []

        if not trailing_newline:
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
    """Parses SSE fields per the WHATWG spec."""

    def __init__(self) -> None:
        self._event = ""
        self._data: list[str] = []
        self._last_event_id = ""
        self._retry: Optional[int] = None

    def decode(self, line: str) -> Optional[ServerSentEvent]:
        if not line:
            if not self._event and not self._data and not self._last_event_id and self._retry is None:
                return None
            sse = ServerSentEvent(
                event=self._event,
                data="\n".join(self._data),
                id=self._last_event_id,
                retry=self._retry,
            )
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
        elif fieldname == "id" and "\0" not in value:
            self._last_event_id = value
        elif fieldname == "retry":
            try:
                self._retry = int(value)
            except (TypeError, ValueError):
                pass

        return None
