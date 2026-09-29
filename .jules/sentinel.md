## 2023-10-24 - Secure Directory Creation for Sensitive Data
**Vulnerability:** Directories containing sensitive configuration and keystores were created using default permissions (often `0o755` or `0o775` depending on umask), allowing potentially broad read access to sensitive material by other users on the system.
**Learning:** Functions like `std::fs::create_dir_all` rely on the user's default umask and do not guarantee restricted permissions, exposing sensitive files.
**Prevention:** Use a wrapper like `crate::utils::secure_create_dir_all` to explicitly enforce restricted permissions (`0o700` on Unix systems) when creating directories intended to hold sensitive information.
