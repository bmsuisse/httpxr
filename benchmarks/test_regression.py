"""
Performance regression tests using pytest-benchmark.

These tests enforce strict latency thresholds to catch performance regressions
before they reach production. Run with:

    uv run maturin develop --release
    uv run pytest benchmarks/test_regression.py -v
"""

from __future__ import annotations

import typing

import pytest


# Threshold: median must stay below this (in seconds)
# 0.50ms = 500μs — threshold for v0.30.7 before deadlocking optimizations
SINGLE_GET_THRESHOLD_S = 0.00050  # 0.50ms


def test_sync_single_get_regression(
    benchmark: typing.Any,
    base_url: str,
    rust_client: typing.Any,
) -> None:
    """
    Performance regression gate for httpxr Single GET.

    Enforces that the median latency stays below 0.50ms on a local ASGI server.
    If this fails, either:
      1. You introduced a performance regression, or
      2. You're running a debug build — use `maturin develop --release`.
    """
    benchmark(rust_client.get, "/")

    median_s = benchmark.stats.stats.median
    median_ms = median_s * 1000
    print(f"\n[Regression] httpxr Single GET Median: {median_ms:.4f} ms")

    # Detect debug builds (usually > 1ms)
    if median_ms > 1.0:
        pytest.fail(
            f"httpxr is extremely slow ({median_ms:.2f}ms). "
            "Are you running a DEBUG build? Use `maturin develop --release`."
        )

    if median_s > SINGLE_GET_THRESHOLD_S:
        pytest.fail(
            f"Performance regression: {median_ms:.4f}ms "
            f"(threshold: {SINGLE_GET_THRESHOLD_S * 1000:.2f}ms)"
        )
