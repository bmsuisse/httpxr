#!/usr/bin/env python3
"""
Comprehensive HTTP library benchmark for httpxr.

Compares httpxr against: urllib3, aiohttp, httpx, rnet, niquests, ry,
curl_cffi, and pyreqwest_impersonate.

Usage:
    uv sync --group dev --group benchmark
    uv run python benchmarks/run_benchmark.py

Outputs:
    benchmarks/benchmark_results.html  — interactive Plotly boxplot
    benchmarks/benchmark_results.png   — static image for README
"""

from __future__ import annotations

import asyncio
import statistics
import sys
import time
import typing
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

ROUNDS = 1000  # Number of repetitions per scenario
SEQUENTIAL_N = 50  # Requests per sequential scenario
CONCURRENT_N = 50  # Requests per concurrent scenario
WARMUP_ROUNDS = 3  # Warmup rounds (discarded)

# ---------------------------------------------------------------------------
# Library colors — vibrant palette for dark theme
# ---------------------------------------------------------------------------

LIBRARY_COLORS: dict[str, str] = {
    "httpxr": "#00E676",  # Bright green — our hero
    "httpr": "#B388FF",  # Light purple
    "httpx": "#FF5252",  # Red
    "urllib3": "#FF9800",  # Orange
    "aiohttp": "#448AFF",  # Blue
    "rnet": "#E040FB",  # Purple
    "niquests": "#FFEB3B",  # Yellow
    "ry": "#00BCD4",  # Cyan
    "curl_cffi": "#FF6E40",  # Deep orange
    "pyreqwest": "#76FF03",  # Light green
    "requests": "#9C27B0",    # Purple
}

# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------


class BenchResult(typing.TypedDict):
    library: str
    scenario: str
    round: int
    duration_ms: float


# ---------------------------------------------------------------------------
# Benchmark helpers
# ---------------------------------------------------------------------------


def _time_sync(fn: typing.Callable[[], typing.Any]) -> float:
    """Time a synchronous callable, return elapsed ms."""
    start = time.perf_counter()
    fn()
    return (time.perf_counter() - start) * 1000


def _time_async(
    coro_fn: typing.Callable[[], typing.Coroutine[typing.Any, typing.Any, typing.Any]],
) -> float:
    """Time an async callable, return elapsed ms."""
    start = time.perf_counter()
    asyncio.run(coro_fn())
    return (time.perf_counter() - start) * 1000


# ---------------------------------------------------------------------------
# Library adapters — each returns a list of BenchResult dicts
# ---------------------------------------------------------------------------


def bench_httpxr(base_url: str) -> list[BenchResult]:
    """Benchmark httpxr (sync + async)."""
    import httpxr

    results: list[BenchResult] = []

    # --- Sync ---
    with httpxr.Client(base_url=base_url) as client:
        # Warmup
        for _ in range(WARMUP_ROUNDS):
            client.get("/")

        # Single GET
        for r in range(ROUNDS):
            ms = _time_sync(lambda: client.get("/"))
            results.append(
                {
                    "library": "httpxr",
                    "scenario": "Single GET",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        # Sequential GETs
        for r in range(ROUNDS):

            def seq() -> None:
                for _ in range(SEQUENTIAL_N):
                    client.get("/")

            ms = _time_sync(seq)
            results.append(
                {
                    "library": "httpxr",
                    "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

    # --- Async concurrent (using optimized gather) ---
    async def concurrent_bench() -> list[BenchResult]:
        res: list[BenchResult] = []
        async with httpxr.AsyncClient(base_url=base_url) as client:
            # Warmup
            for _ in range(WARMUP_ROUNDS):
                await client.get("/")

            for r in range(ROUNDS):
                reqs = [client.build_request("GET", "/") for _ in range(CONCURRENT_N)]
                start = time.perf_counter()
                await client.gather(reqs, max_concurrency=CONCURRENT_N)
                ms = (time.perf_counter() - start) * 1000
                res.append(
                    {
                        "library": "httpxr",
                        "scenario": f"{CONCURRENT_N} Concurrent GETs",
                        "round": r,
                        "duration_ms": ms,
                    }
                )
        return res

    results.extend(asyncio.run(concurrent_bench()))
    return results


def bench_urllib3(base_url: str) -> list[BenchResult]:
    """Benchmark urllib3 (sync only)."""
    import urllib3

    results: list[BenchResult] = []
    http = urllib3.PoolManager()
    url = base_url + "/"

    # Warmup
    for _ in range(WARMUP_ROUNDS):
        http.request("GET", url)

    # Single GET
    for r in range(ROUNDS):
        ms = _time_sync(lambda: http.request("GET", url))
        results.append(
            {
                "library": "urllib3",
                "scenario": "Single GET",
                "round": r,
                "duration_ms": ms,
            }
        )

    # Sequential GETs
    for r in range(ROUNDS):

        def seq() -> None:
            for _ in range(SEQUENTIAL_N):
                http.request("GET", url)

        ms = _time_sync(seq)
        results.append(
            {
                "library": "urllib3",
                "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                "round": r,
                "duration_ms": ms,
            }
        )

    # Concurrent via ThreadPool
    for r in range(ROUNDS):

        def conc() -> None:
            with ThreadPoolExecutor(max_workers=10) as pool:
                futs = [
                    pool.submit(http.request, "GET", url) for _ in range(CONCURRENT_N)
                ]
                for f in as_completed(futs):
                    f.result()

        ms = _time_sync(conc)
        results.append(
            {
                "library": "urllib3",
                "scenario": f"{CONCURRENT_N} Concurrent GETs",
                "round": r,
                "duration_ms": ms,
            }
        )

    http.clear()
    return results


def bench_httpx(base_url: str) -> list[BenchResult]:
    """Benchmark httpx (async)."""
    import httpx

    results: list[BenchResult] = []

    # Sync single + sequential
    with httpx.Client(base_url=base_url) as client:
        for _ in range(WARMUP_ROUNDS):
            client.get("/")

        for r in range(ROUNDS):
            ms = _time_sync(lambda: client.get("/"))
            results.append(
                {
                    "library": "httpx",
                    "scenario": "Single GET",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        for r in range(ROUNDS):

            def seq() -> None:
                for _ in range(SEQUENTIAL_N):
                    client.get("/")

            ms = _time_sync(seq)
            results.append(
                {
                    "library": "httpx",
                    "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

    # Async concurrent
    async def concurrent_bench() -> list[BenchResult]:
        res: list[BenchResult] = []
        async with httpx.AsyncClient(base_url=base_url) as client:
            for _ in range(WARMUP_ROUNDS):
                await client.get("/")
            for r in range(ROUNDS):
                start = time.perf_counter()
                tasks = [client.get("/") for _ in range(CONCURRENT_N)]
                await asyncio.gather(*tasks)
                ms = (time.perf_counter() - start) * 1000
                res.append(
                    {
                        "library": "httpx",
                        "scenario": f"{CONCURRENT_N} Concurrent GETs",
                        "round": r,
                        "duration_ms": ms,
                    }
                )
        return res

    results.extend(asyncio.run(concurrent_bench()))
    return results


def bench_httpr(base_url: str) -> list[BenchResult]:
    """Benchmark httpr (PyPI package — another Rust-based client)."""
    import httpr

    results: list[BenchResult] = []
    url = base_url + "/"

    # Sync single + sequential
    with httpr.Client() as client:
        for _ in range(WARMUP_ROUNDS):
            client.get(url)

        for r in range(ROUNDS):
            ms = _time_sync(lambda: client.get(url))
            results.append(
                {
                    "library": "httpr",
                    "scenario": "Single GET",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        for r in range(ROUNDS):

            def seq() -> None:
                for _ in range(SEQUENTIAL_N):
                    client.get(url)

            ms = _time_sync(seq)
            results.append(
                {
                    "library": "httpr",
                    "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

    # Async concurrent
    async def concurrent_bench() -> list[BenchResult]:
        res: list[BenchResult] = []
        async with httpr.AsyncClient() as client:
            for _ in range(WARMUP_ROUNDS):
                await client.get(url)
            for r in range(ROUNDS):
                start = time.perf_counter()
                tasks = [client.get(url) for _ in range(CONCURRENT_N)]
                await asyncio.gather(*tasks)
                ms = (time.perf_counter() - start) * 1000
                res.append(
                    {
                        "library": "httpr",
                        "scenario": f"{CONCURRENT_N} Concurrent GETs",
                        "round": r,
                        "duration_ms": ms,
                    }
                )
        return res

    results.extend(asyncio.run(concurrent_bench()))
    return results


def bench_aiohttp(base_url: str) -> list[BenchResult]:
    """Benchmark aiohttp (async)."""
    import aiohttp

    results: list[BenchResult] = []
    url = base_url + "/"

    async def run_all() -> list[BenchResult]:
        res: list[BenchResult] = []
        async with aiohttp.ClientSession() as session:
            # Warmup
            for _ in range(WARMUP_ROUNDS):
                async with session.get(url) as resp:
                    await resp.read()

            # Single GET
            for r in range(ROUNDS):
                start = time.perf_counter()
                async with session.get(url) as resp:
                    await resp.read()
                ms = (time.perf_counter() - start) * 1000
                res.append(
                    {
                        "library": "aiohttp",
                        "scenario": "Single GET",
                        "round": r,
                        "duration_ms": ms,
                    }
                )

            # Sequential GETs
            for r in range(ROUNDS):
                start = time.perf_counter()
                for _ in range(SEQUENTIAL_N):
                    async with session.get(url) as resp:
                        await resp.read()
                ms = (time.perf_counter() - start) * 1000
                res.append(
                    {
                        "library": "aiohttp",
                        "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                        "round": r,
                        "duration_ms": ms,
                    }
                )

            # Concurrent GETs
            for r in range(ROUNDS):
                start = time.perf_counter()

                async def single_get() -> None:
                    async with session.get(url) as resp:
                        await resp.read()

                await asyncio.gather(*[single_get() for _ in range(CONCURRENT_N)])
                ms = (time.perf_counter() - start) * 1000
                res.append(
                    {
                        "library": "aiohttp",
                        "scenario": f"{CONCURRENT_N} Concurrent GETs",
                        "round": r,
                        "duration_ms": ms,
                    }
                )

        return res

    results.extend(asyncio.run(run_all()))
    return results


def bench_rnet(base_url: str) -> list[BenchResult]:
    """Benchmark rnet (async)."""
    import rnet

    results: list[BenchResult] = []
    url = base_url + "/"

    async def run_all() -> list[BenchResult]:
        res: list[BenchResult] = []

        # Warmup
        for _ in range(WARMUP_ROUNDS):
            resp = await rnet.get(url)
            await resp.text()

        # Single GET
        for r in range(ROUNDS):
            start = time.perf_counter()
            resp = await rnet.get(url)
            await resp.text()
            ms = (time.perf_counter() - start) * 1000
            res.append(
                {
                    "library": "rnet",
                    "scenario": "Single GET",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        # Sequential GETs
        for r in range(ROUNDS):
            start = time.perf_counter()
            for _ in range(SEQUENTIAL_N):
                resp = await rnet.get(url)
                await resp.text()
            ms = (time.perf_counter() - start) * 1000
            res.append(
                {
                    "library": "rnet",
                    "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        # Concurrent GETs
        for r in range(ROUNDS):
            start = time.perf_counter()

            async def single_get() -> None:
                resp = await rnet.get(url)
                await resp.text()

            await asyncio.gather(*[single_get() for _ in range(CONCURRENT_N)])
            ms = (time.perf_counter() - start) * 1000
            res.append(
                {
                    "library": "rnet",
                    "scenario": f"{CONCURRENT_N} Concurrent GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        return res

    results.extend(asyncio.run(run_all()))
    return results


def bench_niquests(base_url: str) -> list[BenchResult]:
    """Benchmark niquests (async)."""
    import niquests

    results: list[BenchResult] = []

    # Sync single + sequential
    with niquests.Session() as session:
        url = base_url + "/"

        for _ in range(WARMUP_ROUNDS):
            session.get(url)

        for r in range(ROUNDS):
            ms = _time_sync(lambda: session.get(url))
            results.append(
                {
                    "library": "niquests",
                    "scenario": "Single GET",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        for r in range(ROUNDS):

            def seq() -> None:
                for _ in range(SEQUENTIAL_N):
                    session.get(url)

            ms = _time_sync(seq)
            results.append(
                {
                    "library": "niquests",
                    "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

    # Async concurrent
    async def concurrent_bench() -> list[BenchResult]:
        res: list[BenchResult] = []
        async with niquests.AsyncSession() as session:
            url = base_url + "/"
            for _ in range(WARMUP_ROUNDS):
                await session.get(url)
            for r in range(ROUNDS):
                start = time.perf_counter()
                tasks = [session.get(url) for _ in range(CONCURRENT_N)]
                await asyncio.gather(*tasks)
                ms = (time.perf_counter() - start) * 1000
                res.append(
                    {
                        "library": "niquests",
                        "scenario": f"{CONCURRENT_N} Concurrent GETs",
                        "round": r,
                        "duration_ms": ms,
                    }
                )
        return res

    results.extend(asyncio.run(concurrent_bench()))
    return results


def bench_ry(base_url: str) -> list[BenchResult]:
    """Benchmark ry (async)."""
    import ry

    results: list[BenchResult] = []
    url = base_url + "/"

    async def run_all() -> list[BenchResult]:
        res: list[BenchResult] = []
        client = ry.HttpClient()

        # Warmup
        for _ in range(WARMUP_ROUNDS):
            await client.get(url)

        # Single GET
        for r in range(ROUNDS):
            start = time.perf_counter()
            await client.get(url)
            ms = (time.perf_counter() - start) * 1000
            res.append(
                {
                    "library": "ry",
                    "scenario": "Single GET",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        # Sequential GETs
        for r in range(ROUNDS):
            start = time.perf_counter()
            for _ in range(SEQUENTIAL_N):
                await client.get(url)
            ms = (time.perf_counter() - start) * 1000
            res.append(
                {
                    "library": "ry",
                    "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        # Concurrent GETs
        for r in range(ROUNDS):
            start = time.perf_counter()
            tasks = [client.get(url) for _ in range(CONCURRENT_N)]
            await asyncio.gather(*tasks)
            ms = (time.perf_counter() - start) * 1000
            res.append(
                {
                    "library": "ry",
                    "scenario": f"{CONCURRENT_N} Concurrent GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        return res

    results.extend(asyncio.run(run_all()))
    return results


def bench_curl_cffi(base_url: str) -> list[BenchResult]:
    """Benchmark curl_cffi (async)."""
    from curl_cffi.requests import AsyncSession, Session

    results: list[BenchResult] = []
    url = base_url + "/"

    # Sync single + sequential
    with Session() as session:
        for _ in range(WARMUP_ROUNDS):
            session.get(url)

        for r in range(ROUNDS):
            ms = _time_sync(lambda: session.get(url))
            results.append(
                {
                    "library": "curl_cffi",
                    "scenario": "Single GET",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        for r in range(ROUNDS):

            def seq() -> None:
                for _ in range(SEQUENTIAL_N):
                    session.get(url)

            ms = _time_sync(seq)
            results.append(
                {
                    "library": "curl_cffi",
                    "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

    # Async concurrent
    async def concurrent_bench() -> list[BenchResult]:
        res: list[BenchResult] = []
        async with AsyncSession() as session:
            for _ in range(WARMUP_ROUNDS):
                await session.get(url)
            for r in range(ROUNDS):
                start = time.perf_counter()
                tasks = [session.get(url) for _ in range(CONCURRENT_N)]
                await asyncio.gather(*tasks)
                ms = (time.perf_counter() - start) * 1000
                res.append(
                    {
                        "library": "curl_cffi",
                        "scenario": f"{CONCURRENT_N} Concurrent GETs",
                        "round": r,
                        "duration_ms": ms,
                    }
                )
        return res

    results.extend(asyncio.run(concurrent_bench()))
    return results


def bench_pyreqwest(base_url: str) -> list[BenchResult]:
    """Benchmark pyreqwest_impersonate (sync only — no async API)."""
    import pyreqwest_impersonate as pri

    results: list[BenchResult] = []
    url = base_url + "/"

    # Sync single + sequential
    client = pri.Client()

    for _ in range(WARMUP_ROUNDS):
        client.get(url)

    for r in range(ROUNDS):
        ms = _time_sync(lambda: client.get(url))
        results.append(
            {
                "library": "pyreqwest",
                "scenario": "Single GET",
                "round": r,
                "duration_ms": ms,
            }
        )

    for r in range(ROUNDS):

        def seq() -> None:
            for _ in range(SEQUENTIAL_N):
                client.get(url)

        ms = _time_sync(seq)
        results.append(
            {
                "library": "pyreqwest",
                "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                "round": r,
                "duration_ms": ms,
            }
        )

    # Concurrent via ThreadPool (no async API available)
    for r in range(ROUNDS):

        def conc() -> None:
            with ThreadPoolExecutor(max_workers=10) as pool:
                futs = [pool.submit(client.get, url) for _ in range(CONCURRENT_N)]
                for f in as_completed(futs):
                    f.result()

        ms = _time_sync(conc)
        results.append(
            {
                "library": "pyreqwest",
                "scenario": f"{CONCURRENT_N} Concurrent GETs",
                "round": r,
                "duration_ms": ms,
            }
        )

    return results

def bench_requests(base_url: str) -> list[BenchResult]:
    """Benchmark requests (sync only)."""
    import requests

    results: list[BenchResult] = []
    url = base_url + "/"

    # Sync single + sequential
    with requests.Session() as session:
        for _ in range(WARMUP_ROUNDS):
            session.get(url)

        for r in range(ROUNDS):
            ms = _time_sync(lambda: session.get(url))
            results.append(
                {
                    "library": "requests",
                    "scenario": "Single GET",
                    "round": r,
                    "duration_ms": ms,
                }
            )

        for r in range(ROUNDS):

            def seq() -> None:
                for _ in range(SEQUENTIAL_N):
                    session.get(url)

            ms = _time_sync(seq)
            results.append(
                {
                    "library": "requests",
                    "scenario": f"{SEQUENTIAL_N} Sequential GETs",
                    "round": r,
                    "duration_ms": ms,
                }
            )

    # Concurrent via ThreadPool (no async API available)
    for r in range(ROUNDS):

        def conc() -> None:
            with requests.Session() as session:
                with ThreadPoolExecutor(max_workers=10) as pool:
                    futs = [pool.submit(session.get, url) for _ in range(CONCURRENT_N)]
                    for f in as_completed(futs):
                        f.result()

        ms = _time_sync(conc)
        results.append(
            {
                "library": "requests",
                "scenario": f"{CONCURRENT_N} Concurrent GETs",
                "round": r,
                "duration_ms": ms,
            }
        )

    return results

# ---------------------------------------------------------------------------
# Registry of all benchmarks
# ---------------------------------------------------------------------------

# Libraries that MUST be benchmarked — abort if missing
REQUIRED_BENCHMARKS: set[str] = {"httpxr", "httpr"}

BENCHMARKS: list[tuple[str, typing.Callable[[str], list[BenchResult]]]] = [
    ("httpxr", bench_httpxr),
    ("httpr", bench_httpr),
    ("urllib3", bench_urllib3),
    ("httpx", bench_httpx),
    ("aiohttp", bench_aiohttp),
    ("rnet", bench_rnet),
    ("niquests", bench_niquests),
    ("ry", bench_ry),
    ("curl_cffi", bench_curl_cffi),
    ("pyreqwest", bench_pyreqwest),
    ("requests", bench_requests),
]


# ---------------------------------------------------------------------------
# Plotly chart generation
# ---------------------------------------------------------------------------


def _hex_to_rgba(hex_color: str, alpha: float = 1.0) -> str:
    """Convert #RRGGBB to rgba() string."""
    r = int(hex_color[1:3], 16)
    g = int(hex_color[3:5], 16)
    b = int(hex_color[5:7], 16)
    return f"rgba({r},{g},{b},{alpha})"


def generate_plotly_html(
    results: list[BenchResult], output_dir: Path
) -> None:
    """Generate clean horizontal boxplot subplots."""
    import plotly.graph_objects as go  # type: ignore[import-untyped]
    from plotly.subplots import make_subplots  # type: ignore[import-untyped]

    scenarios = list(
        dict.fromkeys(r["scenario"] for r in results)
    )
    libraries = list(
        dict.fromkeys(r["library"] for r in results)
    )

    fig = make_subplots(
        rows=len(scenarios),
        cols=1,
        subplot_titles=scenarios,
        shared_xaxes=False,
        vertical_spacing=0.10,
    )

    for row_idx, scenario in enumerate(scenarios, start=1):
        lib_medians: dict[str, float] = {}
        lib_data: dict[str, list[float]] = {}
        for lib in libraries:
            data = [
                r["duration_ms"]
                for r in results
                if r["library"] == lib
                and r["scenario"] == scenario
            ]
            if data:
                lib_medians[lib] = statistics.median(data)
                lib_data[lib] = data

        # Sort slowest → fastest (top → bottom)
        sorted_libs = sorted(
            lib_medians.keys(),
            key=lambda x: lib_medians[x],
            reverse=True,
        )
        fastest = (
            min(lib_medians.values()) if lib_medians else 1
        )

        # Compute 95th percentile across all libs for
        # x-axis clipping
        all_vals: list[float] = []
        for vals in lib_data.values():
            all_vals.extend(vals)
        all_vals.sort()
        p95_idx = int(len(all_vals) * 0.95)
        p95 = all_vals[p95_idx] if all_vals else 1
        x_max = p95 * 1.3  # some headroom

        for lib in sorted_libs:
            hex_c = LIBRARY_COLORS.get(lib, "#888888")
            med = lib_medians[lib]
            ratio = med / fastest if fastest > 0 else 0

            fig.add_trace(
                go.Box(
                    x=lib_data[lib],
                    y=[lib] * len(lib_data[lib]),
                    orientation="h",
                    name=lib,
                    line=dict(color="#888", width=1),
                    fillcolor=_hex_to_rgba(hex_c, 0.25),
                    boxpoints="suspectedoutliers",
                    marker=dict(
                        color=_hex_to_rgba(hex_c, 0.6),
                        size=4,
                        line=dict(width=0),
                        outliercolor=_hex_to_rgba(
                            hex_c, 0.4
                        ),
                    ),
                    boxmean=False,
                    showlegend=False,
                    whiskerwidth=0.6,
                ),
                row=row_idx,
                col=1,
            )

            # Median annotation to the right
            fmt = (
                f"{med:.2f} ms ({ratio:.1f}×)"
                if med < 100
                else f"{med:.0f} ms ({ratio:.1f}×)"
            )
            y_ref = f"y{row_idx}" if row_idx > 1 else "y"
            fig.add_annotation(
                x=1.0,
                y=lib,
                text=fmt,
                showarrow=False,
                font=dict(
                    size=9, color="#555", family="Arial"
                ),
                xanchor="left",
                xref="paper",
                yref=y_ref,
            )

        # X-axis: clipped at 95th pct
        xkey = (
            f"xaxis{row_idx}" if row_idx > 1 else "xaxis"
        )
        fig.update_layout(
            **{
                xkey: dict(
                    title="Duration (ms)",
                    title_font=dict(
                        size=10,
                        color="#888",
                        family="Arial",
                    ),
                    tickfont=dict(
                        size=9,
                        color="#888",
                        family="Arial",
                    ),
                    gridcolor="rgba(0,0,0,0.06)",
                    showline=True,
                    linecolor="#DDD",
                    zeroline=False,
                    ticks="outside",
                    tickcolor="#DDD",
                    range=[0, x_max],
                )
            }
        )

        # Y-axis (library names)
        ykey = (
            f"yaxis{row_idx}" if row_idx > 1 else "yaxis"
        )
        fig.update_layout(
            **{
                ykey: dict(
                    tickfont=dict(
                        size=10,
                        color="#444",
                        family="Arial",
                    ),
                    gridcolor="rgba(0,0,0,0.04)",
                    showline=True,
                    linecolor="#DDD",
                    ticks="",
                    categoryorder="array",
                    categoryarray=sorted_libs,
                )
            }
        )

    # Subplot titles
    for ann in fig.layout.annotations:
        if hasattr(ann, "font") and ann.text in scenarios:
            ann.font = dict(
                size=13, color="#444", family="Arial"
            )

    chart_h = 1200
    chart_w = 900

    fig.update_layout(
        template="plotly_white",
        paper_bgcolor="#FFFFFF",
        plot_bgcolor="#FFFFFF",
        margin=dict(l=90, r=160, t=40, b=50),
        height=chart_h,
        width=chart_w,
        showlegend=False,
    )

    # Save interactive HTML
    html_path = output_dir / "benchmark_results.html"
    fig.write_html(
        str(html_path),
        include_plotlyjs=True,
        full_html=True,
        config={"displayModeBar": True, "responsive": True},
    )
    print(f"  ✅ Interactive HTML: {html_path}")

    # Save static PNG for README
    try:
        png_path = output_dir / "benchmark_results.png"
        fig.write_image(
            str(png_path),
            width=chart_w,
            height=chart_h,
            scale=2,
        )
        print(f"  ✅ Static PNG:      {png_path}")
    except Exception as exc:
        print(
            "  ⚠️  PNG export failed"
            f" (install kaleido): {exc}"
        )
        try:
            svg_path = output_dir / "benchmark_results.svg"
            fig.write_image(
                str(svg_path),
                width=chart_w,
                height=chart_h,
            )
            print(f"  ✅ Static SVG:      {svg_path}")
        except Exception:
            print(
                "  ⚠️  Static image export unavailable"
                " (pip install kaleido)"
            )


# ---------------------------------------------------------------------------
# Print summary table
# ---------------------------------------------------------------------------


def print_summary(results: list[BenchResult]) -> None:
    """Print a compact summary table to stdout."""
    scenarios = list(dict.fromkeys(r["scenario"] for r in results))
    libraries = list(dict.fromkeys(r["library"] for r in results))

    print("\n" + "=" * 80)
    print("  BENCHMARK SUMMARY  (median, ms)")
    print("=" * 80)

    for scenario in scenarios:
        print(f"\n  📊 {scenario}")
        print(
            f"  {'Library':<18} {'Median':>10} {'Mean':>10} {'StdDev':>10} {'Min':>10} {'Max':>10}"
        )
        print(f"  {'─' * 68}")

        medians: list[tuple[str, float, list[float]]] = []
        for lib in libraries:
            data = [
                r["duration_ms"]
                for r in results
                if r["library"] == lib and r["scenario"] == scenario
            ]
            if data:
                medians.append((lib, statistics.median(data), data))

        medians.sort(key=lambda x: x[1])
        for lib, med, data in medians:
            mean = statistics.mean(data)
            std = statistics.stdev(data) if len(data) > 1 else 0
            mn, mx = min(data), max(data)
            fmt = lambda v: f"{v:.2f}" if v < 100 else f"{v:.1f}"
            print(
                f"  {lib:<18} {fmt(med):>10} {fmt(mean):>10} {fmt(std):>10} {fmt(mn):>10} {fmt(mx):>10}"
            )

    print("\n" + "=" * 80)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    # Add project root to path so we can import benchmark server
    project_root = Path(__file__).resolve().parent.parent
    sys.path.insert(0, str(project_root / "benchmarks"))

    from server import start_server, stop_server

    print("🚀 Starting benchmark server...")
    srv, thread, base_url = start_server()
    print(f"   Server running at {base_url}")

    all_results: list[BenchResult] = []

    try:
        for lib_name, bench_fn in BENCHMARKS:
            print(f"\n⏱  Benchmarking {lib_name}...")
            try:
                lib_results = bench_fn(base_url)
                all_results.extend(lib_results)
                n_scenarios = len(set(r["scenario"] for r in lib_results))
                print(
                    f"   ✅ {lib_name}: {len(lib_results)} measurements across {n_scenarios} scenarios"
                )
            except ImportError as exc:
                if lib_name in REQUIRED_BENCHMARKS:
                    print(f"   ❌ {lib_name}: REQUIRED but not installed: {exc}")
                    print("   💡 Install with: uv sync --group benchmark")
                    sys.exit(1)
                print(f"   ⏭  {lib_name}: skipped (not installed: {exc})")
            except Exception as exc:
                if lib_name in REQUIRED_BENCHMARKS:
                    print(f"   ❌ {lib_name}: REQUIRED but failed: {exc}")
                    sys.exit(1)
                print(f"   ❌ {lib_name}: failed ({exc})")

        if not all_results:
            print("\n❌ No results collected! Check library installations.")
            sys.exit(1)

        # Print summary
        print_summary(all_results)

        # Generate Plotly charts
        print("\n📊 Generating charts...")
        output_dir = Path(__file__).resolve().parent
        generate_plotly_html(all_results, output_dir)

    finally:
        print("\n🛑 Stopping server...")
        stop_server(srv, thread)

    print("\n✅ Benchmark complete!")


if __name__ == "__main__":
    main()
