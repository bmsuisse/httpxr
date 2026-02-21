from __future__ import annotations

import json
from typing import Any, Optional


class ServerSentEvent:
    __slots__ = ("event", "data", "id", "retry")

    def __init__(
        self,
        event: Optional[str] = None,
        data: Optional[str] = None,
        id: Optional[str] = None,
        retry: Optional[int] = None,
    ) -> None:
        self.event = event or "message"
        self.data = data or ""
        self.id = id or ""
        self.retry = retry

    def json(self) -> Any:
        return json.loads(self.data)

    def __repr__(self) -> str:
        pieces = [f"event={self.event!r}"]
        if self.data:
            pieces.append(f"data={self.data!r}")
        if self.id:
            pieces.append(f"id={self.id!r}")
        if self.retry is not None:
            pieces.append(f"retry={self.retry!r}")
        return f"ServerSentEvent({', '.join(pieces)})"
