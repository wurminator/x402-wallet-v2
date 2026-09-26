## 2024-05-24 - Overly Permissive Directory Creation
**Vulnerability:** The application directory (`~/.x402wallet/`) which stores sensitive wallet keystore files and configurations was created using standard `std::fs::create_dir_all`, leading to default OS permissions (often world-readable depending on umask).
**Learning:** Default directory creation in Rust does not restrict permissions for sensitive data storage, which can allow unauthorized local access to private keys.
**Prevention:** Always use `std::fs::DirBuilder` with `mode(0o700)` on Unix systems when creating directories intended to store sensitive information like cryptographic keys or private configuration files.
