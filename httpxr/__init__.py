# ruff: noqa: I001
from . import _exceptions  # noqa: F401
from ._httpxr import *  # noqa: F403
from ._transports import ASGITransport, WSGITransport  # noqa: F401
from . import extensions, sse  # noqa: F401
from .extensions import (  # noqa: F401
    OAuth2Auth,
    aiter_json_bytes,
    apaginate_to_records,
    gather_raw_bytes,
    iter_json_bytes,
    paginate_to_records,
)

try:
    from .cli import main
except ImportError:

    def main() -> None:  # type: ignore[misc]
        import sys

        print(
            'The "httpxr" command requires the CLI extra. '
            'Install it with: pip install "httpxr[cli]"',
            file=sys.stderr,
        )
        sys.exit(1)


_EXCLUDED_FROM_ALL = {"compat", "cli", "main"}
_ALWAYS_IN_ALL = {"sse", "WSGITransport", "extensions"}

_members = [
    member
    for member in vars().keys()
    if (not member.startswith("_") or member in {"__description__", "__title__", "__version__"})
    and member not in _EXCLUDED_FROM_ALL
]
_members = list(set(_members) | _ALWAYS_IN_ALL)

__all__ = sorted(_members, key=str.casefold)  # pyright: ignore[reportUnsupportedDunderAll]
