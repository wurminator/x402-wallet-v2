## 2026-09-15 - Security Enhancements: Memory Zeroization, Secure Directories, and CI/CD Automation
**Vulnerability:** Derived keystore keys were left in memory longer than necessary, config/keystore directories lacked explicit restricted permissions, and automated unlocks required interactive prompts.
**Learning:** In cryptographic utilities, intermediate secrets must be explicitly zeroized, sensitive files must have strict OS-level permissions (e.g., `0o700` for directories on Unix), and environment variables offer safe CI/CD paths.
**Prevention:** Always use `zeroize::Zeroizing` wrappers for derived keys (`key_bytes`), use the newly implemented `crate::utils::secure_create_dir_all` for sensitive directories, and support `X402_KEYSTORE_PASSWORD` for non-interactive password handling.
