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

/// X402 payment header structure
#[derive(Debug, Serialize)]
struct PaymentPayload {
    x402Version: u32,
    scheme: String,
    network: String,
    payload: serde_json::Value,
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
///
/// # Returns
/// Base64-encoded X-PAYMENT header value
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
) -> Result<String> {
    // Parse addresses and amount
    let chain_id = client.get_chainid().await?.as_u64();
    let payer = wallet.address();
    let pay_to: Address = pay_to.parse()?;
    let token: Address = token_addr.parse()?;
    let value: U256 = U256::from_dec_str(amount)?;

    // EIP-712 domain parameters (must match token contract)
    let token_name = token_name.unwrap_or("USD Coin");
    let token_version = token_version.unwrap_or("2");

    // Authorization validity window (current time + 10 minutes)
    let valid_after = unix_time();
    let valid_before = valid_after + 600;

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
        "to": format!("{:#x}", pay_to),
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
            to: format!("{:#x}", pay_to),
            value: value.to_string(),
            validAfter: valid_after.to_string(),
            validBefore: valid_before.to_string(),
            nonce: format!("0x{}", hex::encode(nonce)),
        },
    };

    // Get current network from config
    let cfg = crate::evm::load_network().await?;
    let network = cfg.network.clone();

    // Build x402 payment header
    let payment_header = PaymentPayload {
        x402Version: 1,
        scheme: "exact".to_string(),
        network,
        payload: serde_json::to_value(payload)?,
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