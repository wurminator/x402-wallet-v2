## 2024-05-18 - [CRITICAL] Prevent Passphrase Leaks on Early Return
**Vulnerability:** Passphrases and derived keys were manually zeroized at the end of functions, which meant they were left in memory if the function returned early (e.g., due to an error decoding `hex`).
**Learning:** Manual `.zeroize()` is insecure for early returns. `Drop` semantics must be used.
**Prevention:** Always use `zeroize::Zeroizing` wrapper for sensitive data like passphrases and derived keys so that zeroization happens automatically when variables go out of scope, even on `?` early returns.

## 2024-05-18 - [HIGH] Enforce 0700 Permissions for Keystore Directories
**Vulnerability:** The app directory (`~/.x402wallet`) was created using `std::fs::create_dir_all` without specifying restricted permissions, defaulting to umask, which could leave sensitive keystore and configuration files readable by other users.
**Learning:** Keystore and config directories must be restricted to the owner (`0700`), just like the keystore files themselves.
**Prevention:** On Unix platforms, use `std::fs::DirBuilder` with `.mode(0o700)` when creating directories that store sensitive files.
