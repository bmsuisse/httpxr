"""
httpxr.compat — drop-in migration shim for httpx.

Import once at your application's entrypoint to redirect all ``import httpx``
statements to httpxr (including third-party libraries)::

    import httpxr.compat   # enable the shim
    import httpx            # ← now resolves to httpxr

    httpxr.compat.is_active()  # True
    httpxr.compat.disable()    # restore original httpx
"""

from __future__ import annotations

import logging
import sys
import types
import warnings

import httpxr

logger = logging.getLogger("httpxr.compat")

_original_httpx: types.ModuleType | None = None
_shim_active: bool = False


def _activate() -> None:
    global _original_httpx, _shim_active

    if _shim_active:
        return

    if "httpx" in sys.modules:
        _original_httpx = sys.modules["httpx"]
        warnings.warn(
            "httpxr.compat: 'httpx' was already imported. "
            "The shim will override it, but modules that already hold "
            "a reference to the original httpx objects will not be affected.",
            stacklevel=3,
        )

    sys.modules["httpx"] = httpxr  # type: ignore[assignment]
    _shim_active = True
    logger.info("httpxr.compat: httpx → httpxr shim active")


def disable() -> None:
    """Disable the shim and restore the original ``httpx`` module (if any)."""
    global _original_httpx, _shim_active

    if not _shim_active:
        return

    if _original_httpx is not None:
        sys.modules["httpx"] = _original_httpx
        _original_httpx = None
    else:
        sys.modules.pop("httpx", None)

    _shim_active = False
    logger.info("httpxr.compat: shim disabled")


def is_active() -> bool:
    """Return ``True`` if the httpx → httpxr shim is currently active."""
    return _shim_active


_activate()
