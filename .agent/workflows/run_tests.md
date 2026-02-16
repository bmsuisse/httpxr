---
description: Run the httpr test suite
---

# Run Tests

// turbo-all

1. Run full test suite:
```bash
uv run pytest tests/ -x -q
```

2. Run a specific test file:
```bash
uv run pytest tests/<test_file>.py -x -v
```

3. Run tests matching a keyword:
```bash
uv run pytest tests/ -k "<keyword>" -x -v
```

4. Run benchmarks:
```bash
uv run pytest benchmarks/ -v --benchmark-columns=min,max,mean,stddev,rounds
```
