## 2024-05-15 - Unsafe memory management for credentials with manual zeroization
**Vulnerability:** Keystore passphrases were wiped via `pass.zeroize()` manually at the successful end of the functions `init_wallet` and `load_private_key_hex`. But if these functions returned an error early (e.g. `passphrases do not match`), the memory was not zeroized, causing sensitive strings to remain in memory.
**Learning:** Manual `.zeroize()` is an anti-pattern. Early returns drop variables out of scope, making the cleanup step unreachable.
**Prevention:** Always use RAII wrappers like `zeroize::Zeroizing::new()` for sensitive material in Rust so that memory is zeroized deterministically on `Drop`, no matter how the function exits.
