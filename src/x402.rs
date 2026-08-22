//! X402 payment protocol implementation
//!
//! Creates EIP-3009 transfer authorizations for gasless USDC payments
//! according to the x402 specification.

use anyhow::Result;
use base64::Engine as _;
use ethers::{
    middleware::SignerMiddleware,
    prelude::*,
    types::{Address, U256},
    signers::Signer,
};
use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

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

/// Creates an x402 payment header for EIP-3009 token transfers
///
/// # Arguments
/// * `client` - Ethereum provider with signer
/// * `wallet` - Wallet to sign the authorization
/// * `pay_to` - Recipient address (from 402 response)
/// * `token_addr` - Token contract address (from 402 response)
/// * `amount` - Amount in smallest units (from 402 response)
/// * `token_name` - Token name for EIP-712 domain (optional, defaults to "USD Coin")
/// * `token_version` - Token version for EIP-712 domain (optional, defaults to "2")
/// * `v2` - Emit an x402 v2 payload (PAYMENT-SIGNATURE header) instead of v1 (X-PAYMENT)
/// * `resource_url` - Resource URL embedded in the v2 payload (optional)
/// * `max_timeout_seconds` - maxTimeoutSeconds echoed in v2 accepted requirements
///   (from 402 response `accepts[0].maxTimeoutSeconds`, defaults to 600)
///
/// # Returns
/// Base64-encoded X-PAYMENT (v1) or PAYMENT-SIGNATURE (v2) header value
#[allow(clippy::too_many_arguments)]
pub async fn create_payment(
    client: std::sync::Arc<
        SignerMiddleware<
            Provider<ethers::providers::Http>,
            Wallet<ethers::core::k256::ecdsa::SigningKey>,
        >,
    >,
    wallet: &Wallet<ethers::core::k256::ecdsa::SigningKey>,
    pay_to: &str,
    token_addr: &str,
    amount: &str,
    token_name: Option<&str>,
    token_version: Option<&str>,
    v2: bool,
    resource_url: Option<&str>,
    max_timeout_seconds: Option<u64>,
    // accepted_json: Full `accepts[0]` object from the 402 response (JSON).
    // When given, it is echoed VERBATIM as `accepted` — the only fully
    // provider-agnostic way, since servers deepEqual the echo (including
    // custom extra fields like Exa's breakdown/totalUsd/acceptId).
    accepted_json: Option<&str>,
) -> Result<String> {
    // Chain ID from the provider (validates RPC matches config in http_provider)
    let chain_id = client.get_chainid().await?.as_u64();

    // Get current network name from config (for v1 envelope / CAIP-2 mapping)
    let cfg = crate::evm::load_network().await?;
    let network = cfg.network.clone();

    build_payment(
        wallet,
        chain_id,
        &network,
        pay_to,
        token_addr,
        amount,
        token_name,
        token_version,
        v2,
        resource_url,
        max_timeout_seconds,
        accepted_json,
    )
    .await
}

/// Builds and signs an x402 payment header without any network access
/// (chain_id and network are passed in). This is the fully testable core;
/// `create_payment` is only a thin RPC/config wrapper around it.
#[allow(clippy::too_many_arguments)]
pub async fn build_payment(
    wallet: &Wallet<ethers::core::k256::ecdsa::SigningKey>,
    chain_id: u64,
    network: &str,
    pay_to: &str,
    token_addr: &str,
    amount: &str,
    token_name: Option<&str>,
    token_version: Option<&str>,
    v2: bool,
    resource_url: Option<&str>,
    max_timeout_seconds: Option<u64>,
    accepted_json: Option<&str>,
) -> Result<String> {
    // Parse addresses and amount
    let payer = wallet.address();
    let pay_to_addr: Address = pay_to.parse()?;
    let token: Address = token_addr.parse()?;
    let value: U256 = U256::from_dec_str(amount)?;

    // EIP-712 domain parameters (must match token contract)
    let token_name = token_name.unwrap_or("USD Coin");
    let token_version = token_version.unwrap_or("2");

    // Authorization validity window. validAfter MUST be 0 (not "now"):
    // facilitators reject validAfter > now (ErrValidAfterInFuture), and any
    // clock skew between signer and facilitator breaks "now". 0 is what the
    // official client signs. validBefore bounds the window anyway.
    let timeout = max_timeout_seconds.unwrap_or(600);
    let valid_after = 0u64;
    let valid_before = unix_time() + timeout;

    // Generate random nonce
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);

    // Build EIP-712 typed data structure
    let td_json = serde_json::json!({
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
      "primaryType":"TransferWithAuthorization",
      "domain":{
        "name": token_name,
        "version": token_version,
        "chainId": chain_id,
        "verifyingContract": format!("{:#x}", token)
      },
      "message":{
        "from": format!("{:#x}", payer),
        "to": format!("{:#x}", pay_to_addr),
        "value": value.to_string(),
        "validAfter": valid_after.to_string(),
        "validBefore": valid_before.to_string(),
        "nonce": format!("0x{}", hex::encode(nonce))
      }
    });

    // Sign the typed data
    let typed: ethers::types::transaction::eip712::TypedData = serde_json::from_value(td_json)?;
    let sig = wallet.sign_typed_data(&typed).await?;

    // Combine signature components (r, s, v) into single hex string
    let combined_sig = format!("0x{:064x}{:064x}{:02x}", sig.r, sig.s, sig.v);

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
    let payment_header = if v2 {
        // v2: CAIP-2 network, resource info and echoed payment requirements
        serde_json::to_value(PaymentPayloadV2 {
            x402Version: 2,
            resource: resource_url.map(|url| ResourceInfo {
                url: url.to_string(),
                description: None,
                mimeType: None,
            }),
            accepted: if let Some(json) = accepted_json {
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
                    asset: token_addr.trim().to_string(),
                    payTo: pay_to.trim().to_string(),
                    maxTimeoutSeconds: max_timeout_seconds.unwrap_or(600),
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
fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_secs()
}