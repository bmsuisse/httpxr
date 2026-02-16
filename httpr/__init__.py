from . import _exceptions  # noqa: F401
from ._httpr import *
from ._transports import ASGITransport, WSGITransport

try:
    from .cli import main
except ImportError:

    def main() -> None:  # type: ignore[misc]
        import sys

        print(
            'The "httpr" command requires the CLI extra. '
            'Install it with: pip install "httpr[cli]"',
            file=sys.stderr,
        )
        sys.exit(1)


__all__ = sorted(
    (
        member
        for member in list(vars().keys())
        if not member.startswith("_")
        or member in ["__description__", "__title__", "__version__"]
    ),
    key=str.casefold,
)
