## 2024-09-07 - Zeroize on Drop for Sensitive Variables
**Vulnerability:** Explicit manual `zeroize()` calls at the end of functions for passwords and derived keys. If an intermediate step returned early via `?` (e.g. `hex::decode`), the secrets remained in memory and were dropped without zeroization.
**Learning:** Manual zeroization is fragile in Rust when early returns (`?`) are common. Memory can leak on error paths.
**Prevention:** Always use Drop wrappers like `zeroize::Zeroizing` for sensitive strings and byte arrays so they are reliably zeroized when they go out of scope, including on early error returns.
