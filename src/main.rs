//! x402-wallet: CLI wallet for x402 payment protocol
//!
//! This wallet creates EIP-3009 payment authorizations for x402-protected APIs
//! and provides basic EVM wallet functionality (balance checks, transfers).

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod evm;
mod store;
mod utils;
mod x402;

#[derive(Parser, Debug)]
#[command(
    name = "x402-wallet",
    version,
    about = "x402-capable EVM wallet CLI",
    long_about = "A command-line wallet for creating x402 payments and managing EVM accounts on Ethereum, Base, and Base Sepolia networks."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Initialize a new wallet (create or import private key)
    WalletInit {
        /// Path to .env file (default: ./.env)
        #[arg(long)]
        dotenv: Option<PathBuf>,
        /// Use encrypted keystore instead of .env file
        #[arg(long)]
        keystore: bool,
    },

    /// Display wallet address
    WalletAddress,

    /// Configure network settings
    ConfigSet {
        /// Network name: ethereum, base, or base-sepolia
        #[arg(long)]
        network: String,
        /// Custom RPC URL (optional)
        #[arg(long)]
        rpc: Option<String>,
    },

    /// Check wallet balance
    Balance {
        /// ERC20 token contract address (if omitted, shows ETH balance)
        #[arg(long)]
        erc20: Option<String>,
    },

    /// Send ETH to an address
    SendEth {
        /// Recipient address
        #[arg(long)]
        to: String,
        /// Amount in ETH (e.g., "0.1")
        #[arg(long)]
        eth: String,
    },

    /// Send ERC20 tokens to an address
    SendErc20 {
        /// Token contract address
        #[arg(long)]
        token: String,
        /// Recipient address
        #[arg(long)]
        to: String,
        /// Amount in token units (e.g., "10.5")
        #[arg(long)]
        amount: String,
    },

    /// Create x402 payment signature (outputs base64-encoded X-PAYMENT / PAYMENT-SIGNATURE header)
    CreatePayment {
        /// Recipient address (from 402 response: accepts[0].payTo)
        #[arg(long)]
        pay_to: String,
        /// Token contract address (from 402 response: accepts[0].asset)
        #[arg(long)]
        token: String,
        /// Amount in smallest units (v1: accepts[0].maxAmountRequired, v2: accepts[0].amount)
        #[arg(long)]
        amount: String,
        /// Token name for EIP-712 signing (from 402 response: accepts[0].extra.name)
        #[arg(long)]
        token_name: Option<String>,
        /// Token version for EIP-712 signing (from 402 response: accepts[0].extra.version)
        #[arg(long)]
        token_version: Option<String>,
        /// Emit x402 v2 payload for the PAYMENT-SIGNATURE header (default: v1 for X-PAYMENT)
        #[arg(long)]
        v2: bool,
        /// Resource URL embedded in the v2 payload (optional, v2 only)
        #[arg(long)]
        resource_url: Option<String>,
        /// maxTimeoutSeconds echoed in v2 accepted requirements
        /// (from 402 response: accepts[0].maxTimeoutSeconds, v2 only, default: 600)
        #[arg(long)]
        max_timeout_seconds: Option<u64>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::WalletInit { dotenv, keystore } => {
            store::init_wallet(dotenv, keystore).await
        }

        Cmd::WalletAddress => {
            let ctx = store::load_wallet_context().await?;
            println!("{:#x}", ctx.address);
            Ok(())
        }

        Cmd::ConfigSet { network, rpc } => {
            evm::save_network(&network, rpc.as_deref()).await?;
            println!("Network set to: {}", network.to_lowercase());
            if let Some(url) = rpc {
                println!("RPC URL: {}", url);
            }
            Ok(())
        }

        Cmd::Balance { erc20 } => {
            let provider = evm::http_provider().await?;
            let ctx = store::load_wallet_context().await?;

            if let Some(token) = erc20 {
                let bal = evm::erc20_balance(&provider, &ctx.address, &token).await?;
                println!("{}", bal);
            } else {
                let bal = evm::eth_balance(&provider, &ctx.address).await?;
                println!("{}", bal);
            }
            Ok(())
        }

        Cmd::SendEth { to, eth } => {
            let ctx = store::load_wallet_context().await?;
            let client = evm::provider_with_wallet(ctx.wallet).await?;
            let tx_hash = evm::send_eth(client, &to, &eth).await?;
            println!("Transaction hash: {}", tx_hash);
            Ok(())
        }

        Cmd::SendErc20 { token, to, amount } => {
            let ctx = store::load_wallet_context().await?;
            let client = evm::provider_with_wallet(ctx.wallet).await?;
            let tx_hash = evm::send_erc20(client, &token, &to, &amount).await?;
            println!("Transaction hash: {}", tx_hash);
            Ok(())
        }

        Cmd::CreatePayment {
            pay_to,
            token,
            amount,
            token_name,
            token_version,
            v2,
            resource_url,
            max_timeout_seconds,
        } => {
            let ctx = store::load_wallet_context().await?;
            let client = evm::provider_with_wallet(ctx.wallet.clone()).await?;

            let header = x402::create_payment(
                client,
                &ctx.wallet,
                &pay_to,
                &token,
                &amount,
                token_name.as_deref(),
                token_version.as_deref(),
                v2,
                resource_url.as_deref(),
                max_timeout_seconds,
            )
            .await?;

            // Output only the base64 header to stdout (no extra text)
            println!("{}", header);
            Ok(())
        }
    }
}