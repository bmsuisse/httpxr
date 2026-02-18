# Compatibility Shim

httpxr includes a **zero-config migration shim** that lets you try httpxr in
existing codebases without changing a single import.

## Quick Start

Add **one line** at the top of your application's entrypoint:

```python
import httpxr.compat
```

That's it. Every `import httpx` — in your code **and** in third-party
libraries — now resolves to httpxr.

```python
import httpxr.compat   # ← enable the shim

import httpx           # ← actually resolves to httpxr now

with httpx.Client() as client:
    r = client.get("https://httpbin.org/get")
    print(r.status_code)  # uses Rust transport 🚀
```

---

## How It Works

When you `import httpxr.compat`, the module:

1. Imports `httpxr`
2. Registers it as `sys.modules["httpx"]`
3. Logs an info-level message: `httpxr.compat: httpx → httpxr shim active`

Any subsequent `import httpx` — even from third-party packages — will receive
the httpxr module instead.

!!! note "One-time setup"
    The shim only needs to be imported **once**, as early as possible in your
    application. A good place is `__init__.py`, `manage.py`, `main.py`, or
    your ASGI/WSGI entrypoint.

---

## API Reference

### `httpxr.compat.is_active()`

Returns `True` if the shim is currently active.

```python
import httpxr.compat

print(httpxr.compat.is_active())  # True
```

### `httpxr.compat.disable()`

Disables the shim and restores the original `httpx` module (if it was
installed). Useful for testing or if you need to switch back.

```python
import httpxr.compat

httpxr.compat.disable()
print(httpxr.compat.is_active())  # False
```

---

## Use Cases

### Try httpxr in an existing project

```python
# settings.py (Django) or __init__.py (FastAPI)
import httpxr.compat
```

All your existing `import httpx` code and any libraries that depend on httpx
(like `authlib`, `httpx-auth`, etc.) will transparently use httpxr.

### A/B performance testing

```python
import os

if os.environ.get("USE_HTTPXR"):
    import httpxr.compat  # noqa: F401

import httpx  # uses httpxr or httpx based on env var
```

### Gradual migration

Start with the shim, then progressively replace `import httpx` with
`import httpxr` across your codebase at your own pace.

---

## Caveats

!!! warning "Import order matters"
    `import httpxr.compat` should be called **before** any `import httpx`.
    If httpx is already imported, the shim will still activate but will emit
    a warning — modules that already hold references to the original httpx
    objects will not be affected.

!!! warning "Internal module paths"
    The shim redirects the top-level `httpx` module. Code that reaches into
    httpx internals like `httpx._transports` or `httpx._models` will not
    work through the shim, since httpxr has a different internal structure.
    The entire **public API** is fully supported.

---

## FAQ

**Q: Will the shim affect my production performance?**

No. The shim is just a `sys.modules` assignment (one dict write). It adds
zero overhead to HTTP requests.

**Q: Can I use the shim in tests?**

Yes. Use `httpxr.compat.disable()` in teardown if you need to restore
the original httpx for specific tests.

**Q: What if httpx is not installed at all?**

That's fine — the shim doesn't need httpx to be installed. It simply
registers httpxr under the name `httpx`.

**Q: Does this work with `respx` (httpx mock library)?**

It depends. `respx` patches httpx internals, so it may not work with the
shim. For testing, use httpxr's built-in `MockTransport` instead.
