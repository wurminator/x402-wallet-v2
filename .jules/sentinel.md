## 2024-11-28 - [Secure Directory Creation]
**Vulnerability:** Directories containing sensitive configuration or keystores were created using `fs::create_dir_all` without ensuring restricted permissions (e.g., mode `0o700` on Unix), potentially exposing them to unauthorized access by other users on the system.
**Learning:** Standard library functions like `fs::create_dir_all` do not guarantee secure permissions by default. When storing sensitive data, directory permissions must be explicitly restricted during creation.
**Prevention:** Use a secure wrapper like `secure_create_dir_all` that explicitly sets `mode(0o700)` on Unix systems when creating directories for sensitive data like the application directory containing keystores.
