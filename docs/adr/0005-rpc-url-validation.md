# ADR-0005: Validate custom RPC endpoint URLs

- **Status:** Accepted
- **Date:** 2026-08-26
- **Scope:** RPC URL handling in `src/evm.rs` (`save_network`,
  `rpc_url_and_expected_chain`)

## Context

Security review finding (error): `config-set --rpc` persisted an arbitrary
URL into `~/.x402wallet/config.json` and every later command dialed it
verbatim (`connect_http` in both provider builders). The URL is the sink of
all wallet traffic — balance queries, chain-id checks, signed transactions,
x402 payment preparation — so a cleartext or attacker-repointed endpoint
exposes everything the wallet sends. The constant `default_rpc_map()` URLs
are not affected (compile-time https constants); only the user-supplied
config path is.

## Decision

1. A custom RPC URL must be `https` — validated at **ingress**
   (`save_network`, before persisting) and at **egress**
   (`rpc_url_and_expected_chain`, before dialing; covers pre-existing and
   hand-edited config files).
2. `http` is allowed only for loopback hosts (`127.0.0.1`, `[::1]`,
   `localhost`, `*.localhost`) — local dev nodes such as anvil.
3. Any other scheme (`ws`, `ftp`, …) is rejected; `connect_http` is
   http(s)-only anyway, but the check now fails with a clear message
   instead of an opaque transport error.

## Alternatives rejected

- Allowlist of well-known RPC hosts — rejected: custom/self-hosted RPCs are
  a supported use case; an allowlist would break them.
- Blocking private IP ranges entirely — rejected: self-hosted nodes behind
  VPN/LAN over https are legitimate; the threat here is cleartext and
  scheme confusion, not LAN reachability (the config file is local and
  owner-controlled).
- Validating only at ingress — rejected: config files written before this
  check (or edited by hand) would bypass it.

## Consequences

- Existing configs with non-loopback `http://` RPCs now fail fast with a
  clear error until switched to https (or moved to localhost). This is the
  intended behavior change.
- Unit tests pin: https accepted, http loopback accepted, http elsewhere
  and non-http schemes rejected, and every `default_rpc_map()` entry stays
  valid.
- Redirects are not restricted by this ADR (reqwest default policy applies)
  — tracked as a separate hardening item.
