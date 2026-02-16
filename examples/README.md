# reqx Examples

A collection of examples demonstrating the `reqx` HTTP client — a Rust-backed,
drop-in replacement for `httpx`.

## Quick Start

```bash
# Install reqx (from the project root)
uv pip install -e .

# Run any example
uv run python examples/basic_requests.py
```

## Examples

| File | Description |
|------|-------------|
| [`basic_requests.py`](basic_requests.py) | Simple GET/POST/PUT/PATCH/DELETE using top-level API |
| [`client_usage.py`](client_usage.py) | Connection-pooling `Client` with context manager |
| [`async_client.py`](async_client.py) | Async requests with `AsyncClient` and `asyncio` |
| [`headers_and_params.py`](headers_and_params.py) | Custom headers, query parameters, and cookies |
| [`json_and_forms.py`](json_and_forms.py) | Sending JSON, form-encoded data, and multipart files |
| [`streaming.py`](streaming.py) | Streaming responses with `client.stream()` |
| [`error_handling.py`](error_handling.py) | Status checks, `raise_for_status()`, and exception types |
| [`timeouts_and_limits.py`](timeouts_and_limits.py) | Configuring `Timeout` and `Limits` |
| [`mock_transport.py`](mock_transport.py) | Response/Request/URL objects for testing — no network needed |
| [`authentication.py`](authentication.py) | `BasicAuth`, `DigestAuth`, and custom `FunctionAuth` |
