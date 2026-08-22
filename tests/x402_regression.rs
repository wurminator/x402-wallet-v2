//! Regression & unit tests for the x402 payment core.
//!
//! All tests run fully offline via `build_payment` (no RPC, no config file,
//! no real funds). They encode the three live-verified provider rules that
//! the wallet MUST uphold — each one was broken at some point and caused
//! silent 402 rejections against real providers (Parallel, Exa, 2026-08-22):
//!
//!   1. `accepted.asset` / `accepted.payTo` must be echoed VERBATIM
//!      (checksummed, case-sensitive deepEqual on the server side).
//!   2. `validAfter` must be "0", never "now" (ErrValidAfterInFuture on
//!      any clock skew).
//!   3. `accepted` must be the COMPLETE accepts[0] object when provided
//!      (Exa requires breakdown/totalUsd/acceptId to be echoed).

use base64::Engine as _;
use ethers::signers::{LocalWallet, Signer};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use x402_wallet::{evm, x402};

// Deterministic throwaway key (never funded, well-known test vector style)
const TEST_KEY: &str = "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d";

const USDC_BASE: &str = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913"; // checksummed!
const PAY_TO: &str = "0x6d6E695b09861467c7d462f5AAF31cF3540B9192"; // checksummed!

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn wallet() -> LocalWallet {
    LocalWallet::from_bytes(&hex::decode(TEST_KEY.trim_start_matches("0x")).unwrap())
        .unwrap()
        .with_chain_id(8453u64)
}

/// Builds a v2 header via the offline core and decodes it to JSON.
async fn build_v2(
    accepted_json: Option<&str>,
    max_timeout: Option<u64>,
) -> serde_json::Value {
    let w = wallet().await;
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        PAY_TO,
        USDC_BASE,
        "7000",
        None,
        None,
        true,
        Some("https://api.exa.ai/search"),
        max_timeout,
        accepted_json,
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    serde_json::from_slice(&raw).unwrap()
}

// ---------------------------------------------------------------------------
// Bug 1 regression: verbatim (checksummed) address echo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v2_echoes_asset_and_payto_verbatim_checksummed() {
    let p = build_v2(None, Some(300)).await;
    let acc = &p["accepted"];
    // Exact string equality — any lowercasing/re-formatting must fail here
    assert_eq!(acc["asset"].as_str().unwrap(), USDC_BASE);
    assert_eq!(acc["payTo"].as_str().unwrap(), PAY_TO);
}

// ---------------------------------------------------------------------------
// Bug 2 regression: validAfter == 0, validBefore bounds the window
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v2_signs_valid_after_zero_and_window_from_now() {
    let before = now();
    let p = build_v2(None, Some(300)).await;
    let after = now();

    let auth = &p["payload"]["authorization"];
    assert_eq!(auth["validAfter"].as_str().unwrap(), "0", "validAfter must be \"0\" (ErrValidAfterInFuture otherwise)");

    let vb: u64 = auth["validBefore"].as_str().unwrap().parse().unwrap();
    assert!(
        vb >= before + 300 && vb <= after + 300,
        "validBefore must be ~now+maxTimeoutSeconds (was {vb}, window [{before},{after}])"
    );
}

// ---------------------------------------------------------------------------
// Bug 3 regression: full accepts[0] passthrough (provider extra fields)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v2_accepted_json_is_echoed_verbatim() {
    // Exa-style requirements with custom extra fields
    let accepts0 = serde_json::json!({
        "scheme": "exact",
        "network": "eip155:8453",
        "amount": "7000",
        "asset": USDC_BASE,
        "payTo": PAY_TO,
        "maxTimeoutSeconds": 60,
        "extra": {
            "name": "USD Coin",
            "version": "2",
            "breakdown": { "search": 0.007 },
            "totalUsd": 0.007,
            "acceptId": "legacy"
        }
    });
    let p = build_v2(Some(&accepts0.to_string()), Some(60)).await;
    // Deep equality: the echo must be IDENTICAL to the input, extra fields included
    assert_eq!(p["accepted"], accepts0, "accepted must be a verbatim 1:1 echo");
}

// ---------------------------------------------------------------------------
// Signature round-trip: recovered signer must equal wallet address
// (mirrors the EIP-712 verification facilitators perform on-chain)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v2_signature_recovers_to_wallet_address() {
    use ethers::types::transaction::eip712::TypedData;

    let w = wallet().await;
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        PAY_TO,
        USDC_BASE,
        "7000",
        None,
        None,
        true,
        Some("https://api.exa.ai/search"),
        Some(60),
        None,
    )
    .await
    .unwrap();

    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let auth = &p["payload"]["authorization"];

    // Reconstruct the exact typed data the signer used
    let td = serde_json::json!({
        "types": {
            "EIP712Domain": [
                {"name":"name","type":"string"},
                {"name":"version","type":"string"},
                {"name":"chainId","type":"uint256"},
                {"name":"verifyingContract","type":"address"}
            ],
            "TransferWithAuthorization": [
                {"name":"from","type":"address"},
                {"name":"to","type":"address"},
                {"name":"value","type":"uint256"},
                {"name":"validAfter","type":"uint256"},
                {"name":"validBefore","type":"uint256"},
                {"name":"nonce","type":"bytes32"}
            ]
        },
        "primaryType": "TransferWithAuthorization",
        "domain": {
            "name": "USD Coin",
            "version": "2",
            "chainId": 8453,
            "verifyingContract": USDC_BASE
        },
        "message": {
            "from": auth["from"],
            "to": auth["to"],
            "value": auth["value"],
            "validAfter": auth["validAfter"],
            "validBefore": auth["validBefore"],
            "nonce": auth["nonce"]
        }
    });
    let typed: TypedData = serde_json::from_value(td).unwrap();

    // Recover the signer from the signature
    let sig_hex = p["payload"]["signature"].as_str().unwrap();
    let sig = ethers::types::Signature::from_str(sig_hex).unwrap();
    let recovered = sig.recover_typed_data(&typed).unwrap();

    assert_eq!(
        format!("{:#x}", recovered),
        format!("{:#x}", w.address()),
        "signature must recover to the wallet address"
    );

    // Cross-check authorization fields against inputs
    assert_eq!(auth["value"].as_str().unwrap(), "7000");
    assert_eq!(auth["to"].as_str().unwrap().to_lowercase(), PAY_TO.to_lowercase());
    assert!(auth["nonce"].as_str().unwrap().starts_with("0x"));
    // Signature must be 132 hex chars (r+s+v)
    assert_eq!(sig_hex.len(), 132);
}

// ---------------------------------------------------------------------------
// v1 envelope shape (unchanged legacy behaviour)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v1_envelope_uses_plain_network_name() {
    let w = wallet().await;
    let b64 = x402::build_payment(
        &w, 8453, "base", PAY_TO, USDC_BASE, "7000",
        None, None, false, None, None, None,
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();

    assert_eq!(p["x402Version"], 1);
    assert_eq!(p["scheme"], "exact");
    assert_eq!(p["network"], "base");
    assert!(p["payload"]["authorization"].is_object());
}

// ---------------------------------------------------------------------------
// Structural invariants of the v2 envelope
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v2_envelope_shape() {
    let p = build_v2(None, Some(300)).await;
    assert_eq!(p["x402Version"], 2);
    assert_eq!(p["resource"]["url"], "https://api.exa.ai/search");
    assert_eq!(p["accepted"]["network"], "eip155:8453", "CAIP-2 required in v2");
    assert_eq!(p["accepted"]["scheme"], "exact");
    assert_eq!(p["accepted"]["amount"], "7000");
    assert_eq!(p["accepted"]["maxTimeoutSeconds"], 300);
}

// ---------------------------------------------------------------------------
// CAIP-2 mapping
// ---------------------------------------------------------------------------

#[test]
fn caip2_mapping() {
    assert_eq!(evm::caip2_for_network("base").unwrap(), "eip155:8453");
    assert_eq!(evm::caip2_for_network("ethereum").unwrap(), "eip155:1");
    assert_eq!(evm::caip2_for_network("base-sepolia").unwrap(), "eip155:84532");
    assert!(evm::caip2_for_network("solana").is_err());
}
