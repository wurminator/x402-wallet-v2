## 2025-01-20 - Non-interactive Keystore Unlocking

**Vulnerability:** Keystore unlocking unconditionally blocks on interactive password prompt (`prompt_password`), preventing programmatic or automated execution (e.g. CI environments or when used as an agent). See `.wallet.md:466:6. ❌ Use keystore mode with agents (blocks on password prompt)`.
**Learning:** Hardcoded interactive prompts are an availability risk and usability issue for automated workflows. Standard practice is to support unlocking via environment variable alongside interactive prompt.
**Prevention:** Support providing keystore password via environment variable (`X402_KEYSTORE_PASSWORD`), and only fallback to interactive prompt if the environment variable is not present. This prevents blocking execution in automated environments.
