//! X402 payment protocol implementation
//!
//! Creates EIP-3009 transfer authorizations for gasless USDC payments
//! according to the x402 specification.

use alloy::primitives::{Address, B256, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::Signer;
use alloy::sol_types::Eip712Domain;
use anyhow::Result;
use base64::Engine as _;
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

// EIP-3009 TransferWithAuthorization message, signed via EIP-712.
// Field names must match the token contract's type hash exactly.
mod eip3009 {
    alloy::sol! {
        #[derive(Debug)]
        struct TransferWithAuthorization {
            address from;
            address to;
            uint256 value;
            uint256 validAfter;
            uint256 validBefore;
            bytes32 nonce;
        }
    }
}
pub use eip3009::TransferWithAuthorization;

pub const DEFAULT_TOKEN_NAME: &str = "USD Coin";
pub const DEFAULT_TOKEN_VERSION: &str = "2";

/// X402 v1 payment header structure (X-PAYMENT)
#[derive(Debug, Serialize)]
struct PaymentPayload {
    x402Version: u32,
    scheme: String,
    network: String,
    payload: serde_json::Value,
}

/// X402 v2 payment header structure (PAYMENT-SIGNATURE)
#[derive(Debug, Serialize)]
struct PaymentPayloadV2 {
    x402Version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<ResourceInfo>,
    /// Accepted payment requirements, echoed verbatim from the 402 response
    /// (`accepts[0]`). Some providers add extra fields (e.g. Exa: breakdown,
    /// totalUsd, acceptId) that servers deepEqual against the echo.
    accepted: serde_json::Value,
    payload: serde_json::Value,
}

/// X402 v2 resource description (echoed in the payment payload)
#[derive(Debug, Serialize)]
struct ResourceInfo {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mimeType: Option<String>,
}

/// X402 v2 accepted payment requirements (echoed from the 402 response)
#[derive(Debug, Serialize)]
struct AcceptedRequirements {
    scheme: String,
    /// CAIP-2 network identifier (e.g. "eip155:8453")
    network: String,
    amount: String,
    asset: String,
    payTo: String,
    maxTimeoutSeconds: u64,
    extra: TokenExtra,
}

/// EIP-712 domain parameters of the token (required for eip3009)
#[derive(Debug, Serialize)]
struct TokenExtra {
    name: String,
    version: String,
}

/// EIP-3009 exact payment payload
#[derive(Serialize)]
struct ExactEvm {
    signature: String,
    authorization: Authorization,
}

/// EIP-3009 TransferWithAuthorization parameters
#[derive(Serialize)]
struct Authorization {
    from: String,
    to: String,
    value: String,
    validAfter: String,
    validBefore: String,
    nonce: String,
}

#[derive(Debug, Clone, Default)]
pub struct PaymentParams<'a> {
    pub pay_to: &'a str,
    pub token_addr: &'a str,
    pub amount: &'a str,
    pub token_name: Option<&'a str>,
    pub token_version: Option<&'a str>,
    pub v2: bool,
    pub resource_url: Option<&'a str>,
    pub max_timeout_seconds: Option<u64>,
    pub accepted_json: Option<&'a str>,
}

/// Creates an x402 payment header for EIP-3009 token transfers
///
/// # Arguments
/// * `chain_id` - Chain ID of the configured network (caller must validate
///   it against the RPC — `evm::http_provider()` does that)
/// * `wallet` - Wallet to sign the authorization
/// * `pay_to` - Recipient address (from 402 response)
/// * `token_addr` - Token contract address (from 402 response)
/// * `amount` - Amount in smallest units (from 402 response)
/// * `token_name` - Token name for EIP-712 domain (optional, defaults to `DEFAULT_TOKEN_NAME`)
/// * `token_version` - Token version for EIP-712 domain (optional, defaults to `DEFAULT_TOKEN_VERSION`)
/// * `v2` - Emit an x402 v2 payload (PAYMENT-SIGNATURE header) instead of v1 (X-PAYMENT)
/// * `resource_url` - Resource URL embedded in the v2 payload (optional)
/// * `max_timeout_seconds` - Payment validity window in seconds: bounds
///   `validBefore` in both v1 and v2; in v2 additionally echoed as
///   `maxTimeoutSeconds` in the accepted requirements (defaults to 600)
///
/// # Returns
/// Base64-encoded X-PAYMENT (v1) or PAYMENT-SIGNATURE (v2) header value
pub async fn create_payment(
    chain_id: u64,
    wallet: &PrivateKeySigner,
    params: PaymentParams<'_>,
) -> Result<String> {
    // Get current network name from config (for v1 envelope / CAIP-2 mapping)
    let cfg = crate::evm::load_network().await?;
    let network = cfg.network.clone();

    build_payment(wallet, chain_id, &network, params).await
}

/// Builds and signs an x402 payment header without any network access
/// (chain_id and network are passed in). This is the fully testable core;
/// `create_payment` is only a thin RPC/config wrapper around it.
pub async fn build_payment(
    wallet: &PrivateKeySigner,
    chain_id: u64,
    network: &str,
    params: PaymentParams<'_>,
) -> Result<String> {
    // Parse addresses and params.amount
    let payer = wallet.address();
    let pay_to_addr: Address = params.pay_to.parse()?;
    let token: Address = params.token_addr.parse()?;
    // Decimal-only parsing: alloy's FromStr would accept 0x-prefixed values,
    // which the CLI contract rejects. Empty string keeps the documented
    // legacy behavior of signing a zero-value payment.
    let value = if params.amount.is_empty() {
        U256::ZERO
    } else {
        U256::from_str_radix(params.amount, 10)?
    };

    // EIP-712 domain parameters (must match token contract)
    let token_name = params.token_name.unwrap_or("USD Coin");
    let token_version = params.token_version.unwrap_or("2");

    // Authorization validity window. validAfter MUST be 0 (not "now"):
    // facilitators reject validAfter > now (ErrValidAfterInFuture), and any
    // clock skew between signer and facilitator breaks "now". 0 is what the
    // official client signs. validBefore bounds the window anyway.
    let timeout = params.max_timeout_seconds.unwrap_or(600);
    let valid_after = 0u64;
    let valid_before = unix_time()? + timeout;

    // Generate random nonce
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);

    // EIP-712 typed data (EIP-3009 TransferWithAuthorization)
    let domain = Eip712Domain::new(
        Some(token_name.to_string().into()),
        Some(token_version.to_string().into()),
        Some(U256::from(chain_id)),
        Some(token),
        None,
    );
    let typed = TransferWithAuthorization {
        from: payer,
        to: pay_to_addr,
        value,
        validAfter: U256::from(valid_after),
        validBefore: U256::from(valid_before),
        nonce: B256::from(nonce),
    };
    let sig = wallet.sign_typed_data(&typed, &domain).await?;

    // Combine signature components (r, s, v) into single hex string —
    // as_bytes() is exactly r || s || v (65 bytes, v = 27 + y_parity)
    let combined_sig = format!("0x{}", hex::encode(sig.as_bytes()));

    // Build authorization payload
    let payload = ExactEvm {
        signature: combined_sig,
        authorization: Authorization {
            from: format!("{:#x}", payer),
            to: format!("{:#x}", pay_to_addr),
            value: value.to_string(),
            validAfter: valid_after.to_string(),
            validBefore: valid_before.to_string(),
            nonce: format!("0x{}", hex::encode(nonce)),
        },
    };

    // Build x402 payment header
    let payment_header = if params.v2 {
        // params.v2: CAIP-2 network, resource info and echoed payment requirements
        serde_json::to_value(PaymentPayloadV2 {
            x402Version: 2,
            resource: params.resource_url.map(|url| ResourceInfo {
                url: url.to_string(),
                description: None,
                mimeType: None,
            }),
            accepted: if let Some(json) = params.accepted_json {
                // Verbatim echo: parse accepts[0] JSON and pass it through 1:1
                serde_json::from_str::<serde_json::Value>(json)?
            } else {
                serde_json::to_value(AcceptedRequirements {
                    scheme: "exact".to_string(),
                    network: crate::evm::caip2_for_network(&network)?,
                    amount: value.to_string(),
                    // Echo asset/payTo VERBATIM (checksummed) — servers validate the
                    // echo with a case-sensitive deepEqual against the 402 response;
                    // re-formatting via {:#x} lowercases them and gets rejected.
                    asset: params.token_addr.trim().to_string(),
                    payTo: params.pay_to.trim().to_string(),
                    maxTimeoutSeconds: params.max_timeout_seconds.unwrap_or(600),
                    extra: TokenExtra {
                        name: token_name.to_string(),
                        version: token_version.to_string(),
                    },
                })?
            },
            payload: serde_json::to_value(payload)?,
        })?
    } else {
        // v1: legacy envelope with plain network name
        serde_json::to_value(PaymentPayload {
            x402Version: 1,
            scheme: "exact".to_string(),
            network: network.to_string(),
            payload: serde_json::to_value(payload)?,
        })?
    };

    // Encode as base64
    let b64 =
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&payment_header)?);
    Ok(b64)
}

/// Returns current Unix timestamp in seconds
fn unix_time() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}
