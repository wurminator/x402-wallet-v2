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

use alloy::primitives::{Address, Signature, B256, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol_types::{Eip712Domain, SolStruct};
use base64::Engine as _;
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

fn wallet() -> PrivateKeySigner {
    PrivateKeySigner::from_bytes(&B256::from_slice(
        &hex::decode(TEST_KEY.trim_start_matches("0x")).unwrap(),
    ))
    .unwrap()
}

/// Reconstructs the EIP-712 domain the signer used (must mirror x402.rs)
fn eip712_domain(
    name: &str,
    version: &str,
    chain_id: u64,
    verifying_contract: &str,
) -> Eip712Domain {
    Eip712Domain::new(
        Some(name.to_string().into()),
        Some(version.to_string().into()),
        Some(U256::from(chain_id)),
        Some(verifying_contract.parse::<Address>().unwrap()),
        None,
    )
}

/// Recovers the signer address from an EIP-712 signature over the given
/// authorization fields (mirrors the facilitator's on-chain verification)
fn recover_signer(domain: &Eip712Domain, auth: &serde_json::Value, sig_hex: &str) -> Address {
    use x402::TransferWithAuthorization;
    let typed = TransferWithAuthorization {
        from: auth["from"].as_str().unwrap().parse().unwrap(),
        to: auth["to"].as_str().unwrap().parse().unwrap(),
        value: auth["value"].as_str().unwrap().parse().unwrap(),
        validAfter: auth["validAfter"].as_str().unwrap().parse().unwrap(),
        validBefore: auth["validBefore"].as_str().unwrap().parse().unwrap(),
        nonce: auth["nonce"].as_str().unwrap().parse().unwrap(),
    };
    let digest = typed.eip712_signing_hash(domain);
    let bytes = hex::decode(sig_hex.trim_start_matches("0x")).unwrap();
    let mut arr = [0u8; 65];
    arr.copy_from_slice(&bytes);
    Signature::from_raw_array(&arr)
        .unwrap()
        .recover_address_from_prehash(&digest)
        .unwrap()
}

/// Builds a v2 header via the offline core and decodes it to JSON.
async fn build_v2(accepted_json: Option<&str>, max_timeout: Option<u64>) -> serde_json::Value {
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: true,
            resource_url: Some("https://api.exa.ai/search"),
            max_timeout_seconds: max_timeout,
            accepted_json,
        },
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
    assert_eq!(
        auth["validAfter"].as_str().unwrap(),
        "0",
        "validAfter must be \"0\" (ErrValidAfterInFuture otherwise)"
    );

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
    assert_eq!(
        p["accepted"], accepts0,
        "accepted must be a verbatim 1:1 echo"
    );
}

// ---------------------------------------------------------------------------
// Signature round-trip: recovered signer must equal wallet address
// (mirrors the EIP-712 verification facilitators perform on-chain)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v2_signature_recovers_to_wallet_address() {
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: true,
            resource_url: Some("https://api.exa.ai/search"),
            max_timeout_seconds: Some(60),
            accepted_json: None,
        },
    )
    .await
    .unwrap();

    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    let auth = &p["payload"]["authorization"];

    // Reconstruct the exact typed data the signer used (defaults: USD Coin / 2)
    let domain = eip712_domain("USD Coin", "2", 8453, USDC_BASE);
    let sig_hex = p["payload"]["signature"].as_str().unwrap();
    let recovered = recover_signer(&domain, auth, sig_hex);

    assert_eq!(
        format!("{:#x}", recovered),
        format!("{:#x}", w.address()),
        "signature must recover to the wallet address"
    );

    // Cross-check authorization fields against inputs
    assert_eq!(auth["value"].as_str().unwrap(), "7000");
    assert_eq!(
        auth["to"].as_str().unwrap().to_lowercase(),
        PAY_TO.to_lowercase()
    );
    assert!(auth["nonce"].as_str().unwrap().starts_with("0x"));
    // Signature must be 132 hex chars (r+s+v)
    assert_eq!(sig_hex.len(), 132);
}

// ---------------------------------------------------------------------------
// v1 envelope shape (unchanged legacy behaviour)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn v1_envelope_uses_plain_network_name() {
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: false,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        },
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
    assert_eq!(
        p["accepted"]["network"], "eip155:8453",
        "CAIP-2 required in v2"
    );
    assert_eq!(p["accepted"]["scheme"], "exact");
    assert_eq!(p["accepted"]["amount"], "7000");
    assert_eq!(p["accepted"]["maxTimeoutSeconds"], 300);
}

// ---------------------------------------------------------------------------
// Edge cases & error paths of build_payment()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rejects_invalid_pay_to_address() {
    // Keine gültige Hex-Adresse → Parse-Fehler statt stiller Fehl-Signatur
    let w = wallet();
    assert!(x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: "not-an-address",
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: false,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        }
    )
    .await
    .is_err());
}

#[tokio::test]
async fn rejects_invalid_token_address() {
    // Asset-Adresse muss parse-bar sein (wird für EIP-712 verifyingContract gebraucht)
    let w = wallet();
    assert!(x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: "0x123",
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: false,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        }
    )
    .await
    .is_err());
}

#[tokio::test]
async fn rejects_non_numeric_amount() {
    // Amount muss Dezimal-String in smallest units sein
    let w = wallet();
    for bad in ["7000.5", "-1", "0x1a", "abc"] {
        assert!(
            x402::build_payment(
                &w,
                8453,
                "base",
                x402::PaymentParams {
                    pay_to: PAY_TO,
                    token_addr: USDC_BASE,
                    amount: bad,
                    token_name: None,
                    token_version: None,
                    v2: false,
                    resource_url: None,
                    max_timeout_seconds: None,
                    accepted_json: None,
                }
            )
            .await
            .is_err(),
            "amount {bad:?} must be rejected"
        );
    }
}

#[tokio::test]
async fn accepts_zero_amount() {
    // "0" ist ein valider U256-Wert —Happy Path an der Grenze (Server lehnen ggf. ab, wir nicht)
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "0",
            token_name: None,
            token_version: None,
            v2: false,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        },
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(
        p["payload"]["authorization"]["value"].as_str().unwrap(),
        "0"
    );
}

#[tokio::test]
async fn accepts_huge_amount_as_u256() {
    // Sechsstelliger Betrag (USDC 6 decimals, max supply ~2^53) muss ohne Überlauf durchgehen
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "1000000000000000000000000000",
            token_name: None,
            token_version: None,
            v2: false,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        },
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(
        p["payload"]["authorization"]["value"].as_str().unwrap(),
        "1000000000000000000000000000"
    );
}

#[tokio::test]
async fn rejects_malformed_accepted_json() {
    // Ungültiges JSON im accepted_json-Passthrough muss fehlschlagen, kein leeres Echo
    let w = wallet();
    assert!(x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: true,
            resource_url: None,
            max_timeout_seconds: Some(300),
            accepted_json: Some("{not json"),
        }
    )
    .await
    .is_err());
}

#[tokio::test]
async fn rejects_v2_with_unknown_network_without_accepted_json() {
    // Ohne accepted_json muss CAIP-2 gemapped werden — unbekanntes Netz → Fehler
    let w = wallet();
    assert!(x402::build_payment(
        &w,
        1,
        "unknownnet",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: true,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        }
    )
    .await
    .is_err());
}

#[tokio::test]
async fn v2_unknown_network_ok_with_accepted_json() {
    // Mit verbatim accepted_json wird kein CAIP-2-Mapping gebraucht → beliebige Netze möglich
    let w = wallet();
    let accepted = serde_json::json!({
        "scheme": "exact", "network": "eip155:9999", "amount": "7000",
        "asset": USDC_BASE, "payTo": PAY_TO, "maxTimeoutSeconds": 60
    });
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: true,
            resource_url: None,
            max_timeout_seconds: Some(60),
            accepted_json: Some(&accepted.to_string()),
        },
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(p["accepted"]["network"], "eip155:9999");
}

#[tokio::test]
async fn v1_unknown_network_passes_through() {
    // v1 nutzt den Netzwerk-Namen unverändert — kein Mapping, kein Fehler
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "unknownnet",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: false,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        },
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(p["network"], "unknownnet");
}

#[tokio::test]
async fn defaults_applied_when_options_none() {
    // Defaults: maxTimeoutSeconds=600, token name "USD Coin", version "2" (idR-712-Domain)
    let before = now();
    let p = build_v2(None, None).await;
    let after = now();

    assert_eq!(p["accepted"]["maxTimeoutSeconds"], 600);
    assert_eq!(p["accepted"]["extra"]["name"], "USD Coin");
    assert_eq!(p["accepted"]["extra"]["version"], "2");

    let vb: u64 = p["payload"]["authorization"]["validBefore"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        vb >= before + 600 && vb <= after + 600,
        "validBefore must default to ~now+600 (was {vb}, window [{before},{after}])"
    );
}

#[tokio::test]
async fn zero_timeout_yields_valid_before_now() {
    // maxTimeoutSeconds=0 → validBefore ≈ jetzt; validAfter bleibt 0 (Fenster kann degenerieren, Protokoll erlaubt es)
    let before = now();
    let p = build_v2(None, Some(0)).await;
    let after = now();
    let vb: u64 = p["payload"]["authorization"]["validBefore"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        vb >= before && vb <= after,
        "validBefore should be ~now, was {vb}"
    );
    assert_eq!(p["accepted"]["maxTimeoutSeconds"], 0);
}

#[tokio::test]
async fn custom_token_name_and_version_flow_into_domain() {
    // Eigene Token-Metadaten müssen in extra UND in die EIP-712-Domain einfließen
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: Some("Euro Coin"),
            token_version: Some("3"),
            v2: false,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        },
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    // v1 hat kein accepted; Domain-Check via Signatur-Recovery mit korrekter Domain
    let auth = &p["payload"]["authorization"];
    let domain = eip712_domain("Euro Coin", "3", 8453, USDC_BASE);
    let recovered = recover_signer(&domain, auth, p["payload"]["signature"].as_str().unwrap());
    assert_eq!(
        format!("{:#x}", recovered),
        format!("{:#x}", w.address()),
        "signature must verify against the custom EIP-712 domain"
    );
}

#[tokio::test]
async fn v2_without_resource_url_omits_resource_field() {
    // resource ist Optioneel (skip_serializing_if) — ohne URL darf kein resource-Key auftauchen
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: true,
            resource_url: None,
            max_timeout_seconds: Some(300),
            accepted_json: None,
        },
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert!(
        p.get("resource").is_none(),
        "resource key must be absent, not null"
    );
}

#[tokio::test]
async fn whitespace_addresses_rejected_at_parse_not_trimmed() {
    // DOKUMENTIERTES IST-VERHALTEN: Address-Parsing läuft VOR dem Echo-Trim,
    // daher wird " 0x..." abgelehnt — das trim() im Echo ist dafür wirkungslos.
    // Wer Leerzeichen tolerieren will, muss vor build_payment normalisieren.
    let w = wallet();
    assert!(x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: &format!(" {} ", PAY_TO),
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: true,
            resource_url: None,
            max_timeout_seconds: Some(300),
            accepted_json: None,
        }
    )
    .await
    .is_err());
}

#[tokio::test]
async fn empty_amount_silently_becomes_zero_value_payment() {
    // DOKUMENTIERTES IST-VERHALTEN (Befund, kein Soll): U256::from_dec_str("")
    // ergibt Ok(0) — ein leerer Amount wird als 0-Zahlung signiert, nicht
    // abgelehnt. Server sollten das fangen; der Client tut es aktuell nicht.
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "",
            token_name: None,
            token_version: None,
            v2: false,
            resource_url: None,
            max_timeout_seconds: None,
            accepted_json: None,
        },
    )
    .await
    .unwrap();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(
        p["payload"]["authorization"]["value"].as_str().unwrap(),
        "0"
    );
}

#[tokio::test]
async fn output_is_valid_base64_and_json() {
    // Rückgabe ist base64(STANDARD-Alphabet) über validem JSON —Vertrag für den HTTP-Header
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: true,
            resource_url: Some("https://example.com"),
            max_timeout_seconds: Some(60),
            accepted_json: None,
        },
    )
    .await
    .unwrap();
    assert!(!b64.contains('/') || b64.len() % 4 == 0); // kein Padding-Fehler
    let raw = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .unwrap();
    let _: serde_json::Value = serde_json::from_slice(&raw).unwrap();
}

#[tokio::test]
async fn nonces_differ_between_calls() {
    // Nonce muss kryptografisch frisch sein — zwei Zahlungen dürfen nicht
    // kollidieren (blockieren sonst gegenseitig via EIP-3009 nonce-Verbrauch)
    let w = wallet();
    let build = || async {
        let b64 = x402::build_payment(
            &w,
            8453,
            "base",
            x402::PaymentParams {
                pay_to: PAY_TO,
                token_addr: USDC_BASE,
                amount: "7000",
                token_name: None,
                token_version: None,
                v2: false,
                resource_url: None,
                max_timeout_seconds: None,
                accepted_json: None,
            },
        )
        .await
        .unwrap();
        let raw = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        p["payload"]["authorization"]["nonce"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let n1 = build().await;
    let n2 = build().await;
    assert_ne!(n1, n2, "two payments must never share a nonce");
}

// ---------------------------------------------------------------------------
// --max-timeout-seconds semantics: bounds the window in BOTH versions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn max_timeout_seconds_bounds_v1_validity_window_too() {
    // Docs used to claim "--max-timeout-seconds (v2 only)" — but the flag
    // bounds validBefore in BOTH protocol versions (maxTimeoutSeconds is a
    // v1 PaymentRequirements field as well); only the echo into the v2
    // `accepted` object is v2-specific.
    let before = now();
    let w = wallet();
    let b64 = x402::build_payment(
        &w,
        8453,
        "base",
        x402::PaymentParams {
            pay_to: PAY_TO,
            token_addr: USDC_BASE,
            amount: "7000",
            token_name: None,
            token_version: None,
            v2: false,
            resource_url: None,
            max_timeout_seconds: Some(300),
            accepted_json: None,
        },
    )
    .await
    .unwrap();
    let after = now();
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    let p: serde_json::Value = serde_json::from_slice(&raw).unwrap();

    let vb: u64 = p["payload"]["authorization"]["validBefore"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        vb >= before + 300 && vb <= after + 300,
        "v1 validBefore must follow --max-timeout-seconds (was {vb}, window [{before},{after}])"
    );
    // ... while the accepted echo remains v2-only
    assert!(p.get("accepted").is_none());
}

// ---------------------------------------------------------------------------
// CAIP-2 mapping
// ---------------------------------------------------------------------------

#[test]
fn caip2_mapping() {
    assert_eq!(evm::caip2_for_network("base").unwrap(), "eip155:8453");
    assert_eq!(evm::caip2_for_network("ethereum").unwrap(), "eip155:1");
    assert_eq!(
        evm::caip2_for_network("base-sepolia").unwrap(),
        "eip155:84532"
    );
    assert!(evm::caip2_for_network("solana").is_err());
}
