## 2024-05-20 - [Insecure Default Permissions on Sensitive Directories]
**Vulnerability:** Directories containing sensitive configuration and keystores were being created with default, overly permissive access rights using `fs::create_dir_all`.
**Learning:** File system abstractions often default to standard permissive directories. In environments where sensitive data (like encrypted keys or private configuration) is stored locally, explicit permission narrowing is necessary.
**Prevention:** Use a wrapper function like `secure_create_dir_all` that uses `DirBuilderExt` to explicitly set `mode(0o700)` on Unix systems when creating directories meant for sensitive storage.
