//! EVM network configuration and blockchain interaction utilities

use anyhow::{anyhow, Result};
use ethers::{
    prelude::*,
    providers::{Http, Provider},
    types::{Address, TransactionRequest},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, str::FromStr, sync::Arc};

use crate::utils::home_dir;

/// Network configuration (stored in ~/.x402wallet/config.json)
#[derive(Serialize, Deserialize, Clone)]
pub struct NetCfg {
    /// Active network: "ethereum", "base", or "base-sepolia"
    pub network: String,
    /// RPC URLs for each network
    pub rpc: HashMap<String, String>,
}

/// Returns path to config file (~/.x402wallet/config.json)
fn cfg_path() -> Result<PathBuf> {
    let mut p = home_dir()?;
    p.push(".x402wallet/config.json");
    std::fs::create_dir_all(p.parent().unwrap())?;
    Ok(p)
}

/// Default RPC endpoints for supported networks
fn default_rpc_map() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("ethereum".into(), "https://cloudflare-eth.com".into());
    m.insert("base".into(), "https://mainnet.base.org".into());
    m.insert("base-sepolia".into(), "https://sepolia.base.org".into());
    m
}

/// Default configuration (Base mainnet)
fn default_cfg() -> NetCfg {
    NetCfg {
        network: "base".into(),
        rpc: default_rpc_map(),
    }
}

/// Save network configuration
pub async fn save_network(network: &str, rpc: Option<&str>) -> Result<()> {
    let mut cfg = if cfg_path()?.exists() {
        load_network().await?
    } else {
        default_cfg()
    };

    // Normalize network name
    let net = match network.to_lowercase().as_str() {
        "eth" | "ethereum" => "ethereum",
        "base" => "base",
        "base-sepolia" | "base_sepolia" | "base-sepolia-testnet" => "base-sepolia",
        other => return Err(anyhow!("unknown network: {}", other)),
    };

    cfg.network = net.to_string();

    // Update RPC if provided
    if let Some(url) = rpc {
        cfg.rpc.insert(net.to_string(), url.to_string());
    }

    // Save to disk
    let path = cfg_path()?;
    fs::write(path, serde_json::to_vec_pretty(&cfg)?)?;
    Ok(())
}

/// Load network configuration (creates default if missing)
pub async fn load_network() -> Result<NetCfg> {
    let path = cfg_path()?;
    if !path.exists() {
        let cfg = default_cfg();
        fs::write(&path, serde_json::to_vec_pretty(&cfg)?)?;
        return Ok(cfg);
    }
    let data = fs::read(path)?;
    Ok(serde_json::from_slice(&data)?)
}

/// Get chain ID for configured network
pub async fn chain_id() -> Result<u64> {
    Ok(match load_network().await?.network.as_str() {
        "ethereum" => 1u64,
        "base" => 8453u64,
        "base-sepolia" => 84532u64,
        other => return Err(anyhow!("unknown network: {}", other)),
    })
}

/// Returns the CAIP-2 network identifier used by x402 v2 (e.g. "eip155:8453")
pub fn caip2_for_network(name: &str) -> Result<String> {
    Ok(match name {
        "ethereum" => "eip155:1",
        "base" => "eip155:8453",
        "base-sepolia" => "eip155:84532",
        other => return Err(anyhow!("unknown network: {}", other)),
    }
    .to_string())
}

/// Create HTTP provider for configured network (validates chain ID)
pub async fn http_provider() -> Result<Provider<Http>> {
    let cfg = load_network().await?;
    let net = cfg.network.clone();
    let url = cfg
        .rpc
        .get(&net)
        .cloned()
        .ok_or_else(|| anyhow!("no RPC configured for network: {}", net))?;

    let provider = Provider::<Http>::try_from(url.as_str())?;

    // Verify RPC is on correct chain
    let rpc_chain = provider.get_chainid().await?.as_u64();
    let expected_chain = chain_id().await?;
    if rpc_chain != expected_chain {
        return Err(anyhow!(
            "RPC chain ID mismatch: got {}, expected {} for network '{}'",
            rpc_chain,
            expected_chain,
            net
        ));
    }

    Ok(provider)
}

/// Create provider with wallet signer
pub async fn provider_with_wallet(
    wallet: Wallet<k256::ecdsa::SigningKey>,
) -> Result<Arc<SignerMiddleware<Provider<Http>, Wallet<k256::ecdsa::SigningKey>>>> {
    let provider = http_provider().await?;
    Ok(Arc::new(SignerMiddleware::new(provider, wallet)))
}

/// Get ETH balance for address
pub async fn eth_balance(provider: &Provider<Http>, addr: &Address) -> Result<String> {
    let bal = provider.get_balance(*addr, None).await?;
    Ok(ethers::utils::format_units(bal, 18)?)
}

// ERC20 ABI for balance checks and transfers
abigen!(
    ERC20,
    r#"[
        function decimals() view returns (uint8)
        function balanceOf(address) view returns (uint256)
        function transfer(address to, uint256 amount) returns (bool)
    ]"#
);

/// Get ERC20 token balance for address
pub async fn erc20_balance(provider: &Provider<Http>, addr: &Address, token: &str) -> Result<String> {
    let token_contract = ERC20::new(Address::from_str(token)?, Arc::new(provider.clone()));
    let decimals = token_contract.decimals().call().await?;
    let raw_balance = token_contract.balance_of(*addr).call().await?;
    Ok(ethers::utils::format_units(raw_balance, decimals as u32)?)
}

/// Send ETH transaction
pub async fn send_eth(
    client: Arc<SignerMiddleware<Provider<Http>, Wallet<k256::ecdsa::SigningKey>>>,
    to: &str,
    eth: &str,
) -> Result<String> {
    let to_addr = Address::from_str(to)?;
    let value = ethers::utils::parse_ether(eth)?;

    let tx = TransactionRequest::new().to(to_addr).value(value);
    let pending = client.send_transaction(tx, None).await?;
    let receipt = pending
        .await?
        .ok_or_else(|| anyhow!("transaction dropped from mempool"))?;

    Ok(format!("{:?}", receipt.transaction_hash))
}

/// Send ERC20 token transaction
pub async fn send_erc20(
    client: Arc<SignerMiddleware<Provider<Http>, Wallet<k256::ecdsa::SigningKey>>>,
    token: &str,
    to: &str,
    amount: &str,
) -> Result<String> {
    let token_addr = Address::from_str(token)?;
    let to_addr = Address::from_str(to)?;

    let contract = ERC20::new(token_addr, client.clone());
    let decimals = contract.decimals().call().await?;
    let raw_amount = ethers::utils::parse_units(amount, decimals as u32)?;

    let call = contract.transfer(to_addr, raw_amount.into());
    let pending = call.send().await?;
    let receipt = pending
        .await?
        .ok_or_else(|| anyhow!("transaction dropped from mempool"))?;

    Ok(format!("{:?}", receipt.transaction_hash))
}