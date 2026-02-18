from . import _exceptions  # noqa: F401
from ._httpxr import *
from ._transports import ASGITransport, WSGITransport

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

__all__ = sorted(
    (
        member
        for member in list(vars().keys())
        if (
            not member.startswith("_")
            or member in ["__description__", "__title__", "__version__"]
        )
        and member not in _EXCLUDED_FROM_ALL
    ),
    key=str.casefold,
)

