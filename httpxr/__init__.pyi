"""Type stubs for the httpxr package."""

from ._httpxr import *  # noqa: F403
from ._httpxr import __version__ as __version__
from ._transports import ASGITransport as ASGITransport, WSGITransport as WSGITransport

def main() -> None: ...

__all__: list[str]
