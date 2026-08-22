# ADR-0003: Polygon default RPC is publicnode

- **Status:** Accepted
- **Date:** 2026-08-22
- **Scope:** `src/evm.rs` (`default_rpc_map`), network docs

## Context

Polygon support (commit `d5cf12c`, chain 137 / `eip155:137`) needs a
default RPC that works without registration or API key. The de-facto
"public" endpoint `polygon-rpc.com` **rejects anonymous requests**
("API key disabled") — verified live — and is therefore unusable as a
shipped default.

## Decision

The default Polygon RPC is `https://polygon-bor-rpc.publicnode.com`
(free, no key). Users can override it with `config-set --rpc`.

Also documented (README, wallet.md, llms.txt): x402 on Polygon uses the
EIP-3009 path, which requires **native Circle USDC**
(`0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359`). Bridged **USDC.e**
(`0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174`) has no EIP-3009 support
and cannot be used with this wallet.

## Alternatives rejected

- `polygon-rpc.com` — rejects anonymous requests (verified live).
- Keyed providers (Alchemy, Infura, …) — not shippable as an anonymous
  default.

## Consequences

- New networks must verify their default RPC answers anonymous requests
  before shipping (also encoded in AGENTS.md's network checklist).
- If publicnode rate-limits or degrades, the answer is a user-supplied
  `--rpc`, not a silent provider switch.
