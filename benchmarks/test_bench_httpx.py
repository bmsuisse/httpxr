"""
Benchmark: httpx Python (pure) vs httpr Rust (PyO3)

Run with:
    uv run pytest benchmarks/ -v --benchmark-columns=min,max,mean,stddev,rounds
"""

from __future__ import annotations

import typing
from concurrent.futures import ThreadPoolExecutor, as_completed

import pytest


# ============================================================================
# 1. Single GET latency
# ============================================================================


def test_single_get__python(benchmark: typing.Any, python_client: typing.Any) -> None:
    """Single GET / — pure-Python httpx."""
    benchmark(python_client.get, "/")


def test_single_get__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """Single GET / — Rust httpr."""
    benchmark(rust_client.get, "/")


# ============================================================================
# 2. Single POST latency (1 KB body)
# ============================================================================

SMALL_BODY = b"x" * 1_024  # 1 KB


def test_single_post__python(benchmark: typing.Any, python_client: typing.Any) -> None:
    """Single POST /echo_body with 1 KB body — pure-Python httpx."""
    benchmark(python_client.post, "/echo_body", content=SMALL_BODY)


def test_single_post__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """Single POST /echo_body with 1 KB body — Rust httpr."""
    benchmark(rust_client.post, "/echo_body", content=SMALL_BODY)


# ============================================================================
# 2b. Fast-path raw API latency (no Request/Response objects)
# ============================================================================


def test_single_get_raw__rust(benchmark: typing.Any, rust_client: typing.Any, base_url: str) -> None:
    """Single GET / — Rust httpr raw fast-path."""
    url = base_url + "/"
    benchmark(rust_client.get_raw, url)


def test_single_post_raw__rust(benchmark: typing.Any, rust_client: typing.Any, base_url: str) -> None:
    """Single POST /echo_body with 1 KB body — Rust httpr raw fast-path."""
    url = base_url + "/echo_body"
    benchmark(rust_client.post_raw, url, body=SMALL_BODY)


def _sequential_raw_gets(client: typing.Any, url: str, n: int) -> None:
    for _ in range(n):
        client.get_raw(url)


def test_sequential_raw_gets__rust(
    benchmark: typing.Any, rust_client: typing.Any, base_url: str
) -> None:
    """100 sequential GET / — Rust httpr raw fast-path."""
    url = base_url + "/"
    benchmark(_sequential_raw_gets, rust_client, url, SEQUENTIAL_N)


# ============================================================================
# 3. Sequential throughput — 100 GET requests
# ============================================================================

SEQUENTIAL_N = 100


def _sequential_gets(client: typing.Any, n: int) -> None:
    for _ in range(n):
        client.get("/")


def test_sequential_gets__python(
    benchmark: typing.Any, python_client: typing.Any
) -> None:
    """100 sequential GET / — pure-Python httpx."""
    benchmark(_sequential_gets, python_client, SEQUENTIAL_N)


def test_sequential_gets__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """100 sequential GET / — Rust httpr."""
    benchmark(_sequential_gets, rust_client, SEQUENTIAL_N)


# ============================================================================
# 4. Concurrent throughput — 50 GET requests via ThreadPoolExecutor
# ============================================================================

CONCURRENT_N = 50
CONCURRENT_WORKERS = 10


def _concurrent_gets(
    httpx_mod: typing.Any, base_url: str, n: int, workers: int
) -> None:
    """Each thread creates its own client (connection-pool isolation)."""

    def _do_get(_: int) -> int:
        with httpx_mod.Client(base_url=base_url) as c:
            resp = c.get("/")
        return resp.status_code

    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = [pool.submit(_do_get, i) for i in range(n)]
        for f in as_completed(futures):
            f.result()


def test_concurrent_gets__python(
    benchmark: typing.Any, python_httpx: typing.Any, base_url: str
) -> None:
    """50 concurrent GET / (10 threads) — pure-Python httpx."""
    benchmark(
        _concurrent_gets, python_httpx, base_url, CONCURRENT_N, CONCURRENT_WORKERS
    )


def test_concurrent_gets__rust(
    benchmark: typing.Any, rust_httpx: typing.Any, base_url: str
) -> None:
    """50 concurrent GET / (10 threads) — Rust httpr."""
    benchmark(_concurrent_gets, rust_httpx, base_url, CONCURRENT_N, CONCURRENT_WORKERS)


# ============================================================================
# 5. JSON response parsing
# ============================================================================


def _get_json(client: typing.Any) -> typing.Any:
    resp = client.get("/json")
    return resp.json()


def test_json_parse__python(benchmark: typing.Any, python_client: typing.Any) -> None:
    """GET /json + .json() parse — pure-Python httpx."""
    result = benchmark(_get_json, python_client)
    assert result["message"] == "Hello"


def test_json_parse__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """GET /json + .json() parse — Rust httpr."""
    result = benchmark(_get_json, rust_client)
    assert result["message"] == "Hello"


# ============================================================================
# 6. Large payload — POST 1 MB body
# ============================================================================

LARGE_BODY = b"y" * 1_000_000  # 1 MB


def test_large_post__python(benchmark: typing.Any, python_client: typing.Any) -> None:
    """POST /echo_body with 1 MB body — pure-Python httpx."""
    benchmark(python_client.post, "/echo_body", content=LARGE_BODY)


def test_large_post__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """POST /echo_body with 1 MB body — Rust httpr."""
    benchmark(rust_client.post, "/echo_body", content=LARGE_BODY)


# ============================================================================
# 7. HEAVY: 500 sequential GETs
# ============================================================================

HEAVY_SEQUENTIAL_N = 500


def test_heavy_sequential_gets__python(
    benchmark: typing.Any, python_client: typing.Any
) -> None:
    """500 sequential GET / — pure-Python httpx."""
    benchmark(_sequential_gets, python_client, HEAVY_SEQUENTIAL_N)


def test_heavy_sequential_gets__rust(
    benchmark: typing.Any, rust_client: typing.Any
) -> None:
    """500 sequential GET / — Rust httpr."""
    benchmark(_sequential_gets, rust_client, HEAVY_SEQUENTIAL_N)


# ============================================================================
# 8. HEAVY: 200 concurrent GETs with 50 workers
# ============================================================================

HEAVY_CONCURRENT_N = 200
HEAVY_CONCURRENT_WORKERS = 50


def test_heavy_concurrent_gets__python(
    benchmark: typing.Any, python_httpx: typing.Any, base_url: str
) -> None:
    """200 concurrent GET / (50 threads) — pure-Python httpx."""
    benchmark(
        _concurrent_gets,
        python_httpx,
        base_url,
        HEAVY_CONCURRENT_N,
        HEAVY_CONCURRENT_WORKERS,
    )


def test_heavy_concurrent_gets__rust(
    benchmark: typing.Any, rust_httpx: typing.Any, base_url: str
) -> None:
    """200 concurrent GET / (50 threads) — Rust httpr."""
    benchmark(
        _concurrent_gets,
        rust_httpx,
        base_url,
        HEAVY_CONCURRENT_N,
        HEAVY_CONCURRENT_WORKERS,
    )


# ============================================================================
# 9. HEAVY: POST 10 MB body
# ============================================================================

HUGE_BODY = b"z" * 10_000_000  # 10 MB


def test_huge_post__python(benchmark: typing.Any, python_client: typing.Any) -> None:
    """POST /echo_body with 10 MB body — pure-Python httpx."""
    benchmark(python_client.post, "/echo_body", content=HUGE_BODY)


def test_huge_post__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """POST /echo_body with 10 MB body — Rust httpr."""
    benchmark(rust_client.post, "/echo_body", content=HUGE_BODY)


# ============================================================================
# 10. HEAVY: 100 sequential POSTs with 10 KB body
# ============================================================================

MEDIUM_BODY = b"m" * 10_240  # 10 KB
SEQUENTIAL_POST_N = 100


def _sequential_posts(client: typing.Any, n: int, body: bytes) -> None:
    for _ in range(n):
        client.post("/echo_body", content=body)


def test_heavy_sequential_posts__python(
    benchmark: typing.Any, python_client: typing.Any
) -> None:
    """100 sequential POST /echo_body (10 KB each) — pure-Python httpx."""
    benchmark(_sequential_posts, python_client, SEQUENTIAL_POST_N, MEDIUM_BODY)


def test_heavy_sequential_posts__rust(
    benchmark: typing.Any, rust_client: typing.Any
) -> None:
    """100 sequential POST /echo_body (10 KB each) — Rust httpr."""
    benchmark(_sequential_posts, rust_client, SEQUENTIAL_POST_N, MEDIUM_BODY)


# ============================================================================
# 11. HEAVY: Rapid client create/destroy cycle (connection overhead)
# ============================================================================

CLIENT_CHURN_N = 20


def _client_churn(httpx_mod: typing.Any, base_url: str, n: int) -> None:
    """Create a new client, make one request, destroy it — N times."""
    for _ in range(n):
        with httpx_mod.Client(base_url=base_url) as c:
            c.get("/")


def test_client_churn__python(
    benchmark: typing.Any, python_httpx: typing.Any, base_url: str
) -> None:
    """100× create Client → GET / → close — pure-Python httpx."""
    benchmark(_client_churn, python_httpx, base_url, CLIENT_CHURN_N)


def test_client_churn__rust(
    benchmark: typing.Any, rust_httpx: typing.Any, base_url: str
) -> None:
    """100× create Client → GET / → close — Rust httpr."""
    benchmark(_client_churn, rust_httpx, base_url, CLIENT_CHURN_N)


# ============================================================================
# 12. HEAVY: Large JSON response (1 MB JSON parse)
# ============================================================================


def _get_large_json(client: typing.Any) -> typing.Any:
    resp = client.get("/large_json")
    return resp.json()


def test_large_json_parse__python(
    benchmark: typing.Any, python_client: typing.Any
) -> None:
    """GET /large_json (1 MB) + .json() parse — pure-Python httpx."""
    result = benchmark(_get_large_json, python_client)
    assert "data" in result


def test_large_json_parse__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """GET /large_json (1 MB) + .json() parse — Rust httpr."""
    result = benchmark(_get_large_json, rust_client)
    assert "data" in result


# ============================================================================
# 13. HEAVY: Concurrent POSTs with mixed payloads
# ============================================================================

MIXED_CONCURRENT_N = 20
MIXED_WORKERS = 5
MIXED_BODIES = [
    b"a" * s for s in (1_024, 10_240, 51_200, 102_400)
]  # 1K, 10K, 50K, 100K


def _concurrent_mixed_posts(
    httpx_mod: typing.Any, base_url: str, n: int, workers: int
) -> None:
    def _do_post(i: int) -> int:
        body = MIXED_BODIES[i % len(MIXED_BODIES)]
        with httpx_mod.Client(base_url=base_url) as c:
            resp = c.post("/echo_body", content=body)
        return resp.status_code

    with ThreadPoolExecutor(max_workers=workers) as pool:
        futures = [pool.submit(_do_post, i) for i in range(n)]
        for f in as_completed(futures):
            f.result()


def test_concurrent_mixed_posts__python(
    benchmark: typing.Any, python_httpx: typing.Any, base_url: str
) -> None:
    """100 concurrent POSTs (mixed 1K–100K, 20 threads) — pure-Python httpx."""
    benchmark(
        _concurrent_mixed_posts,
        python_httpx,
        base_url,
        MIXED_CONCURRENT_N,
        MIXED_WORKERS,
    )


def test_concurrent_mixed_posts__rust(
    benchmark: typing.Any, rust_httpx: typing.Any, base_url: str
) -> None:
    """100 concurrent POSTs (mixed 1K–100K, 20 threads) — Rust httpr."""
    benchmark(
        _concurrent_mixed_posts,
        rust_httpx,
        base_url,
        MIXED_CONCURRENT_N,
        MIXED_WORKERS,
    )


# ============================================================================
# 14. NESTED JSON: Shallow (1K records, 2-level nesting) — .json() parse
# ============================================================================


def _parse_nested_json(client: typing.Any, path: str) -> typing.Any:
    """GET nested JSON → .json()."""
    resp = client.get(path)
    return resp.json()


def test_nested_shallow_json__python(
    benchmark: typing.Any, python_client: typing.Any
) -> None:
    """GET /nested_json/shallow (1K, 2-deep) → .json() — Python httpx."""
    data = benchmark(_parse_nested_json, python_client, "/nested_json/shallow")
    assert len(data) == 1_000


def test_nested_shallow_json__rust(
    benchmark: typing.Any, rust_client: typing.Any
) -> None:
    """GET /nested_json/shallow (1K, 2-deep) → .json() — Rust httpr."""
    data = benchmark(_parse_nested_json, rust_client, "/nested_json/shallow")
    assert len(data) == 1_000


# ============================================================================
# 15. NESTED JSON: Deep (500 records, 5-level nesting) — .json() parse
# ============================================================================


def test_nested_deep_json__python(
    benchmark: typing.Any, python_client: typing.Any
) -> None:
    """GET /nested_json/deep (500, 5-deep) → .json() — Python httpx."""
    data = benchmark(_parse_nested_json, python_client, "/nested_json/deep")
    assert len(data) == 500


def test_nested_deep_json__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """GET /nested_json/deep (500, 5-deep) → .json() — Rust httpr."""
    data = benchmark(_parse_nested_json, rust_client, "/nested_json/deep")
    assert len(data) == 500


# ============================================================================
# 16. NESTED JSON: Wide (500 records, 20 metrics + arrays) — .json() parse
# ============================================================================


def test_nested_wide_json__python(
    benchmark: typing.Any, python_client: typing.Any
) -> None:
    """GET /nested_json/wide (500, 20+ keys) → .json() — Python httpx."""
    data = benchmark(_parse_nested_json, python_client, "/nested_json/wide")
    assert len(data) == 500


def test_nested_wide_json__rust(benchmark: typing.Any, rust_client: typing.Any) -> None:
    """GET /nested_json/wide (500, 20+ keys) → .json() — Rust httpr."""
    data = benchmark(_parse_nested_json, rust_client, "/nested_json/wide")
    assert len(data) == 500
