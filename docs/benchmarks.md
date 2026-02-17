---
hide:
  - toc
---

# Benchmarks

All benchmarks run against **10 HTTP libraries** on a local ASGI server (uvicorn), 100 rounds each.

## Results

Hover over the charts for detailed statistics per library.

### Single GET

<iframe src="../benchmark_single.html" width="100%" height="400" frameborder="0" scrolling="no" style="overflow:hidden"></iframe>

### 50 Sequential GETs

<iframe src="../benchmark_sequential.html" width="100%" height="400" frameborder="0" scrolling="no" style="overflow:hidden"></iframe>

### 50 Concurrent GETs

<iframe src="../benchmark_concurrent.html" width="100%" height="400" frameborder="0" scrolling="no" style="overflow:hidden"></iframe>

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
