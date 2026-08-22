# ADR-0001: Echo v2 payment requirements verbatim

- **Status:** Accepted
- **Date:** 2026-08-22
- **Scope:** `src/x402.rs` (v2 payment payload, `--accepted` flag)

## Context

x402 v2 servers validate the `accepted` object echoed back in the
`PAYMENT-SIGNATURE` header with a **case-sensitive deepEqual** against the
requirements from the 402 `PAYMENT-REQUIRED` header. Two findings from live
testing on 2026-08-22 (against parallelmpp.dev and Exa):

1. The initial implementation formatted `asset`/`payTo` with
   `format!("{:#x}")`, which lowercases addresses. The EIP-712 signature
   itself is case-insensitive (addresses are parsed to bytes), but the echo
   is not — payments were **silently rejected**.
2. Some providers attach custom extra fields to `accepts[0]` (Exa:
   `breakdown`, `totalUsd`, `acceptId`). A hand-built echo that omits them
   fails the same deepEqual.

Fixed in commits `95e3375` (verbatim `asset`/`payTo`) and `69d3b9c`
(`--accepted` passthrough).

## Decision

1. `asset` and `payTo` are echoed **verbatim** from the CLI arguments,
   preserving checksummed case. Never re-format addresses for the echo.
2. `create-payment --accepted <json>` passes the full `accepts[0]` object
   from the 402 response through verbatim as `accepted`. This is the robust
   path for providers with extra fields; the hand-built echo remains the
   fallback when no JSON is supplied.

## Alternatives rejected

- Formatting addresses with `{:#x}` for symmetry with the signing side —
  lowercases checksummed addresses, breaks the server-side deepEqual.
- Normalizing to a whitelist of known fields when building the echo —
  drops provider-specific extras; the set of extra fields is open.

## Consequences

- Regression-tested in `tests/x402_regression.rs`:
  `v2_echoes_asset_and_payto_verbatim_checksummed`,
  `v2_accepted_json_is_echoed_verbatim`.
- General rule for any future x402 version: echo requirements **as
  received**, never re-serialize with formatting.
- Related: [ADR-0002](0002-validafter-zero.md).
