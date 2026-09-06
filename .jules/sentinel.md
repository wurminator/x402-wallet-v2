## 2024-09-06 - [Memory Zeroization for Derived Keys]
**Vulnerability:** Derived encryption keys (`[u8; 32]` arrays from Argon2) were being left in memory after use, posing a risk of memory leaks for sensitive key material.
**Learning:** Raw arrays (`[u8; 32]`) used for sensitive data do not automatically zeroize their memory when dropped in Rust.
**Prevention:** Use Drop wrappers like `zeroize::Zeroizing` to ensure sensitive data is securely erased from RAM as soon as it goes out of scope.
