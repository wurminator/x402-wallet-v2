# AGENTS.md

Context for AI coding agents working in this repository. Human-facing docs:
[README.md](README.md). Agent-facing usage instructions for the built binary:
[wallet.md](wallet.md) and [llms.txt](llms.txt). Protocol decisions and their
rationale live in [docs/adr/](docs/adr/) — read the relevant ADR before
touching that area.

## What this repo is

`x402-wallet` is a Rust CLI wallet for the [x402](https://x402.org) HTTP
payment protocol ("exact" scheme, EIP-3009 `TransferWithAuthorization`
signatures). It supports x402 **v1** (`X-PAYMENT` header, default) and
**v2** (`PAYMENT-SIGNATURE` header, CAIP-2 network ids, resource/accepted
envelope, opt-in via `--v2`). Fork of 0xKoda/x402-wallet, published as
[wurminator/x402-wallet-v2](https://github.com/wurminator/x402-wallet-v2).

## Build, test, verify

- `cargo build --release` — binary at `target/release/x402-wallet`. Pure
  rustls build (reqwest `default-features = false`): no system TLS deps.
- `cargo test` — 7 offline regression tests in `tests/x402_regression.rs`
  (no RPC, no network, no funds). Run after any change to `src/x402.rs` or
  `src/evm.rs`.
- `cargo fmt` / `cargo clippy` — CI advisors (`.github/workflows/ci.yml`);
  keep new code clean. CI also smoke-builds on Windows.
- The payment core is `build_payment()` in `src/x402.rs`, exposed as a
  library via `src/lib.rs` so tests can reach it without a network. Keep it
  network-free.

## Code layout

| Path | Contents |
|---|---|
| `src/main.rs` | clap CLI surface (flags, stdout output) |
| `src/x402.rs` | v1/v2 payment envelopes, EIP-712/EIP-3009 signing, `build_payment()` |
| `src/evm.rs` | networks: default RPCs, chain ids, CAIP-2 mapping |
| `src/store.rs` | key storage (`.env` plaintext or encrypted keystore) |
| `tests/x402_regression.rs` | offline regression tests |
| `README.md`, `wallet.md`, `llms.txt`, `resource-list.md` | docs (see below) |

## Conventions

- **Always `git pull` before starting work** — the owner pushes live-test
  commits from other contexts.
- Commit and push only on explicit request of the owner.
- Public docs in English; README keeps a German `Kurzübersicht` section.
- Adding a network requires entries in **four places** in `src/evm.rs`
  (`default_rpc_map`, `save_network` normalization incl. aliases,
  `chain_id`, `caip2_for_network`) plus updates to all three docs:
  README.md, wallet.md, llms.txt. Verify the default RPC actually answers
  anonymous requests before shipping it.
- reqwest stays `default-features = false` with `rustls-tls` only — do not
  reintroduce native-tls/openssl-sys.

## Hard rules (each backed by an ADR)

1. **Echo payment requirements verbatim.** Never re-format addresses for
   the echo — `{:#x}` lowercases checksummed addresses and servers
   deepEqual the echo case-sensitively → silent payment rejections.
   See [ADR-0001](docs/adr/0001-verbatim-accepted-echo.md).
2. **Sign `validAfter = 0`**, never "now". Facilitators reject
   `validAfter > now` on any clock skew (`ErrValidAfterInFuture`); the
   window is bounded by `validBefore` alone.
   See [ADR-0002](docs/adr/0002-validafter-zero.md).
3. **`polygon-rpc.com` must not be the default Polygon RPC** — it rejects
   anonymous requests. Default is `https://polygon-bor-rpc.publicnode.com`.
   See [ADR-0003](docs/adr/0003-polygon-default-rpc.md).

## Don'ts

- No secrets, private keys or passphrases in commits, docs or agent memory.
- Don't let the three docs drift: a new flag or network goes into README.md,
  wallet.md and llms.txt together.
- Don't break the offline property of the test suite (no RPC calls,
  no funds).
