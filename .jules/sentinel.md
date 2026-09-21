## 2025-09-21 - Secure Configuration Directories
**Vulnerability:** Directory creation in `src/store.rs` and `src/evm.rs` uses standard `fs::create_dir_all`, which can result in overly permissive permissions (e.g., `drwxr-xr-x` / `0o755`) for the `.x402wallet` configuration directory and its contents depending on umask.
**Learning:** Configurations containing sensitive data, such as encrypted keystores, must restrict read and write access strictly to the owner. This requires explicit directory permissions when creating configuration paths (e.g., `0o700`).
**Prevention:** Always use a helper like `secure_create_dir_all` that uses `DirBuilder` with explicit `mode(0o700)` on Unix when creating sensitive application directories.
