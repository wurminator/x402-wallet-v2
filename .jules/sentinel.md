## 2024-05-18 - Insecure Directory Creation
**Vulnerability:** Sensitive directories (like the `.x402wallet` app config and keystore directory) were being created with default permissions using `std::fs::create_dir_all`, potentially allowing other users on the system to read sensitive keys and config files.
**Learning:** Default directory creation permissions on Unix systems are often too permissive (e.g., `0o755`) for directories holding sensitive information.
**Prevention:** Implement and use a helper function like `secure_create_dir_all` which uses `std::fs::DirBuilder` with `mode(0o700)` on Unix systems to ensure restricted access to sensitive directories.
