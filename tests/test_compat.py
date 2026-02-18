"""Tests for the httpxr.compat migration shim."""

from __future__ import annotations

import sys
import types
import warnings

import pytest


def _cleanup_httpx_modules() -> None:
    """Remove httpx and httpxr.compat from sys.modules for test isolation."""
    for key in list(sys.modules.keys()):
        if key == "httpx" or key.startswith("httpx."):
            del sys.modules[key]
    # Reset the compat module state
    if "httpxr.compat" in sys.modules:
        mod = sys.modules["httpxr.compat"]
        mod._shim_active = False  # type: ignore[attr-defined]
        mod._original_httpx = None  # type: ignore[attr-defined]


@pytest.fixture(autouse=True)
def _isolate_compat() -> None:  # type: ignore[return]
    """Ensure each test starts with a clean module state."""
    _cleanup_httpx_modules()
    yield  # type: ignore[misc]
    _cleanup_httpx_modules()


class TestCompatShim:
    """Tests for the httpx → httpxr shim."""

    def test_import_enables_shim(self) -> None:
        """Importing httpxr.compat should register httpxr as httpx."""
        import httpxr.compat

        assert httpxr.compat.is_active()
        assert "httpx" in sys.modules

        import httpxr

        assert sys.modules["httpx"] is httpxr

    def test_httpx_resolves_to_httpxr(self) -> None:
        """After enabling the shim, import httpx should return httpxr."""
        import httpxr.compat  # noqa: F401

        # This import should resolve to httpxr
        httpx = __import__("httpx")

        import httpxr

        assert httpx is httpxr

    def test_client_works_through_shim(self) -> None:
        """Client class should work when accessed via httpx after shim."""
        import httpxr.compat  # noqa: F401

        httpx = __import__("httpx")

        import httpxr

        assert httpx.Client is httpxr.Client
        assert httpx.AsyncClient is httpxr.AsyncClient

    def test_response_works_through_shim(self) -> None:
        """Response and other types should be accessible via httpx."""
        import httpxr.compat  # noqa: F401

        httpx = __import__("httpx")

        import httpxr

        assert httpx.Response is httpxr.Response
        assert httpx.Request is httpxr.Request
        assert httpx.URL is httpxr.URL

    def test_disable_restores_state(self) -> None:
        """disable() should remove httpxr from sys.modules['httpx']."""
        import httpxr.compat

        assert httpxr.compat.is_active()

        httpxr.compat.disable()

        assert not httpxr.compat.is_active()
        assert "httpx" not in sys.modules

    def test_disable_restores_original_httpx(self) -> None:
        """If httpx was previously imported, disable() should restore it."""
        # Create a fake httpx module
        fake_httpx = types.ModuleType("httpx")
        fake_httpx.__version__ = "0.99.0"  # type: ignore[attr-defined]
        sys.modules["httpx"] = fake_httpx

        import httpxr.compat

        assert httpxr.compat.is_active()

        # httpx should now be httpxr
        import httpxr

        assert sys.modules["httpx"] is httpxr

        # Disable should restore the fake
        httpxr.compat.disable()
        assert sys.modules["httpx"] is fake_httpx

    def test_warns_when_httpx_already_imported(self) -> None:
        """Shim should warn if httpx was already in sys.modules."""
        fake_httpx = types.ModuleType("httpx")
        sys.modules["httpx"] = fake_httpx

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            import httpxr.compat

            httpxr.compat._activate()  # force re-check after fixture cleanup

        # Check that a warning was issued
        httpx_warnings = [
            x for x in w if "already imported" in str(x.message)
        ]
        assert len(httpx_warnings) >= 1

    def test_is_active_reflects_state(self) -> None:
        """is_active() should reflect the current shim state."""
        import httpxr.compat

        assert httpxr.compat.is_active()

        httpxr.compat.disable()
        assert not httpxr.compat.is_active()

    def test_double_activate_is_safe(self) -> None:
        """Calling _activate() twice should be a no-op."""
        import httpxr.compat

        assert httpxr.compat.is_active()
        # Second activation should not raise or change state
        httpxr.compat._activate()
        assert httpxr.compat.is_active()

    def test_double_disable_is_safe(self) -> None:
        """Calling disable() twice should be a no-op."""
        import httpxr.compat

        httpxr.compat.disable()
        assert not httpxr.compat.is_active()
        # Second disable should not raise
        httpxr.compat.disable()
        assert not httpxr.compat.is_active()

    def test_exception_hierarchy_through_shim(self) -> None:
        """Exceptions should be accessible through the shim."""
        import httpxr.compat  # noqa: F401

        httpx = __import__("httpx")

        import httpxr

        assert httpx.HTTPError is httpxr.HTTPError
        assert httpx.RequestError is httpxr.RequestError
        assert httpx.TimeoutException is httpxr.TimeoutException
