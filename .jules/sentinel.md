## 2025-01-20 - Robust Memory Zeroization using Drop Wrappers
**Vulnerability:** Manual `zeroize()` calls on sensitive variables (passwords, keys) are easily bypassed if the function returns early due to an error, allowing sensitive data to persist in memory.
**Learning:** Rust's control flow makes manual cleanup fragile. Any `?` operator before the cleanup guarantees a leak if it triggers.
**Prevention:** Always use `zeroize::Zeroizing` to wrap sensitive data upon creation. It implements the `Drop` trait to guarantee zeroization regardless of how the scope is exited.
