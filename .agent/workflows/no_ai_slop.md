---
description: Code quality enforcement — no AI slop
---

# No AI Slop Checklist

Before generating or modifying code, verify:

## Banned

- [ ] No inline comments explaining what code does
- [ ] No `# Step N:` narration
- [ ] No generic `try/except Exception`
- [ ] No `hasattr()` or `isinstance()` on statically typed objects
- [ ] No `getattr(obj, "x", None)` on typed objects
- [ ] No verbose docstrings restating signatures
- [ ] No TODOs or FIXMEs left in committed code
- [ ] No `.unwrap()` in Rust library code
- [ ] No `println!` in Rust — use `log` macros
- [ ] No `print()` in Python — use `logging` if needed

## Required

- [ ] All Python functions have type annotations
- [ ] All Rust functions return `PyResult` at the boundary
- [ ] File stays under 1000 lines
- [ ] No code duplication — extract shared logic
- [ ] Names are self-documenting
- [ ] Pyright passes with 0 errors
- [ ] `cargo check` passes
