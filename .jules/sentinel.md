## 2024-05-24 - Secure Directory Creation
**Vulnerability:** Directories storing sensitive configurations and keystores (like the app directory `~/.x402wallet`) were created with default permissions, which could allow unauthorized local users to read sensitive files.
**Learning:** `std::fs::create_dir_all` uses default umask permissions, which is unsafe for directories containing secrets.
**Prevention:** Use a helper function `secure_create_dir_all` with `DirBuilder` to enforce restricted permissions (`0o700` on Unix systems) when creating directories that store sensitive data.
