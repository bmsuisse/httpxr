"""
httpxr.compat — Drop-in migration shim for httpx.

Import this module once at your application's entrypoint to transparently
redirect all ``import httpx`` statements to httpxr::

    import httpxr.compat   # enable the shim
    import httpx            # ← now resolves to httpxr

This works for your code **and** for third-party libraries that depend on httpx.

Usage::

    # Enable
    import httpxr.compat

    # Check status
    httpxr.compat.is_active()  # True

    # Disable (restore original httpx if it was installed)
    httpxr.compat.disable()
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
    """Register httpxr as the ``httpx`` module in ``sys.modules``."""
    global _original_httpx, _shim_active

    if _shim_active:
        return

    # Preserve existing httpx if already imported
    if "httpx" in sys.modules:
        _original_httpx = sys.modules["httpx"]
        warnings.warn(
            "httpxr.compat: 'httpx' was already imported. "
            "The shim will override it, but modules that already hold "
            "a reference to the original httpx objects will not be affected.",
            stacklevel=3,
        )

    # Register httpxr as httpx
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


# Auto-activate on import
_activate()
