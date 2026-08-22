# ADR-0002: Sign validAfter = 0

- **Status:** Accepted
- **Date:** 2026-08-22
- **Scope:** EIP-3009 `TransferWithAuthorization` parameters in `src/x402.rs`

## Context

Signing `validAfter` as "now" (wall-clock of the signing machine) caused
live rejections with `ErrValidAfterInFuture`: facilitators reject any
payment whose `validAfter` is greater than their own `now`, so any clock
skew between signer and facilitator — even seconds — kills the payment.
The official x402 client signs 0. Fixed in commit `95e3375`.

## Decision

`validAfter` is always signed as **0** (valid immediately). The validity
window is bounded solely by `validBefore`, derived from
`--max-timeout-seconds` (default 600 s), matching `maxTimeoutSeconds` in
the v2 accepted requirements.

## Alternatives rejected

- `validAfter = now` — rejected after live rejections on clock skew.
- `validAfter = now - safety margin` — trades one failure mode for
  guesswork about acceptable skew; unnecessary because `validBefore`
  already bounds the window on its own.

## Consequences

- Regression-tested in `tests/x402_regression.rs`:
  `v2_signs_valid_after_zero_and_window_from_now`.
- Clock skew between the wallet machine and the facilitator no longer
  matters for the window start; only `validBefore` must lie in the
  facilitator's future.
- Related: [ADR-0001](0001-verbatim-accepted-echo.md).
