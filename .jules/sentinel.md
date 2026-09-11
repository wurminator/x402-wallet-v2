## 2023-11-14 - Sensitive Directory Permissions
**Vulnerability:** The application directory (`~/.x402wallet`), which stores the encrypted keystore and configuration file, was created using `fs::create_dir_all` with default system permissions (often `0o755`). This allowed other users on the system to read the sensitive files.
**Learning:** Even if files within a directory have restricted permissions, the parent directory containing sensitive key material should also explicitly enforce restricted access permissions to provide defense in depth and avoid leaking the existence or metadata of files.
**Prevention:** Use `std::fs::DirBuilder` with an explicit `mode(0o700)` on Unix systems when creating directories meant to store sensitive user data, configurations, or keystores.
