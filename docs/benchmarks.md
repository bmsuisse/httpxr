---
hide:
  - toc
---

# Benchmarks

All benchmarks run against **10 HTTP libraries** on a local ASGI server (uvicorn), 100 rounds each.

## Results

![HTTP Library Benchmark](benchmark_results.png)

## Interactive Chart

Hover over the chart below for detailed statistics per library. Use the Plotly
toolbar (top right) to zoom, pan, or download as PNG.

<iframe src="../benchmark_results.html" width="100%" height="650" frameborder="0" style="border-radius: 8px; border: 1px solid var(--md-default-fg-color--lightest);"></iframe>

## Key Takeaways

| Scenario | httpxr vs httpx |
|:---|:---|
| Single GET | ~2× faster |
| 50 Sequential GETs | ~2.4× faster |
| 50 Concurrent GETs | ~13× faster (GIL-free Rust) |

!!! info "Reproduce locally"
    ```bash
    uv sync --group dev --group benchmark
    uv run python benchmarks/run_benchmark.py
    ```
