//! EVM network configuration and blockchain interaction utilities

use alloy::network::{Ethereum, TransactionBuilder};
use alloy::primitives::{
    utils::format_units, utils::parse_ether, utils::parse_units, utils::ParseUnits, Address,
};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, str::FromStr};
use url::Url;

use crate::utils::home_dir;

/// Network configuration (stored in ~/.x402wallet/config.json)
#[derive(Serialize, Deserialize, Clone)]
pub struct NetCfg {
    /// Active network: "ethereum", "base", "base-sepolia", or "polygon"
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
    m.insert(
        "polygon".into(),
        "https://polygon-bor-rpc.publicnode.com".into(),
    );
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
        "polygon" | "matic" => "polygon",
        other => return Err(anyhow!("unknown network: {}", other)),
    };

    cfg.network = net.to_string();

    // Ensure the selected network has an RPC entry — migrates configs that
    // were created before this network was supported
    if !cfg.rpc.contains_key(net) {
        if let Some(url) = default_rpc_map().get(net) {
            cfg.rpc.insert(net.to_string(), url.clone());
        }
    }

    // Update RPC if provided. Validated at ingress: an unvalidated URL
    // persisted here is the source of every later request (see ADR-0005).
    if let Some(url) = rpc {
        let parsed = Url::parse(url)?;
        validate_rpc_url(&parsed)?;
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
        "polygon" => 137u64,
        other => return Err(anyhow!("unknown network: {}", other)),
    })
}

/// Returns the CAIP-2 network identifier used by x402 v2 (e.g. "eip155:8453")
pub fn caip2_for_network(name: &str) -> Result<String> {
    Ok(match name {
        "ethereum" => "eip155:1",
        "base" => "eip155:8453",
        "base-sepolia" => "eip155:84532",
        "polygon" => "eip155:137",
        other => return Err(anyhow!("unknown network: {}", other)),
    }
    .to_string())
}

/// Resolve the configured RPC URL and the chain ID the network should have
async fn rpc_url_and_expected_chain() -> Result<(Url, String, u64)> {
    let cfg = load_network().await?;
    let net = cfg.network.clone();
    let url = cfg
        .rpc
        .get(&net)
        .ok_or_else(|| anyhow!("no RPC configured for network: {}", net))?;
    let expected_chain = chain_id().await?;
    let url = Url::parse(url)?;
    // Validated at egress too: covers configs written before this check
    // existed and hand-edited config.json files (see ADR-0005).
    validate_rpc_url(&url)?;
    Ok((url, net, expected_chain))
}

/// Validates an RPC endpoint before it is stored or dialed (ADR-0005).
///
/// Custom RPC URLs are user-supplied (`config-set --rpc` / config.json) and
/// are the sink of every request the wallet makes — signed payloads, balance
/// queries, chain-id checks — so they are treated as untrusted input:
/// https only, with a loopback exception for local dev nodes (anvil/geth on
/// 127.0.0.1, [::1] or localhost). Cleartext http to any other host leaks
/// request data on the wire and is rejected.
fn validate_rpc_url(url: &Url) -> Result<()> {
    let is_loopback = match url.host() {
        Some(url::Host::Domain(h)) => h == "localhost" || h.ends_with(".localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    let host = url.host_str().unwrap_or("<no host>");
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback => Ok(()),
        "http" => Err(anyhow!(
            "refusing cleartext http RPC for '{host}' — use https \
             (http is only allowed for localhost dev nodes)"
        )),
        other => Err(anyhow!("unsupported RPC URL scheme '{other}' — use https")),
    }
}

/// Create HTTP provider for configured network (validates chain ID)
pub async fn http_provider() -> Result<impl Provider<Ethereum> + Clone + Send + Sync + 'static> {
    let (url, net, expected_chain) = rpc_url_and_expected_chain().await?;

    let provider = ProviderBuilder::new().connect_http(url);

    // Verify RPC is on correct chain
    let rpc_chain = provider.get_chain_id().await?;
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
    wallet: alloy::signers::local::PrivateKeySigner,
) -> Result<impl Provider<Ethereum> + Clone + Send + Sync + 'static> {
    let (url, net, expected_chain) = rpc_url_and_expected_chain().await?;

    let provider = ProviderBuilder::new().wallet(wallet).connect_http(url);

    // Verify RPC is on correct chain
    let rpc_chain = provider.get_chain_id().await?;
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

/// Get ETH balance for address
pub async fn eth_balance<P: Provider<Ethereum>>(provider: &P, addr: &Address) -> Result<String> {
    let bal = provider.get_balance(*addr).await?;
    Ok(format_units(bal, "ether")?)
}

// ERC20 ABI for balance checks and transfers
sol! {
    #[derive(Debug)]
    #[sol(rpc)]
    interface IERC20 {
        function decimals() external view returns (uint8);
        function balanceOf(address account) external view returns (uint256);
        function transfer(address to, uint256 amount) external returns (bool);
    }
}

/// Get ERC20 token balance for address
pub async fn erc20_balance<P: Provider<Ethereum> + Clone>(
    provider: &P,
    addr: &Address,
    token: &str,
) -> Result<String> {
    let token_contract = IERC20::new(Address::from_str(token)?, provider);
    let decimals = token_contract.decimals().call().await?;
    let raw_balance = token_contract.balanceOf(*addr).call().await?;
    Ok(format_units(raw_balance, decimals)?)
}

/// Send ETH transaction
pub async fn send_eth<P: Provider<Ethereum> + Clone + 'static>(
    client: P,
    to: &str,
    eth: &str,
) -> Result<String> {
    let to_addr = Address::from_str(to)?;
    let value = parse_ether(eth)?;

    let tx = TransactionRequest::default()
        .with_to(to_addr)
        .with_value(value);
    let receipt = client
        .send_transaction(tx)
        .await?
        .get_receipt()
        .await
        .map_err(|_| anyhow!("transaction dropped from mempool"))?;

    Ok(format!("{:?}", receipt.transaction_hash))
}

/// Send ERC20 token transaction
pub async fn send_erc20<P: Provider<Ethereum> + Clone + 'static>(
    client: P,
    token: &str,
    to: &str,
    amount: &str,
) -> Result<String> {
    let token_addr = Address::from_str(token)?;
    let to_addr = Address::from_str(to)?;

    let contract = IERC20::new(token_addr, client);
    let decimals = contract.decimals().call().await?;
    let raw_amount = match parse_units(amount, decimals)? {
        ParseUnits::U256(v) => v,
        ParseUnits::I256(_) => return Err(anyhow!("negative amounts are not supported")),
    };

    let pending = contract.transfer(to_addr, raw_amount).send().await?;
    let receipt = pending
        .get_receipt()
        .await
        .map_err(|_| anyhow!("transaction dropped from mempool"))?;

    Ok(format!("{:?}", receipt.transaction_hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(url: &str) -> Result<()> {
        validate_rpc_url(&Url::parse(url).unwrap())
    }

    #[test]
    fn https_rpc_urls_are_accepted() {
        assert!(check("https://polygon-bor-rpc.publicnode.com").is_ok());
        assert!(check("https://mainnet.base.org").is_ok());
        assert!(check("https://rpc.example/some/path?key=abc").is_ok());
    }

    #[test]
    fn http_is_only_allowed_for_loopback_dev_nodes() {
        assert!(check("http://127.0.0.1:8545").is_ok());
        assert!(check("http://localhost:8545").is_ok());
        assert!(check("http://[::1]:8545").is_ok());
        // Private/LAN and public cleartext endpoints are rejected
        assert!(check("http://192.168.1.10:8545").is_err());
        assert!(check("http://10.0.0.5:8545").is_err());
        assert!(check("http://rpc.example.com").is_err());
    }

    #[test]
    fn non_http_schemes_are_rejected() {
        assert!(check("ws://127.0.0.1:8545").is_err());
        assert!(check("ftp://rpc.example").is_err());
        assert!(check("file:///etc/passwd").is_err());
    }

    #[test]
    fn default_rpc_map_entries_pass_validation() {
        for url in default_rpc_map().values() {
            let parsed = Url::parse(url).unwrap();
            validate_rpc_url(&parsed)
                .unwrap_or_else(|e| panic!("default RPC {url} must stay valid: {e}"));
        }
    }
}
