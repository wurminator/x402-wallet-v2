# ADR-0004: Harden the keystore KDF and file permissions

- **Status:** Accepted
- **Date:** 2026-08-23
- **Scope:** Keystore encryption in `src/store.rs`

## Context

Code review finding (medium): `Argon2::default()` (Argon2id, 19 MiB, t=2,
p=1) is only the OWASP minimum; RFC 9106 §4 recommends stronger parameters
for high-value key material (m = 64 MiB, t = 3, p = 4 when large memory is
unavailable). The keystore protects a private key that directly controls
funds. Additionally, `keystore.json` was written via plain `fs::write`
with no file permissions — the same CWE-732 gap fixed for `.env` earlier.

## Decision

1. **New keystores** are encrypted with RFC 9106 §4 parameters:
   Argon2id, m = 65536 KiB (64 MiB), t = 3, p = 4.
2. **The parameters are recorded in `keystore.json`** (`m_cost`, `t_cost`,
   `p_cost`). Decryption derives the key with exactly the recorded values.
3. **Backward compatibility:** pre-hardening files carry no parameter
   fields; serde defaults them to the legacy `Argon2::default()` values
   (19456 KiB, t=2, p=1), so old keystores keep decrypting unchanged.
   There is no migration — a re-`wallet-init --keystore` re-encrypts with
   the new parameters if the user wants it.
4. **`keystore.json` is written owner-only**, same as `.env`: created with
   mode 0600 on Unix (and repaired on every write), best-effort
   `icacls /inheritance:r /grant:r <user>:F` on Windows. Both paths share
   the `write_private()` helper.

## Alternatives rejected

- RFC 9106 first option (m = 2 GiB, t=1, p=4) — rejected: too heavy for a
  CLI that runs on small VPS/container environments; the 64 MiB option is
  the RFC's explicit recommendation for that case.
- Keeping `Argon2::default()` — rejected: OWASP minimum, review finding.
- Silent re-encryption of old keystores on unlock — rejected: writing key
  material back to disk as a side effect of a read path is a risk of its
  own; explicit re-init is safer and simpler.

## Consequences

- Derivation for new keystores is several times slower than the legacy
  parameters (tens to a few hundred milliseconds in release builds,
  depending on CPU) — acceptable for the manual-use-only keystore mode.
- A keystore file encrypted with parameters different from the recorded
  ones fails at AEAD decrypt time — unit tests pin that the parameters
  flow into the derivation, that legacy files deserialize with the
  legacy defaults, and that legacy-parameter files keep decrypting.
- Related: the `.env` permission handling from `408dc21`, now shared via
  `write_private()`.
