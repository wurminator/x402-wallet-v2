## 2024-09-14 - Use RAII for memory zeroization of sensitive data

**Vulnerability:** Memory leak of sensitive data (like keystore passphrases) due to bypassed manual `.zeroize()` calls on early returns.
**Learning:** Manual `.zeroize()` placed at the end of a block is ineffective if an error occurs and the function returns early (e.g., via the `?` operator).
**Prevention:** Always use Drop wrappers like `zeroize::Zeroizing` (RAII pattern) for sensitive data such as passphrases. This ensures memory is securely zeroized when the variable goes out of scope, regardless of how the block exits (normal return, early return, or panic).
