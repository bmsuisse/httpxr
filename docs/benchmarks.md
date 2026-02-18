---
hide:
  - toc
---

# Benchmarks

All benchmarks run against **10 HTTP libraries** on a local ASGI server (uvicorn), 100 rounds each.

## Results

Hover over the charts for detailed statistics per library.

### Single GET

<div class="benchmark-chart">
<iframe src="../benchmark_single.html" frameborder="0" scrolling="no"></iframe>
</div>

### 50 Sequential GETs

<div class="benchmark-chart">
<iframe src="../benchmark_sequential.html" frameborder="0" scrolling="no"></iframe>
</div>

### 50 Concurrent GETs

<div class="benchmark-chart">
<iframe src="../benchmark_concurrent.html" frameborder="0" scrolling="no"></iframe>
</div>

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
